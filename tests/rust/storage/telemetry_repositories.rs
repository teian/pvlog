//! Shared hot telemetry, idempotency, correction, and isolation contracts.

use std::{error::Error, time::SystemTime};

use pvlog_domain::{AccountId, CorrectionId, ObservationId, SystemId};
use pvlog_storage::{
    AccountConfigurationRepository, CorrectionRecord, DatabaseTarget, IdempotencyOutcome,
    IdempotencyRecord, ObservationInsertOutcome, PostgresAccountConfigurationRepository,
    PostgresTelemetryRepository, SqliteAccountConfigurationRepository, SqliteAccountPoolConfig,
    SqliteAccountPoolRouter, SqliteAccountProvisioner, SqliteTelemetryRepository,
    StoredObservation, SystemConfigurationRecord, TelemetryRepository, TelemetryRepositoryError,
    apply_migrations,
};
use sqlx::{Connection as _, PgConnection, SqliteConnection, sqlite::SqliteConnectOptions};
use tempfile::TempDir;
use uuid::Uuid;

#[tokio::test]
async fn sqlite_telemetry_repository_contract() -> Result<(), Box<dyn Error>> {
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
    let router =
        SqliteAccountPoolRouter::new(management, accounts, SqliteAccountPoolConfig::default())?;
    let routed_a = router.route(account_a).await?;
    let routed_b = router.route(account_b).await?;
    let telemetry_a = SqliteTelemetryRepository::new(routed_a.clone());
    let telemetry_b = SqliteTelemetryRepository::new(routed_b.clone());
    let configuration_a = SqliteAccountConfigurationRepository::new(routed_a);
    verify_contract(&telemetry_a, &telemetry_b, &configuration_a).await
}

#[tokio::test]
async fn postgres_telemetry_repository_contract_when_configured() -> Result<(), Box<dyn Error>> {
    let Ok(url) = std::env::var("TEST_POSTGRES_URL") else {
        return Ok(());
    };
    apply_migrations(&DatabaseTarget::Postgres { url: url.clone() }).await?;
    let account_a = create_postgres_account(&url, "a").await?;
    let account_b = create_postgres_account(&url, "b").await?;
    let telemetry_a = PostgresTelemetryRepository::new(url.clone(), account_a);
    let telemetry_b = PostgresTelemetryRepository::new(url.clone(), account_b);
    let configuration_a = PostgresAccountConfigurationRepository::new(url, account_a);
    verify_contract(&telemetry_a, &telemetry_b, &configuration_a).await
}

