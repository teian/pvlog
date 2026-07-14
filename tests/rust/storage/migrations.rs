//! Migration contract tests for `SQLite` and optional `PostgreSQL`.

use std::{collections::BTreeSet, error::Error, fs::File, path::Path, str::FromStr as _};

use pvlog_storage::{
    DatabaseTarget, MigrationError, MigrationKind, MigrationState, apply_migrations,
    ensure_schema_compatible, migration_plan, migration_status, probe_database,
};
use sqlx::{Connection as _, Row as _, SqliteConnection, sqlite::SqliteConnectOptions};
use tempfile::TempDir;
use uuid::Uuid;

#[tokio::test]
async fn sqlite_management_and_accounts_plan_apply_and_verify_independently()
-> Result<(), Box<dyn Error>> {
    let directory = TempDir::new()?;
    let target = sqlite_target(
        directory.path(),
        &["account-b.sqlite3", "account-a.sqlite3"],
    )?;
    File::create(directory.path().join("accounts/README.txt"))?;

    let initial = migration_status(&target).await?;
    assert_eq!(initial.len(), 3);
    assert_eq!(initial[0].kind, MigrationKind::SqliteManagement);
    assert!(initial.iter().all(|status| !status.compatible));
    assert_eq!(migration_plan(&target).await?.len(), 22);
    assert!(ensure_schema_compatible(&target).await.is_err());

    let applied = apply_migrations(&target).await?;
    assert!(applied.iter().all(|status| status.compatible));
    assert!(applied.iter().all(|status| {
        status
            .migrations
            .iter()
            .all(|migration| migration.state == MigrationState::Applied)
    }));
    assert!(migration_plan(&target).await?.is_empty());
    ensure_schema_compatible(&target).await?;
    probe_database(&target).await?;
    Ok(())
}

#[tokio::test]
async fn sqlite_management_schema_enforces_identity_security_routing_and_projection_boundaries()
-> Result<(), Box<dyn Error>> {
    let directory = TempDir::new()?;
    let target = sqlite_target(directory.path(), &[])?;
    apply_migrations(&target).await?;
    let management_path = directory.path().join("management.sqlite3");
    let mut connection = sqlite_connection(&management_path).await?;

    let tables = sqlx::query_scalar::<_, String>(
        "SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name",
    )
    .fetch_all(&mut connection)
    .await?
    .into_iter()
    .collect::<BTreeSet<_>>();
    for required in [
        "users",
        "local_credentials",
        "user_invitations",
        "password_recovery_tokens",
        "auth_connectors",
        "external_identities",
        "external_token_state",
        "sessions",
        "accounts",
        "memberships",
        "api_credentials",
        "api_credential_scopes",
        "rbac_roles",
        "rbac_role_inheritance",
        "rbac_role_permissions",
        "rbac_role_assignments",
        "quota_policies",
        "global_configuration",
        "account_database_registry",
        "system_registry",
        "account_provisioning_log",
        "global_audit_events",
        "account_projection_checkpoints",
        "system_discovery_projections",
    ] {
        assert!(
            tables.contains(required),
            "missing management table {required}"
        );
    }
    assert!(!tables.iter().any(|name| {
        name.contains("telemetry") || name.contains("observation") || name.contains("segment")
    }));

    connection.close().await?;
    Ok(())
}

