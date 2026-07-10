//! Contract tests for account-local `SQLite` configuration and operations schema.

use std::{collections::BTreeSet, error::Error, path::Path, str::FromStr as _};

use pvlog_domain::AccountId;
use pvlog_storage::{DatabaseTarget, SqliteAccountProvisioner, apply_migrations};
use sqlx::{Connection as _, SqliteConnection, sqlite::SqliteConnectOptions};
use tempfile::TempDir;
use uuid::Uuid;

#[tokio::test]
async fn account_schema_contains_only_account_owned_configuration_and_operations()
-> Result<(), Box<dyn Error>> {
    let setup = Setup::new().await?;
    let mut account = setup.account_connection().await?;
    let tables = sqlx::query_scalar::<_, String>(
        "SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name",
    )
    .fetch_all(&mut account)
    .await?
    .into_iter()
    .collect::<BTreeSet<_>>();
    for required in [
        "systems",
        "equipment",
        "tariffs",
        "channel_definitions",
        "account_audit_events",
        "import_jobs",
        "export_jobs",
        "alert_rules",
        "alert_events",
        "webhook_subscriptions",
        "webhook_deliveries",
        "webhook_delivery_attempts",
        "provider_configurations",
        "account_jobs",
    ] {
        assert!(
            tables.contains(required),
            "missing account table {required}"
        );
    }
    for management_only in [
        "users",
        "local_credentials",
        "sessions",
        "accounts",
        "memberships",
        "account_database_registry",
        "global_audit_events",
    ] {
        assert!(
            !tables.contains(management_only),
            "management table leaked into account database: {management_only}"
        );
    }
    account.close().await?;
    Ok(())
}

#[tokio::test]
async fn system_equipment_tariff_and_channel_constraints_preserve_effective_history()
-> Result<(), Box<dyn Error>> {
    let setup = Setup::new().await?;
    let mut account = setup.account_connection().await?;
    let system_id = insert_system(&mut account).await?;

    assert!(
        sqlx::query(
            "INSERT INTO equipment \
             (id, system_id, equipment_kind, name, effective_from, effective_to, created_at, updated_at) \
             VALUES (?, ?, 'array', 'Roof', 20, 10, 1, 1)",
        )
        .bind(id())
        .bind(&system_id)
        .execute(&mut account)
        .await
        .is_err()
    );
    sqlx::query(
        "INSERT INTO equipment \
         (id, system_id, equipment_kind, name, capacity_watts, effective_from, created_at, updated_at) \
         VALUES (?, ?, 'array', 'Roof', 8000, 10, 1, 1)",
    )
    .bind(id())
    .bind(&system_id)
    .execute(&mut account)
    .await?;
    sqlx::query(
        "INSERT INTO tariffs \
         (id, system_id, name, direction, currency_code, minor_units_per_kwh, effective_from, created_at, updated_at) \
         VALUES (?, ?, 'Feed in', 'export', 'EUR', 820, 10, 1, 1)",
    )
    .bind(id())
    .bind(&system_id)
    .execute(&mut account)
    .await?;

    let channel_id = id();
    sqlx::query(
        "INSERT INTO channel_definitions \
         (id, system_id, channel_key, display_name, data_type, unit, scale, effective_from, created_at, updated_at) \
         VALUES (?, ?, 'irradiance', 'Irradiance', 'integer', 'W/m2', 0, 10, 1, 1)",
    )
    .bind(&channel_id)
    .bind(&system_id)
    .execute(&mut account)
    .await?;
    assert!(
        sqlx::query(
            "INSERT INTO channel_definitions \
             (id, system_id, channel_key, display_name, data_type, unit, scale, effective_from, created_at, updated_at) \
             VALUES (?, ?, 'irradiance', 'Duplicate', 'integer', 'W/m2', 0, 20, 1, 1)",
        )
        .bind(id())
        .bind(&system_id)
        .execute(&mut account)
        .await
        .is_err()
    );
    account.close().await?;
    Ok(())
}

