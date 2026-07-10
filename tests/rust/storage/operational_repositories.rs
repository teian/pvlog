//! Shared rollup, summary, community, integration, and job repository contracts.

use std::{error::Error, time::SystemTime};

use pvlog_domain::{
    AccountId, AlertRuleId, JobId, ProviderId, SystemId, TeamId, UserId, WebhookSubscriptionId,
};
use pvlog_storage::{
    AccountConfigurationRepository, AlertRuleRecord, DailySummaryRecord, DatabaseTarget, JobRecord,
    LifetimeSummaryRecord, OperationalRepository, OperationalRepositoryError,
    PostgresAccountConfigurationRepository, PostgresOperationalRepository, ProviderRecord,
    RollupRecord, SqliteAccountConfigurationRepository, SqliteAccountPoolConfig,
    SqliteAccountPoolRouter, SqliteAccountProvisioner, SqliteOperationalRepository,
    SystemConfigurationRecord, TeamRecord, TeamRollupRecord, WebhookSubscriptionRecord,
    apply_migrations,
};
use sqlx::{Connection as _, PgConnection, SqliteConnection, sqlite::SqliteConnectOptions};
use tempfile::TempDir;

#[tokio::test]
async fn sqlite_operational_repository_contract() -> Result<(), Box<dyn Error>> {
    let directory = TempDir::new()?;
    let management = directory.path().join("management.sqlite3");
    let accounts = directory.path().join("accounts");
    apply_migrations(&DatabaseTarget::Sqlite {
        management_path: management.clone(),
        accounts_dir: accounts.clone(),
    })
    .await?;
    let owner = create_sqlite_user(&management).await?;
    let account_a = create_sqlite_account(&management, &accounts, "a").await?;
    let account_b = create_sqlite_account(&management, &accounts, "b").await?;
    let router = SqliteAccountPoolRouter::new(
        management.clone(),
        accounts,
        SqliteAccountPoolConfig::default(),
    )?;
    let routed_a = router.route(account_a).await?;
    let repository_a = SqliteOperationalRepository::new(management.clone(), routed_a.clone());
    let repository_b = SqliteOperationalRepository::new(management, router.route(account_b).await?);
    let configuration = SqliteAccountConfigurationRepository::new(routed_a);
    verify_contract(&repository_a, &repository_b, &configuration, owner).await
}

#[tokio::test]
async fn postgres_operational_repository_contract_when_configured() -> Result<(), Box<dyn Error>> {
    let Ok(url) = std::env::var("TEST_POSTGRES_URL") else {
        return Ok(());
    };
    apply_migrations(&DatabaseTarget::Postgres { url: url.clone() }).await?;
    let owner = create_postgres_user(&url).await?;
    let account_a = create_postgres_account(&url, "a").await?;
    let account_b = create_postgres_account(&url, "b").await?;
    let repository_a = PostgresOperationalRepository::new(url.clone(), account_a);
    let repository_b = PostgresOperationalRepository::new(url.clone(), account_b);
    let configuration = PostgresAccountConfigurationRepository::new(url, account_a);
    verify_contract(&repository_a, &repository_b, &configuration, owner).await
}