#[tokio::test]
async fn sqlite_management_identity_constraints_are_fail_closed() -> Result<(), Box<dyn Error>> {
    let (_directory, mut connection) = migrated_management_database().await?;
    let user_id = id();
    sqlx::query(
        "INSERT INTO users \
         (id, email, display_name, status, created_at, updated_at) \
         VALUES (?, 'owner@example.test', 'Owner', 'active', 1, 1)",
    )
    .bind(&user_id)
    .execute(&mut connection)
    .await?;
    sqlx::query(
        "INSERT INTO local_credentials \
         (user_id, password_hash, password_changed_at) VALUES (?, '$argon2id$v=19$test', 1)",
    )
    .bind(&user_id)
    .execute(&mut connection)
    .await?;
    assert!(
        sqlx::query(
            "INSERT INTO users \
             (id, email, display_name, status, created_at, updated_at) \
             VALUES (X'01', 'invalid@example.test', 'Invalid', 'active', 1, 1)",
        )
        .execute(&mut connection)
        .await
        .is_err()
    );

    let connector_id = id();
    sqlx::query(
        "INSERT INTO auth_connectors \
         (id, slug, display_name, protocol, discovery_url, client_id, client_secret_ref, \
          scopes_json, claim_mapping_json, created_at, updated_at) \
         VALUES (?, 'company-login', 'Company Login', 'oidc', \
                 'https://issuer.example/.well-known/openid-configuration', 'client', \
                 'secrets/oidc/client', '[\"openid\"]', '{\"subject\":\"sub\"}', 1, 1)",
    )
    .bind(&connector_id)
    .execute(&mut connection)
    .await?;
    assert!(
        sqlx::query(
            "INSERT INTO auth_connectors \
             (id, slug, display_name, protocol, client_id, client_secret_ref, scopes_json, \
              claim_mapping_json, created_at, updated_at) \
             VALUES (?, 'vendor-protocol', 'Invalid', 'vendor', 'client', 'secret/ref', \
                     '[]', '{}', 1, 1)",
        )
        .bind(id())
        .execute(&mut connection)
        .await
        .is_err()
    );

    let identity_id = id();
    sqlx::query(
        "INSERT INTO external_identities \
         (id, connector_id, user_id, provider_subject, linked_at) VALUES (?, ?, ?, 'subject-1', 1)",
    )
    .bind(&identity_id)
    .bind(&connector_id)
    .bind(&user_id)
    .execute(&mut connection)
    .await?;
    assert!(
        sqlx::query(
            "INSERT INTO external_identities \
             (id, connector_id, user_id, provider_subject, linked_at) \
             VALUES (?, ?, ?, 'subject-1', 1)",
        )
        .bind(id())
        .bind(&connector_id)
        .bind(&user_id)
        .execute(&mut connection)
        .await
        .is_err()
    );

    connection.close().await?;
    Ok(())
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn sqlite_management_routing_rbac_and_projection_constraints_are_fail_closed()
-> Result<(), Box<dyn Error>> {
    let (_directory, mut connection) = migrated_management_database().await?;
    let user_id = insert_user(&mut connection, "owner@example.test").await?;
    let account_id = id();
    sqlx::query(
        "INSERT INTO accounts \
         (id, slug, display_name, status, created_by, created_at, updated_at) \
         VALUES (?, 'home', 'Home', 'provisioning', ?, 1, 1)",
    )
    .bind(&account_id)
    .bind(&user_id)
    .execute(&mut connection)
    .await?;
    assert!(
        sqlx::query(
            "INSERT INTO account_database_registry \
             (account_id, opaque_locator, lifecycle_state, created_at, updated_at) \
             VALUES (?, '../escape.sqlite3', 'reserved', 1, 1)",
        )
        .bind(&account_id)
        .execute(&mut connection)
        .await
        .is_err()
    );
    sqlx::query(
        "INSERT INTO account_database_registry \
         (account_id, opaque_locator, lifecycle_state, created_at, updated_at) \
         VALUES (?, '019505c8-7c85-7f0b-9bc3-2a3c4d5e6f70.sqlite3', 'reserved', 1, 1)",
    )
    .bind(&account_id)
    .execute(&mut connection)
    .await?;

    assert!(
        sqlx::query(
            "INSERT INTO system_registry (system_id, account_id, created_at, updated_at) \
             VALUES (?, ?, 1, 1)",
        )
        .bind(id())
        .bind(id())
        .execute(&mut connection)
        .await
        .is_err()
    );
    let system_id = id();
    sqlx::query(
        "INSERT INTO system_registry (system_id, account_id, created_at, updated_at) \
         VALUES (?, ?, 1, 1)",
    )
    .bind(&system_id)
    .bind(&account_id)
    .execute(&mut connection)
    .await?;
    assert_eq!(
        sqlx::query_scalar::<_, Vec<u8>>(
            "SELECT account_id FROM system_registry WHERE system_id = ?",
        )
        .bind(&system_id)
        .fetch_one(&mut connection)
        .await?,
        account_id
    );

    let role_id = id();
    sqlx::query(
        "INSERT INTO rbac_roles \
         (id, account_id, name, role_kind, created_by, created_at, updated_at) \
         VALUES (?, ?, 'operator', 'custom', ?, 1, 1)",
    )
    .bind(&role_id)
    .bind(&account_id)
    .bind(&user_id)
    .execute(&mut connection)
    .await?;
    assert!(
        sqlx::query(
            "INSERT INTO rbac_role_assignments \
             (id, role_id, principal_type, principal_id, scope_type, account_id, created_at) \
             VALUES (?, ?, 'user', ?, 'account', ?, 1)",
        )
        .bind(id())
        .bind(&role_id)
        .bind(id())
        .bind(&account_id)
        .execute(&mut connection)
        .await
        .is_err()
    );
    sqlx::query(
        "INSERT INTO rbac_role_assignments \
         (id, role_id, principal_type, principal_id, scope_type, account_id, created_at) \
         VALUES (?, ?, 'user', ?, 'account', ?, 1)",
    )
    .bind(id())
    .bind(&role_id)
    .bind(&user_id)
    .bind(&account_id)
    .execute(&mut connection)
    .await?;

    assert!(
        sqlx::query(
            "INSERT INTO system_discovery_projections \
             (system_id, account_id, display_name, location_label, location_precision, \
              capacity_watts, visibility, activity_state, source_sequence, projected_at) \
             VALUES (?, ?, 'Private system', 'Secret place', 'locality', 5000, 'private', \
                     'active', 1, 1)",
        )
        .bind(id())
        .bind(&account_id)
        .execute(&mut connection)
        .await
        .is_err()
    );

    connection.close().await?;
    Ok(())
}

#[tokio::test]
async fn sqlite_management_audit_and_secret_storage_are_hardened() -> Result<(), Box<dyn Error>> {
    let (_directory, mut connection) = migrated_management_database().await?;
    let audit_id = id();
    sqlx::query(
        "INSERT INTO global_audit_events \
         (id, occurred_at, actor_type, action, target_type, outcome, event_hash) \
         VALUES (?, 1, 'anonymous', 'instance.bootstrap', 'instance', 'succeeded', ?)",
    )
    .bind(&audit_id)
    .bind(vec![7_u8; 32])
    .execute(&mut connection)
    .await?;
    assert!(
        sqlx::query("UPDATE global_audit_events SET action = 'changed' WHERE id = ?")
            .bind(&audit_id)
            .execute(&mut connection)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("DELETE FROM global_audit_events WHERE id = ?")
            .bind(&audit_id)
            .execute(&mut connection)
            .await
            .is_err()
    );

    let security_columns = sqlx::query(
        "SELECT tables.name AS table_name, columns.name AS column_name \
         FROM sqlite_master AS tables \
         JOIN pragma_table_info(tables.name) AS columns \
         WHERE tables.name IN \
         ('local_credentials', 'password_recovery_tokens', 'external_token_state')",
    )
    .fetch_all(&mut connection)
    .await?
    .into_iter()
    .map(|row| {
        (
            row.get::<String, _>("table_name"),
            row.get::<String, _>("column_name"),
        )
    })
    .collect::<BTreeSet<_>>();
    for expected in [
        ("local_credentials", "password_hash"),
        ("password_recovery_tokens", "token_digest"),
        ("external_token_state", "encrypted_access_token"),
        ("external_token_state", "encrypted_refresh_token"),
        ("external_token_state", "encrypted_id_token"),
    ] {
        assert!(security_columns.contains(&(expected.0.to_owned(), expected.1.to_owned())));
    }
    assert!(!security_columns.iter().any(|(_, column)| {
        matches!(
            column.as_str(),
            "password" | "access_token" | "refresh_token" | "id_token" | "token"
        )
    }));

    connection.close().await?;
    Ok(())
}

#[tokio::test]
async fn sqlite_checksum_changes_fail_closed_at_startup() -> Result<(), Box<dyn Error>> {
    let directory = TempDir::new()?;
    let target = sqlite_target(directory.path(), &[])?;
    apply_migrations(&target).await?;

    let management_path = directory.path().join("management.sqlite3");
    let mut connection = sqlite_connection(&management_path).await?;
    sqlx::query("UPDATE _sqlx_migrations SET checksum = X'00' WHERE version = 1")
        .execute(&mut connection)
        .await?;
    connection.close().await?;

    let status = migration_status(&target).await?;
    assert_eq!(
        status[0].migrations[0].state,
        MigrationState::ChecksumMismatch
    );
    assert!(!status[0].compatible);
    assert!(ensure_schema_compatible(&target).await.is_err());
    assert!(probe_database(&target).await.is_err());
    Ok(())
}

#[tokio::test]
async fn sqlite_nonexpired_lease_blocks_apply_and_expired_lease_is_recovered()
-> Result<(), Box<dyn Error>> {
    let directory = TempDir::new()?;
    let target = sqlite_target(directory.path(), &[])?;
    let management_path = directory.path().join("management.sqlite3");
    let mut connection = sqlite_connection(&management_path).await?;
    sqlx::query(
        "CREATE TABLE _pvlog_migration_lease (\
         singleton INTEGER PRIMARY KEY CHECK (singleton = 1), \
         owner TEXT NOT NULL, expires_at INTEGER NOT NULL)",
    )
    .execute(&mut connection)
    .await?;
    sqlx::query(
        "INSERT INTO _pvlog_migration_lease (singleton, owner, expires_at) \
         VALUES (1, 'other-process', 4102444800)",
    )
    .execute(&mut connection)
    .await?;
    connection.close().await?;

    assert!(matches!(
        apply_migrations(&target).await,
        Err(MigrationError::LeaseHeld { .. })
    ));

    let mut connection = sqlite_connection(&management_path).await?;
    sqlx::query("UPDATE _pvlog_migration_lease SET expires_at = 0 WHERE singleton = 1")
        .execute(&mut connection)
        .await?;
    connection.close().await?;

    let statuses = apply_migrations(&target).await?;
    assert!(statuses[0].compatible);
    Ok(())
}

#[tokio::test]
async fn sqlite_account_failure_does_not_prevent_later_healthy_account_migration()
-> Result<(), Box<dyn Error>> {
    let directory = TempDir::new()?;
    let target = sqlite_target(directory.path(), &["account-a.sqlite3"])?;
    apply_migrations(&target).await?;

    let accounts_dir = directory.path().join("accounts");
    let broken_path = accounts_dir.join("account-a.sqlite3");
    let mut connection = sqlite_connection(&broken_path).await?;
    sqlx::query("UPDATE _sqlx_migrations SET checksum = X'00' WHERE version = 1")
        .execute(&mut connection)
        .await?;
    connection.close().await?;
    File::create(accounts_dir.join("account-b.sqlite3"))?;

    assert!(matches!(
        apply_migrations(&target).await,
        Err(MigrationError::AccountFailures(_))
    ));
    let statuses = migration_status(&target).await?;
    let healthy = statuses
        .iter()
        .find(|status| status.database == "sqlite-account:account-b.sqlite3")
        .ok_or("healthy account migration status is missing")?;
    let broken = statuses
        .iter()
        .find(|status| status.database == "sqlite-account:account-a.sqlite3")
        .ok_or("broken account migration status is missing")?;
    assert!(healthy.compatible);
    assert!(!broken.compatible);
    Ok(())
}

#[tokio::test]
async fn postgres_uses_checksum_verified_migrations_when_configured() -> Result<(), Box<dyn Error>>
{
    let Ok(url) = std::env::var("TEST_POSTGRES_URL") else {
        return Ok(());
    };
    let target = DatabaseTarget::Postgres { url };

    apply_migrations(&target).await?;
    let statuses = migration_status(&target).await?;
    assert_eq!(statuses.len(), 1);
    assert_eq!(statuses[0].kind, MigrationKind::Postgres);
    assert!(statuses[0].compatible);
    ensure_schema_compatible(&target).await?;
    Ok(())
}

async fn migrated_management_database() -> Result<(TempDir, SqliteConnection), Box<dyn Error>> {
    let directory = TempDir::new()?;
    let target = sqlite_target(directory.path(), &[])?;
    apply_migrations(&target).await?;
    let connection = sqlite_connection(&directory.path().join("management.sqlite3")).await?;
    Ok((directory, connection))
}

async fn insert_user(
    connection: &mut SqliteConnection,
    email: &str,
) -> Result<Vec<u8>, sqlx::Error> {
    let user_id = id();
    sqlx::query(
        "INSERT INTO users \
         (id, email, display_name, status, created_at, updated_at) \
         VALUES (?, ?, 'Owner', 'active', 1, 1)",
    )
    .bind(&user_id)
    .bind(email)
    .execute(connection)
    .await?;
    Ok(user_id)
}

fn sqlite_target(root: &Path, accounts: &[&str]) -> Result<DatabaseTarget, Box<dyn Error>> {
    let accounts_dir = root.join("accounts");
    std::fs::create_dir_all(&accounts_dir)?;
    for account in accounts {
        File::create(accounts_dir.join(account))?;
    }
    Ok(DatabaseTarget::Sqlite {
        management_path: root.join("management.sqlite3"),
        accounts_dir,
    })
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