#[tokio::test]
async fn audit_integrations_and_jobs_store_only_safe_or_encrypted_sensitive_state()
-> Result<(), Box<dyn Error>> {
    let setup = Setup::new().await?;
    let mut account = setup.account_connection().await?;
    let system_id = insert_system(&mut account).await?;
    let audit_id = id();
    sqlx::query(
        "INSERT INTO account_audit_events \
         (id, occurred_at, actor_type, action, target_type, target_id, outcome, event_hash) \
         VALUES (?, 1, 'worker', 'system.created', 'system', ?, 'succeeded', ?)",
    )
    .bind(&audit_id)
    .bind(&system_id)
    .bind(vec![9_u8; 32])
    .execute(&mut account)
    .await?;
    assert!(
        sqlx::query("DELETE FROM account_audit_events WHERE id = ?")
            .bind(&audit_id)
            .execute(&mut account)
            .await
            .is_err()
    );

    sqlx::query(
        "INSERT INTO webhook_subscriptions \
         (id, name, endpoint_url, state, event_types_json, encryption_key_id, \
          encrypted_signing_secret, created_at, updated_at) \
         VALUES (?, 'operations', 'https://hooks.example.test/pv', 'active', \
                 '[\"alert.triggered\"]', 'key-1', ?, 1, 1)",
    )
    .bind(id())
    .bind(vec![4_u8; 48])
    .execute(&mut account)
    .await?;
    sqlx::query(
        "INSERT INTO provider_configurations \
         (id, provider_kind, name, enabled, credential_secret_ref, created_at, updated_at) \
         VALUES (?, 'insolation', 'regional-source', 1, 'secrets/providers/insolation', 1, 1)",
    )
    .bind(id())
    .execute(&mut account)
    .await?;
    assert!(
        sqlx::query(
            "INSERT INTO account_jobs \
             (id, job_kind, state, payload_json, max_attempts, available_at, created_at, updated_at) \
             VALUES (?, 'reconcile', 'pending', '{}', 0, 1, 1, 1)",
        )
        .bind(id())
        .execute(&mut account)
        .await
        .is_err()
    );

    let sensitive_columns = sqlx::query_scalar::<_, String>(
        "SELECT columns.name FROM sqlite_master AS tables \
         JOIN pragma_table_info(tables.name) AS columns \
         WHERE tables.name IN ('webhook_subscriptions', 'provider_configurations')",
    )
    .fetch_all(&mut account)
    .await?;
    assert!(
        sensitive_columns
            .iter()
            .any(|name| name == "encrypted_signing_secret")
    );
    assert!(
        sensitive_columns
            .iter()
            .any(|name| name == "credential_secret_ref")
    );
    assert!(!sensitive_columns.iter().any(|name| {
        matches!(
            name.as_str(),
            "signing_secret" | "credential" | "api_key" | "access_token" | "refresh_token"
        )
    }));
    account.close().await?;
    Ok(())
}

struct Setup {
    _directory: TempDir,
    account_path: std::path::PathBuf,
}

impl Setup {
    async fn new() -> Result<Self, Box<dyn Error>> {
        let directory = TempDir::new()?;
        let management_path = directory.path().join("management.sqlite3");
        let accounts_dir = directory.path().join("accounts");
        let target = DatabaseTarget::Sqlite {
            management_path: management_path.clone(),
            accounts_dir: accounts_dir.clone(),
        };
        apply_migrations(&target).await?;
        let account_id = AccountId::new();
        let mut management = sqlite_connection(&management_path).await?;
        sqlx::query(
            "INSERT INTO accounts \
             (id, slug, display_name, status, created_at, updated_at) \
             VALUES (?, 'schema-test', 'Schema Test', 'provisioning', 1, 1)",
        )
        .bind(account_id.as_uuid().as_bytes().as_slice())
        .execute(&mut management)
        .await?;
        management.close().await?;
        let provisioner = SqliteAccountProvisioner::new(management_path, accounts_dir.clone());
        let result = provisioner.provision(account_id).await?;
        Ok(Self {
            _directory: directory,
            account_path: accounts_dir.join(result.opaque_locator),
        })
    }

    async fn account_connection(&self) -> Result<SqliteConnection, sqlx::Error> {
        sqlite_connection(&self.account_path).await
    }
}

async fn insert_system(connection: &mut SqliteConnection) -> Result<Vec<u8>, sqlx::Error> {
    let system_id = id();
    sqlx::query(
        "INSERT INTO systems \
         (id, name, timezone, status_interval_seconds, power_calculation_mode, \
          net_calculation_mode, created_at, updated_at) \
         VALUES (?, 'Home PV', 'Europe/Berlin', 300, 'reported', 'separate_flows', 1, 1)",
    )
    .bind(&system_id)
    .execute(connection)
    .await?;
    Ok(system_id)
}

async fn sqlite_connection(path: &Path) -> Result<SqliteConnection, sqlx::Error> {
    let options = SqliteConnectOptions::from_str(path.to_string_lossy().as_ref())?
        .create_if_missing(true)
        .foreign_keys(true);
    SqliteConnection::connect_with(&options).await
}

fn id() -> Vec<u8> {
    Uuid::now_v7().as_bytes().to_vec()
}