#[allow(clippy::too_many_lines)]
async fn verify_contract(
    repository: &dyn TelemetryRepository,
    other_account: &dyn TelemetryRepository,
    configuration: &dyn AccountConfigurationRepository,
) -> Result<(), Box<dyn Error>> {
    assert_ne!(repository.account_id(), other_account.account_id());
    let system_id = SystemId::new();
    configuration
        .save_system(&SystemConfigurationRecord {
            id: system_id,
            name: "Telemetry contract".to_owned(),
            description: String::new(),
            timezone: "UTC".to_owned(),
            visibility: "private".to_owned(),
            lifecycle: "active".to_owned(),
            status_interval_seconds: 300,
            power_calculation_mode: "reported".to_owned(),
            net_calculation_mode: "separate_flows".to_owned(),
            created_at: 1,
            updated_at: 1,
        })
        .await?;
    let base = epoch_millis()?;
    let first = observation(system_id, base, "source-1", [1_u8; 32]);
    let second = observation(system_id, base + 10, "source-2", [2_u8; 32]);
    assert_eq!(
        repository.insert_observation(&first).await?,
        ObservationInsertOutcome::Inserted
    );
    assert_eq!(
        repository.insert_observation(&first).await?,
        ObservationInsertOutcome::Duplicate
    );
    assert_eq!(
        repository.insert_observation(&second).await?,
        ObservationInsertOutcome::Inserted
    );
    let mut conflicting = first.clone();
    conflicting.id = ObservationId::new();
    conflicting.canonical_hash = [9_u8; 32];
    assert!(matches!(
        repository.insert_observation(&conflicting).await,
        Err(TelemetryRepositoryError::UniquenessConflict)
    ));
    assert_eq!(
        repository.observations(system_id, base, base + 10).await?,
        vec![first.clone()]
    );
    assert!(
        other_account
            .observations(system_id, base, base + 20)
            .await?
            .is_empty()
    );

    let idempotency = IdempotencyRecord {
        id: Uuid::now_v7(),
        principal_type: "user".to_owned(),
        principal_id: Uuid::now_v7(),
        operation: "telemetry.create".to_owned(),
        key: format!("key-{}", Uuid::now_v7()),
        request_hash: [3_u8; 32],
        response_status: 201,
        response: serde_json::json!({"observationId": first.id}),
        created_at: base,
        expires_at: base + 60_000,
    };
    assert_eq!(
        repository.store_idempotency(&idempotency).await?,
        IdempotencyOutcome::Stored
    );
    assert!(matches!(
        repository.store_idempotency(&idempotency).await?,
        IdempotencyOutcome::Replay(_)
    ));
    let mut idempotency_conflict = idempotency;
    idempotency_conflict.request_hash = [4_u8; 32];
    assert!(matches!(
        repository.store_idempotency(&idempotency_conflict).await,
        Err(TelemetryRepositoryError::IdempotencyConflict)
    ));

    let correction = CorrectionRecord {
        id: CorrectionId::new(),
        system_id,
        observation_id: first.id,
        measured_at: base,
        operation: "replace".to_owned(),
        expected_version: 1,
        replacement: Some(serde_json::json!({"generationPowerWatts": 999})),
        reason: "operator correction".to_owned(),
        actor_id: Some(Uuid::now_v7()),
        request_id: Some(Uuid::now_v7()),
        created_at: base + 1,
    };
    repository.append_correction(&correction).await?;
    assert!(matches!(
        repository.append_correction(&correction).await,
        Err(TelemetryRepositoryError::OptimisticConflict)
    ));
    assert_eq!(
        repository.corrections(system_id, base, base + 1).await?,
        vec![correction]
    );
    assert!(matches!(
        repository.observations(system_id, base, base).await,
        Err(TelemetryRepositoryError::InvalidRange)
    ));
    Ok(())
}

fn observation(
    system_id: SystemId,
    measured_at: i64,
    source_identity: &str,
    canonical_hash: [u8; 32],
) -> StoredObservation {
    StoredObservation {
        id: ObservationId::new(),
        system_id,
        measured_at,
        received_at: measured_at + 1,
        source_kind: "modern_api".to_owned(),
        source_identity: source_identity.to_owned(),
        idempotency_identity: Some(format!("idem-{source_identity}")),
        quality_flags: 0,
        generation_power_watts: Some(800),
        generation_energy_wh: Some(50),
        consumption_power_watts: Some(300),
        consumption_energy_wh: Some(20),
        provenance: serde_json::json!({"source": "contract"}),
        canonical_hash,
        version: 1,
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
    management_path: &std::path::Path,
    accounts_dir: &std::path::Path,
    label: &str,
) -> Result<AccountId, Box<dyn Error>> {
    let account_id = AccountId::new();
    let mut management = SqliteConnection::connect_with(
        &SqliteConnectOptions::new()
            .filename(management_path)
            .create_if_missing(false)
            .foreign_keys(true),
    )
    .await?;
    sqlx::query(
        "INSERT INTO accounts (id,slug,display_name,status,created_at,updated_at) \
         VALUES (?,?,?,'provisioning',1,1)",
    )
    .bind(account_id.as_uuid().as_bytes().as_slice())
    .bind(format!("telemetry-repository-{label}-{account_id}"))
    .bind(format!("Account {label}"))
    .execute(&mut management)
    .await?;
    management.close().await?;
    SqliteAccountProvisioner::new(management_path.to_owned(), accounts_dir.to_owned())
        .provision(account_id)
        .await?;
    Ok(account_id)
}

async fn create_postgres_account(url: &str, label: &str) -> Result<AccountId, Box<dyn Error>> {
    let account_id = AccountId::new();
    let mut connection = PgConnection::connect(url).await?;
    sqlx::query(
        "INSERT INTO management.accounts (id,slug,display_name,status,created_at,updated_at) \
         VALUES ($1,$2,$3,'active',1,1)",
    )
    .bind(account_id.as_uuid())
    .bind(format!("telemetry-repository-{label}-{account_id}"))
    .bind(format!("Account {label}"))
    .execute(&mut connection)
    .await?;
    connection.close().await?;
    Ok(account_id)
}