#[allow(clippy::too_many_lines)]
async fn verify_contract(
    repository: &dyn OperationalRepository,
    other_account: &dyn OperationalRepository,
    configuration: &dyn AccountConfigurationRepository,
    owner: UserId,
) -> Result<(), Box<dyn Error>> {
    assert_ne!(repository.account_id(), other_account.account_id());
    let system_id = SystemId::new();
    configuration.save_system(&system(system_id)).await?;
    let base = epoch_millis()?;

    let rollup = RollupRecord {
        system_id,
        resolution: "hour".to_owned(),
        bucket_start: base,
        bucket_end: base + 3_600_000,
        timezone: "UTC".to_owned(),
        generation: 1,
        point_count: 12,
        expected_count: 12,
        generation_energy_wh: Some(2_400),
        quality_flags: 0,
        coverage_basis_points: 10_000,
        calculated_at: base + 1,
    };
    repository.save_rollup(&rollup).await?;
    assert_eq!(
        repository.rollups(system_id, base, base + 1).await?,
        vec![rollup]
    );
    assert!(
        other_account
            .rollups(system_id, base, base + 1)
            .await?
            .is_empty()
    );

    let daily = DailySummaryRecord {
        system_id,
        local_date: "2026-07-10".to_owned(),
        timezone: "UTC".to_owned(),
        generation: 1,
        generation_energy_wh: Some(12_000),
        consumption_energy_wh: Some(6_000),
        coverage_basis_points: 9_900,
        quality_flags: 1,
        calculated_at: base,
    };
    repository.save_daily_summary(&daily).await?;
    assert_eq!(
        repository
            .daily_summary(system_id, &daily.local_date)
            .await?,
        Some(daily)
    );
    assert!(
        other_account
            .daily_summary(system_id, "2026-07-10")
            .await?
            .is_none()
    );

    let lifetime = LifetimeSummaryRecord {
        system_id,
        generation: 2,
        first_observation_at: Some(base - 1_000),
        last_observation_at: Some(base),
        generation_energy_wh: Some(25_000),
        consumption_energy_wh: Some(11_000),
        coverage_basis_points: 9_800,
        calculated_at: base + 2,
    };
    repository.save_lifetime_summary(&lifetime).await?;
    assert_eq!(
        repository.lifetime_summary(system_id).await?,
        Some(lifetime)
    );
    assert!(other_account.lifetime_summary(system_id).await?.is_none());

    let team = TeamRecord {
        id: TeamId::new(),
        account_id: repository.account_id(),
        name: format!("Contract team {base}"),
        visibility: "public".to_owned(),
        owner_user_id: owner,
        created_at: base,
        updated_at: base,
    };
    repository.save_team(&team).await?;
    assert_eq!(repository.team(team.id).await?, Some(team.clone()));
    assert!(other_account.team(team.id).await?.is_none());
    let team_rollup = TeamRollupRecord {
        team_id: team.id,
        team_account_id: repository.account_id(),
        period_start: base,
        period_end: base + 86_400_000,
        generation_energy_wh: 12_000,
        normalized_generation_wh_per_kw: Some(1_500),
        coverage_basis_points: 9_700,
        source_sequence: 1,
        projected_at: base + 3,
    };
    repository.save_team_rollup(&team_rollup).await?;
    assert_eq!(
        repository.team_rollups(team.id, base, base + 1).await?,
        vec![team_rollup]
    );

    let alert = AlertRuleRecord {
        id: AlertRuleId::new(),
        system_id,
        name: format!("Low production {base}"),
        alert_kind: "generation".to_owned(),
        enabled: true,
        condition: serde_json::json!({"belowWatts": 100}),
        schedule: serde_json::json!({"timezone": "UTC"}),
        debounce_seconds: 300,
        cooldown_seconds: 900,
        created_at: base,
        updated_at: base,
    };
    repository.save_alert(&alert).await?;
    assert_eq!(repository.alert(alert.id).await?, Some(alert.clone()));
    assert!(other_account.alert(alert.id).await?.is_none());

    let webhook = WebhookSubscriptionRecord {
        id: WebhookSubscriptionId::new(),
        name: format!("Operations {base}"),
        endpoint_url: "https://example.invalid/hooks/pv".to_owned(),
        state: "active".to_owned(),
        event_types: serde_json::json!(["alert.triggered"]),
        encryption_key_id: "key-1".to_owned(),
        encrypted_signing_secret: vec![1, 2, 3, 4],
        created_at: base,
        updated_at: base,
    };
    repository.save_webhook(&webhook).await?;
    assert_eq!(repository.webhook(webhook.id).await?, Some(webhook.clone()));
    assert!(other_account.webhook(webhook.id).await?.is_none());

    let provider = ProviderRecord {
        id: ProviderId::new(),
        provider_kind: "weather".to_owned(),
        name: format!("Forecast {base}"),
        enabled: true,
        endpoint_url: Some("https://example.invalid/weather".to_owned()),
        credential_secret_ref: Some("secret://weather/primary".to_owned()),
        configuration: serde_json::json!({"timeoutSeconds": 5}),
        license_metadata: serde_json::json!({"attribution": "fixture"}),
        circuit_state: "closed".to_owned(),
        created_at: base,
        updated_at: base,
    };
    repository.save_provider(&provider).await?;
    assert_eq!(
        repository.provider(provider.id).await?,
        Some(provider.clone())
    );
    assert!(other_account.provider(provider.id).await?.is_none());

    let job = JobRecord {
        id: JobId::new(),
        job_kind: "recompute_rollups".to_owned(),
        state: "pending".to_owned(),
        payload: serde_json::json!({"systemId": system_id}),
        idempotency_key: Some(format!("rollup-{base}")),
        priority: 10,
        attempt_count: 0,
        max_attempts: 5,
        available_at: base,
        created_at: base,
        updated_at: base,
    };
    repository.save_job(&job).await?;
    assert_eq!(repository.job(job.id).await?, Some(job.clone()));
    assert!(other_account.job(job.id).await?.is_none());

    assert!(matches!(
        repository.rollups(system_id, base, base).await,
        Err(OperationalRepositoryError::InvalidPeriod)
    ));
    Ok(())
}

