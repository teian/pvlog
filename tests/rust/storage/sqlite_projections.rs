//! Transactional outbox and idempotent management projection contracts.

use std::{error::Error, time::Duration};

use pvlog_domain::{AccountId, SystemId};
use pvlog_storage::{
    DatabaseTarget, ProjectionActivityState, ProjectionError, ProjectionInvalidationReason,
    ProjectionLocationPrecision, ProjectionVisibility, RoutedSqliteAccount,
    SqliteAccountPoolConfig, SqliteAccountPoolRouter, SqliteAccountProvisioner,
    SqliteProjectionCoordinator, SystemDiscoveryProjection, SystemProjectionEvent,
    append_projection_event, apply_migrations,
};
use sqlx::{Connection as _, Row as _, SqliteConnection, sqlite::SqliteConnectOptions};
use tempfile::TempDir;

#[tokio::test]
async fn authoritative_write_and_outbox_sequence_commit_or_roll_back_together()
-> Result<(), Box<dyn Error>> {
    let setup = ProjectionSetup::new().await?;
    let system_id = SystemId::new();
    let projection = public_projection(system_id);
    {
        let mut writer = setup.account.acquire_writer().await?;
        let mut transaction = writer.connection().begin().await?;
        insert_system(&mut transaction, system_id, "Rolled back").await?;
        append_projection_event(
            &mut transaction,
            &SystemProjectionEvent::upsert(projection.clone(), false, None)?,
        )
        .await?;
        transaction.rollback().await?;
    }
    let mut account = setup.account.acquire().await?;
    let system_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM systems")
        .fetch_one(&mut *account)
        .await?;
    let outbox_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM projection_outbox_events")
        .fetch_one(&mut *account)
        .await?;
    let sequence: i64 = sqlx::query_scalar(
        "SELECT current_sequence FROM projection_outbox_state WHERE singleton = 1",
    )
    .fetch_one(&mut *account)
    .await?;
    assert_eq!((system_count, outbox_count, sequence), (0, 0, 0));
    drop(account);

    {
        let mut writer = setup.account.acquire_writer().await?;
        let mut transaction = writer.connection().begin().await?;
        insert_system(&mut transaction, system_id, "Committed").await?;
        let sequence = append_projection_event(
            &mut transaction,
            &SystemProjectionEvent::upsert(projection, false, None)?,
        )
        .await?;
        assert_eq!(sequence, 1);
        transaction.commit().await?;
    }
    let report = setup
        .coordinator
        .reconcile_account(&setup.account, 100)
        .await?;
    assert_eq!(report.applied, 1);
    assert_eq!(report.last_sequence, Some(1));

    let mut management = setup.management_connection().await?;
    let row = sqlx::query(
        "SELECT display_name, country_code, location_precision, visibility, source_sequence \
         FROM system_discovery_projections WHERE system_id = ?",
    )
    .bind(system_id.as_uuid().as_bytes().as_slice())
    .fetch_one(&mut management)
    .await?;
    assert_eq!(row.get::<String, _>("display_name"), "Roof array");
    assert_eq!(row.get::<String, _>("country_code"), "DE");
    assert_eq!(row.get::<String, _>("location_precision"), "locality");
    assert_eq!(row.get::<String, _>("visibility"), "public");
    assert_eq!(row.get::<i64, _>("source_sequence"), 1);
    management.close().await?;
    Ok(())
}

#[tokio::test]
async fn repeated_delivery_repairs_local_checkpoint_without_reapplying_projection()
-> Result<(), Box<dyn Error>> {
    let setup = ProjectionSetup::new().await?;
    let system_id = SystemId::new();
    setup
        .commit_system_event(
            system_id,
            SystemProjectionEvent::upsert(public_projection(system_id), false, None)?,
        )
        .await?;
    setup
        .coordinator
        .reconcile_account(&setup.account, 100)
        .await?;

    let mut writer = setup.account.acquire_writer().await?;
    sqlx::query(
        "UPDATE projection_outbox_deliveries SET delivered_at = NULL WHERE source_sequence = 1",
    )
    .execute(writer.connection())
    .await?;
    drop(writer);
    let replay = setup
        .coordinator
        .reconcile_account(&setup.account, 100)
        .await?;
    assert_eq!(replay.applied, 0);
    assert_eq!(replay.duplicates, 1);

    let mut management = setup.management_connection().await?;
    let inbox_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM account_projection_inbox WHERE account_id = ?")
            .bind(setup.account_id.as_uuid().as_bytes().as_slice())
            .fetch_one(&mut management)
            .await?;
    let projected_sequence: i64 = sqlx::query_scalar(
        "SELECT projected_sequence FROM account_database_registry WHERE account_id = ?",
    )
    .bind(setup.account_id.as_uuid().as_bytes().as_slice())
    .fetch_one(&mut management)
    .await?;
    assert_eq!((inbox_count, projected_sequence), (1, 1));
    management.close().await?;
    Ok(())
}

