//! Shared rollup, summary, integration, and job repository contracts.

use std::{error::Error, time::SystemTime};

use pvlog_domain::{AccountId, AlertRuleId, JobId, ProviderId, SystemId, WebhookSubscriptionId};
use pvlog_storage::{
    AccountConfigurationRepository, AlertRuleRecord, DailySummaryRecord, DatabaseTarget, JobRecord,
    JobRetryDisposition, LifetimeSummaryRecord, OperationalRepository, OperationalRepositoryError,
    PostgresAccountConfigurationRepository, PostgresOperationalRepository, ProviderRecord,
    RollupRecord, SqliteAccountConfigurationRepository, SqliteAccountPoolConfig,
    SqliteAccountPoolRouter, SqliteAccountProvisioner, SqliteOperationalRepository,
    SystemConfigurationRecord, WebhookSubscriptionRecord, apply_migrations,
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
    verify_contract(&repository_a, &repository_b, &configuration).await
}

#[tokio::test]
async fn postgres_operational_repository_contract_when_configured() -> Result<(), Box<dyn Error>> {
    let Ok(url) = std::env::var("TEST_POSTGRES_URL") else {
        return Ok(());
    };
    apply_migrations(&DatabaseTarget::Postgres { url: url.clone() }).await?;
    let account_a = create_postgres_account(&url, "a").await?;
    let account_b = create_postgres_account(&url, "b").await?;
    let repository_a = PostgresOperationalRepository::new(url.clone(), account_a);
    let repository_b = PostgresOperationalRepository::new(url.clone(), account_b);
    let configuration = PostgresAccountConfigurationRepository::new(url, account_a);
    verify_contract(&repository_a, &repository_b, &configuration).await
}

#[allow(clippy::too_many_lines)]
async fn verify_contract(
    repository: &dyn OperationalRepository,
    other_account: &dyn OperationalRepository,
    configuration: &dyn AccountConfigurationRepository,
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

    let lease_a = repository
        .lease_job("worker-a", base, base + 100)
        .await?
        .ok_or("expected first lease")?;
    assert_eq!(lease_a.job.id, job.id);
    assert!(
        repository
            .heartbeat_job(job.id, "worker-a", base + 10, base + 200)
            .await?
    );
    assert!(
        repository
            .lease_job("worker-b", base + 199, base + 300)
            .await?
            .is_none()
    );
    let lease_b = repository
        .lease_job("worker-b", base + 201, base + 400)
        .await?
        .ok_or("expired lease must recover after worker restart")?;
    assert_eq!(lease_b.job.attempt_count, 2);
    assert!(
        repository
            .complete_job(job.id, "worker-b", base + 210)
            .await?
    );
    assert!(
        repository
            .complete_job(job.id, "worker-b", base + 211)
            .await?,
        "handler completion is idempotent"
    );

    let dead = JobRecord {
        id: JobId::new(),
        job_kind: "always_fails".to_owned(),
        state: "pending".to_owned(),
        payload: serde_json::json!({}),
        idempotency_key: Some(format!("dead-{base}")),
        priority: 1,
        attempt_count: 0,
        max_attempts: 2,
        available_at: base,
        created_at: base,
        updated_at: base,
    };
    repository.save_job(&dead).await?;
    let dead_lease = repository
        .lease_job("worker-c", base + 300, base + 400)
        .await?
        .ok_or("expected failing job lease")?;
    assert_eq!(dead_lease.job.id, dead.id);
    let JobRetryDisposition::RetryAt(retry_at) = repository
        .retry_job(
            dead.id,
            "worker-c",
            base + 310,
            100,
            10_000,
            "fixture_failure",
        )
        .await?
    else {
        return Err("first bounded retry unexpectedly dead-lettered".into());
    };
    assert!((base + 410..=base + 435).contains(&retry_at));
    assert!(
        repository
            .lease_job("worker-d", retry_at - 1, retry_at + 100)
            .await?
            .is_none()
    );
    repository
        .lease_job("worker-d", retry_at, retry_at + 100)
        .await?
        .ok_or("retry did not become available")?;
    assert_eq!(
        repository
            .retry_job(
                dead.id,
                "worker-d",
                retry_at + 1,
                100,
                10_000,
                "fixture_failure"
            )
            .await?,
        JobRetryDisposition::DeadLetter
    );
    assert!(
        repository
            .dead_letter_jobs(10)
            .await?
            .iter()
            .any(|item| item.id == dead.id)
    );
    assert!(repository.requeue_job(dead.id, retry_at + 2).await?);
    assert_eq!(
        repository.job(dead.id).await?.map(|job| job.state),
        Some("pending".to_owned())
    );
    assert!(repository.cancel_job(dead.id, retry_at + 3).await?);
    assert_eq!(
        repository.job(dead.id).await?.map(|job| job.state),
        Some("cancelled".to_owned())
    );
    assert!(repository.requeue_job(dead.id, retry_at + 4).await?);

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