fn system(id: SystemId) -> SystemConfigurationRecord {
    SystemConfigurationRecord {
        id,
        name: "Operational contract".to_owned(),
        description: String::new(),
        timezone: "UTC".to_owned(),
        visibility: "private".to_owned(),
        lifecycle: "active".to_owned(),
        status_interval_seconds: 300,
        power_calculation_mode: "reported".to_owned(),
        net_calculation_mode: "separate_flows".to_owned(),
        created_at: 1,
        updated_at: 1,
    }
}

fn epoch_millis() -> Result<i64, Box<dyn Error>> {
    Ok(i64::try_from(
        SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_millis(),
    )?)
}

async fn create_sqlite_user(path: &std::path::Path) -> Result<UserId, Box<dyn Error>> {
    let id = UserId::new();
    let mut connection = sqlite_connection(path).await?;
    sqlx::query("INSERT INTO users (id,email,display_name,status,created_at,updated_at) VALUES (?,?,?,'active',1,1)")
        .bind(id.as_uuid().as_bytes().as_slice())
        .bind(format!("owner-{id}@example.invalid"))
        .bind("Contract owner")
        .execute(&mut connection)
        .await?;
    connection.close().await?;
    Ok(id)
}

async fn create_sqlite_account(
    management: &std::path::Path,
    accounts: &std::path::Path,
    label: &str,
) -> Result<AccountId, Box<dyn Error>> {
    let id = AccountId::new();
    let mut connection = sqlite_connection(management).await?;
    sqlx::query("INSERT INTO accounts (id,slug,display_name,status,created_at,updated_at) VALUES (?,?,?,'provisioning',1,1)")
        .bind(id.as_uuid().as_bytes().as_slice())
        .bind(format!("operational-{label}-{id}"))
        .bind(format!("Account {label}"))
        .execute(&mut connection)
        .await?;
    connection.close().await?;
    SqliteAccountProvisioner::new(management.to_owned(), accounts.to_owned())
        .provision(id)
        .await?;
    Ok(id)
}

async fn sqlite_connection(path: &std::path::Path) -> Result<SqliteConnection, sqlx::Error> {
    SqliteConnection::connect_with(
        &SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(false)
            .foreign_keys(true),
    )
    .await
}

async fn create_postgres_user(url: &str) -> Result<UserId, Box<dyn Error>> {
    let id = UserId::new();
    let mut connection = PgConnection::connect(url).await?;
    sqlx::query("INSERT INTO management.users (id,email,display_name,status,created_at,updated_at) VALUES ($1,$2,$3,'active',1,1)")
        .bind(id.as_uuid())
        .bind(format!("owner-{id}@example.invalid"))
        .bind("Contract owner")
        .execute(&mut connection)
        .await?;
    connection.close().await?;
    Ok(id)
}

async fn create_postgres_account(url: &str, label: &str) -> Result<AccountId, Box<dyn Error>> {
    let id = AccountId::new();
    let mut connection = PgConnection::connect(url).await?;
    sqlx::query("INSERT INTO management.accounts (id,slug,display_name,status,created_at,updated_at) VALUES ($1,$2,$3,'active',1,1)")
        .bind(id.as_uuid())
        .bind(format!("operational-{label}-{id}"))
        .bind(format!("Account {label}"))
        .execute(&mut connection)
        .await?;
    connection.close().await?;
    Ok(id)
}