#[tokio::test]
async fn bounded_reconciliation_exposes_source_and_applied_freshness() -> Result<(), Box<dyn Error>>
{
    let setup = ProjectionSetup::new().await?;
    for _ in 0..2 {
        let system_id = SystemId::new();
        setup
            .commit_system_event(
                system_id,
                SystemProjectionEvent::upsert(public_projection(system_id), false, None)?,
            )
            .await?;
    }
    let first = setup
        .coordinator
        .reconcile_account(&setup.account, 1)
        .await?;
    assert_eq!(first.applied, 1);
    let mut management = setup.management_connection().await?;
    let row = sqlx::query(
        "SELECT source_sequence, applied_sequence FROM account_projection_checkpoints \
         WHERE account_id = ?",
    )
    .bind(setup.account_id.as_uuid().as_bytes().as_slice())
    .fetch_one(&mut management)
    .await?;
    assert_eq!(row.get::<i64, _>("source_sequence"), 2);
    assert_eq!(row.get::<i64, _>("applied_sequence"), 1);
    management.close().await?;

    setup
        .coordinator
        .reconcile_account(&setup.account, 1)
        .await?;
    let mut management = setup.management_connection().await?;
    let applied: i64 = sqlx::query_scalar(
        "SELECT applied_sequence FROM account_projection_checkpoints WHERE account_id = ?",
    )
    .bind(setup.account_id.as_uuid().as_bytes().as_slice())
    .fetch_one(&mut management)
    .await?;
    assert_eq!(applied, 2);
    management.close().await?;
    Ok(())
}

#[tokio::test]
async fn privacy_reservation_hides_projection_before_authoritative_change()
-> Result<(), Box<dyn Error>> {
    let setup = ProjectionSetup::new().await?;
    let system_id = SystemId::new();
    setup
        .commit_system_event(
            system_id,
            SystemProjectionEvent::upsert(public_projection(system_id), false, None)?,
        )
        .await?;
    setup
        .coordinator
        .reconcile_account(&setup.account, 100)
        .await?;

    let invalidation_id = setup
        .coordinator
        .reserve_privacy_invalidation(
            setup.account_id,
            system_id,
            ProjectionInvalidationReason::VisibilityReduction,
        )
        .await?;
    assert_eq!(setup.projection_count(system_id).await?, 0);
    {
        let mut writer = setup.account.acquire_writer().await?;
        let mut transaction = writer.connection().begin().await?;
        sqlx::query("UPDATE systems SET visibility = 'private', updated_at = 2 WHERE id = ?")
            .bind(system_id.as_uuid().as_bytes().as_slice())
            .execute(&mut *transaction)
            .await?;
        append_projection_event(
            &mut transaction,
            &SystemProjectionEvent::invalidate(system_id, invalidation_id),
        )
        .await?;
        transaction.commit().await?;
    }
    assert_eq!(setup.projection_count(system_id).await?, 0);
    setup
        .coordinator
        .reconcile_account(&setup.account, 100)
        .await?;
    assert_eq!(setup.projection_count(system_id).await?, 0);

    let mut management = setup.management_connection().await?;
    let resolved_sequence: i64 = sqlx::query_scalar(
        "SELECT resolved_sequence FROM projection_invalidation_reservations WHERE id = ?",
    )
    .bind(invalidation_id.as_bytes().as_slice())
    .fetch_one(&mut management)
    .await?;
    assert_eq!(resolved_sequence, 2);
    management.close().await?;
    assert!(matches!(
        SystemProjectionEvent::upsert(public_projection(system_id), true, None),
        Err(ProjectionError::PrivacyReservationRequired)
    ));
    Ok(())
}

#[tokio::test]
async fn reconciliation_fails_closed_on_a_source_sequence_gap() -> Result<(), Box<dyn Error>> {
    let setup = ProjectionSetup::new().await?;
    let system_id = SystemId::new();
    {
        let mut writer = setup.account.acquire_writer().await?;
        let mut transaction = writer.connection().begin().await?;
        sqlx::query("UPDATE projection_outbox_state SET current_sequence = 1 WHERE singleton = 1")
            .execute(&mut *transaction)
            .await?;
        insert_system(&mut transaction, system_id, "Gap").await?;
        let sequence = append_projection_event(
            &mut transaction,
            &SystemProjectionEvent::upsert(public_projection(system_id), false, None)?,
        )
        .await?;
        assert_eq!(sequence, 2);
        transaction.commit().await?;
    }
    assert!(matches!(
        setup
            .coordinator
            .reconcile_account(&setup.account, 100)
            .await,
        Err(ProjectionError::SequenceGap {
            expected: 1,
            actual: 2
        })
    ));
    assert_eq!(setup.projection_count(system_id).await?, 0);
    Ok(())
}

struct ProjectionSetup {
    _directory: TempDir,
    management_path: std::path::PathBuf,
    account_id: AccountId,
    account: RoutedSqliteAccount,
    coordinator: SqliteProjectionCoordinator,
}

impl ProjectionSetup {
    async fn new() -> Result<Self, Box<dyn Error>> {
        let directory = TempDir::new()?;
        let management_path = directory.path().join("management.sqlite3");
        let accounts_dir = directory.path().join("accounts");
        apply_migrations(&DatabaseTarget::Sqlite {
            management_path: management_path.clone(),
            accounts_dir: accounts_dir.clone(),
        })
        .await?;
        let account_id = AccountId::new();
        let mut management = sqlite_connection(&management_path).await?;
        sqlx::query(
            "INSERT INTO accounts \
             (id, slug, display_name, status, created_at, updated_at) \
             VALUES (?, ?, 'Projection', 'provisioning', 1, 1)",
        )
        .bind(account_id.as_uuid().as_bytes().as_slice())
        .bind(format!("projection-{account_id}"))
        .execute(&mut management)
        .await?;
        management.close().await?;
        SqliteAccountProvisioner::new(management_path.clone(), accounts_dir.clone())
            .provision(account_id)
            .await?;
        let router = SqliteAccountPoolRouter::new(
            management_path.clone(),
            accounts_dir,
            SqliteAccountPoolConfig {
                max_open_account_pools: 4,
                max_connections_per_account: 2,
                busy_timeout: Duration::from_secs(1),
                acquire_timeout: Duration::from_secs(1),
                idle_pool_timeout: Duration::from_mins(1),
            },
        )?;
        let account = router.route(account_id).await?;
        let coordinator =
            SqliteProjectionCoordinator::new(management_path.clone(), Duration::from_secs(1));
        Ok(Self {
            _directory: directory,
            management_path,
            account_id,
            account,
            coordinator,
        })
    }

    async fn commit_system_event(
        &self,
        system_id: SystemId,
        event: SystemProjectionEvent,
    ) -> Result<(), Box<dyn Error>> {
        let mut writer = self.account.acquire_writer().await?;
        let mut transaction = writer.connection().begin().await?;
        insert_system(&mut transaction, system_id, "PV").await?;
        append_projection_event(&mut transaction, &event).await?;
        transaction.commit().await?;
        Ok(())
    }

    async fn projection_count(&self, system_id: SystemId) -> Result<i64, sqlx::Error> {
        let mut management = self.management_connection().await?;
        let count = sqlx::query_scalar(
            "SELECT COUNT(*) FROM system_discovery_projections WHERE system_id = ?",
        )
        .bind(system_id.as_uuid().as_bytes().as_slice())
        .fetch_one(&mut management)
        .await?;
        management.close().await?;
        Ok(count)
    }

    async fn management_connection(&self) -> Result<SqliteConnection, sqlx::Error> {
        sqlite_connection(&self.management_path).await
    }
}

fn public_projection(system_id: SystemId) -> SystemDiscoveryProjection {
    SystemDiscoveryProjection {
        system_id,
        display_name: "Roof array".to_owned(),
        country_code: Some("DE".to_owned()),
        location_label: Some("Berlin".to_owned()),
        location_precision: ProjectionLocationPrecision::Locality,
        capacity_watts: 12_000,
        visibility: ProjectionVisibility::Public,
        activity_state: ProjectionActivityState::Active,
    }
}

async fn insert_system(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    system_id: SystemId,
    name: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO systems \
         (id, name, timezone, visibility, status_interval_seconds, power_calculation_mode, \
          net_calculation_mode, created_at, updated_at) \
         VALUES (?, ?, 'Europe/Berlin', 'public', 300, 'reported', 'separate_flows', 1, 1)",
    )
    .bind(system_id.as_uuid().as_bytes().as_slice())
    .bind(name)
    .execute(&mut **transaction)
    .await?;
    Ok(())
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
