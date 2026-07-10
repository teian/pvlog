//! Recoverable projection delivery across isolated account and management databases.

use std::{path::PathBuf, sync::Arc, time::Duration};

use pvlog_domain::{AccountId, SystemId};
use serde::{Deserialize, Serialize};
use sqlx::{
    Connection as _, Row as _, Sqlite, SqliteConnection, Transaction, sqlite::SqliteConnectOptions,
};
use thiserror::Error;
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::{RoutedSqliteAccount, SqliteRoutingError};

/// Privacy-filtered system fields permitted in the management database.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemDiscoveryProjection {
    /// System represented by this projection.
    pub system_id: SystemId,
    /// Public or unlisted display label.
    pub display_name: String,
    /// Optional two-letter country code for a public system.
    pub country_code: Option<String>,
    /// Optional coarse location label for a public system.
    pub location_label: Option<String>,
    /// Maximum location precision explicitly permitted for projection.
    pub location_precision: ProjectionLocationPrecision,
    /// Effective system capacity.
    pub capacity_watts: i64,
    /// Cross-account visibility.
    pub visibility: ProjectionVisibility,
    /// Public lifecycle representation.
    pub activity_state: ProjectionActivityState,
}

impl SystemDiscoveryProjection {
    fn privacy_filtered(mut self) -> Result<Self, ProjectionError> {
        if self.display_name.trim().is_empty() || self.capacity_watts < 0 {
            return Err(ProjectionError::InvalidProjectionPayload);
        }
        if self
            .country_code
            .as_ref()
            .is_some_and(|country| country.len() != 2 || !country.is_ascii())
        {
            return Err(ProjectionError::InvalidProjectionPayload);
        }
        if self.visibility != ProjectionVisibility::Public {
            self.country_code = None;
            self.location_label = None;
            self.location_precision = ProjectionLocationPrecision::Hidden;
        } else if self.location_precision == ProjectionLocationPrecision::Hidden {
            self.location_label = None;
        }
        Ok(self)
    }
}

/// Coarse location precision allowed outside the account database.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectionLocationPrecision {
    /// No location data.
    Hidden,
    /// Country only.
    Country,
    /// Region only.
    Region,
    /// Locality only; exact coordinates are never projected.
    Locality,
}

impl ProjectionLocationPrecision {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Hidden => "hidden",
            Self::Country => "country",
            Self::Region => "region",
            Self::Locality => "locality",
        }
    }
}

/// Visibility retained by a safe management projection.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectionVisibility {
    /// Must not be represented by a discoverable projection.
    Private,
    /// Addressable with authorization but excluded from public discovery.
    Unlisted,
    /// Eligible for public discovery.
    Public,
}

impl ProjectionVisibility {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Private => "private",
            Self::Unlisted => "unlisted",
            Self::Public => "public",
        }
    }
}

/// Lifecycle state permitted in discovery projections.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectionActivityState {
    /// Active system.
    Active,
    /// Archived system.
    Archived,
    /// Administratively disabled system.
    Disabled,
}

impl ProjectionActivityState {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Archived => "archived",
            Self::Disabled => "disabled",
        }
    }
}

/// Typed account-local event written in the same transaction as authoritative changes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SystemProjectionEvent {
    kind: ProjectionEventKind,
    system_id: SystemId,
    projection: Option<SystemDiscoveryProjection>,
    privacy_reducing: bool,
    invalidation_id: Option<Uuid>,
}

impl SystemProjectionEvent {
    /// Builds a privacy-filtered upsert event.
    ///
    /// A privacy-reducing upsert, such as public to unlisted, requires an invalidation
    /// reservation created before the authoritative account transaction begins.
    ///
    /// # Errors
    ///
    /// Returns an error for unsafe payload fields or a missing privacy reservation.
    pub fn upsert(
        projection: SystemDiscoveryProjection,
        privacy_reducing: bool,
        invalidation_id: Option<Uuid>,
    ) -> Result<Self, ProjectionError> {
        if privacy_reducing && invalidation_id.is_none() {
            return Err(ProjectionError::PrivacyReservationRequired);
        }
        let projection = projection.privacy_filtered()?;
        Ok(Self {
            kind: ProjectionEventKind::Upsert,
            system_id: projection.system_id,
            projection: Some(projection),
            privacy_reducing,
            invalidation_id,
        })
    }

    /// Builds an invalidation event that keeps authoritative account data intact.
    #[must_use]
    pub const fn invalidate(system_id: SystemId, invalidation_id: Uuid) -> Self {
        Self {
            kind: ProjectionEventKind::Invalidate,
            system_id,
            projection: None,
            privacy_reducing: true,
            invalidation_id: Some(invalidation_id),
        }
    }

    /// Builds a deletion event after a privacy invalidation reservation.
    #[must_use]
    pub const fn delete(system_id: SystemId, invalidation_id: Uuid) -> Self {
        Self {
            kind: ProjectionEventKind::Delete,
            system_id,
            projection: None,
            privacy_reducing: true,
            invalidation_id: Some(invalidation_id),
        }
    }
}

/// Appends an immutable projection event using the caller's authoritative account transaction.
///
/// # Errors
///
/// Returns an error when sequence allocation, serialization, or insertion fails.
pub async fn append_projection_event(
    transaction: &mut Transaction<'_, Sqlite>,
    event: &SystemProjectionEvent,
) -> Result<i64, ProjectionError> {
    let sequence: i64 = sqlx::query_scalar(
        "UPDATE projection_outbox_state SET current_sequence = current_sequence + 1 \
         WHERE singleton = 1 RETURNING current_sequence",
    )
    .fetch_one(&mut **transaction)
    .await?;
    let event_id = Uuid::now_v7();
    let payload = event
        .projection
        .as_ref()
        .map_or_else(|| Ok("{}".to_owned()), serde_json::to_string)?;
    let created_at = epoch_millis()?;
    sqlx::query(
        "INSERT INTO projection_outbox_events \
         (source_sequence, event_id, event_kind, system_id, payload_json, privacy_reducing, \
          invalidation_id, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(sequence)
    .bind(event_id.as_bytes().as_slice())
    .bind(event.kind.as_str())
    .bind(event.system_id.as_uuid().as_bytes().as_slice())
    .bind(payload)
    .bind(event.privacy_reducing)
    .bind(event.invalidation_id.map(|id| id.as_bytes().to_vec()))
    .bind(created_at)
    .execute(&mut **transaction)
    .await?;
    sqlx::query(
        "INSERT INTO projection_outbox_deliveries \
         (source_sequence, delivery_attempts, last_attempt_at) VALUES (?, 0, ?)",
    )
    .bind(sequence)
    .bind(created_at)
    .execute(&mut **transaction)
    .await?;
    Ok(sequence)
}

/// Reason a management projection must be hidden before an account privacy change.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProjectionInvalidationReason {
    /// Public or unlisted visibility is being reduced.
    VisibilityReduction,
    /// Authoritative system deletion is beginning.
    SystemDeletion,
    /// Account access is being suspended.
    AccountSuspension,
    /// Operator-directed consistency repair.
    OperatorRepair,
}

impl ProjectionInvalidationReason {
    const fn as_str(self) -> &'static str {
        match self {
            Self::VisibilityReduction => "visibility_reduction",
            Self::SystemDeletion => "system_deletion",
            Self::AccountSuspension => "account_suspension",
            Self::OperatorRepair => "operator_repair",
        }
    }
}

/// Applies account outbox events idempotently to privacy-safe management projections.
#[derive(Clone, Debug)]
pub struct SqliteProjectionCoordinator {
    management_path: PathBuf,
    busy_timeout: Duration,
    writer: Arc<Mutex<()>>,
}

impl SqliteProjectionCoordinator {
    /// Creates a management projection coordinator.
    #[must_use]
    pub fn new(management_path: PathBuf, busy_timeout: Duration) -> Self {
        Self {
            management_path,
            busy_timeout,
            writer: Arc::new(Mutex::new(())),
        }
    }

    /// Removes a discoverable row and durably reserves the following privacy change.
    ///
    /// Call this before committing the authoritative account change. A failed later operation
    /// leaves the projection hidden, which is safe and recoverable by a later upsert.
    ///
    /// # Errors
    ///
    /// Returns an error when the management transaction cannot commit.
    pub async fn reserve_privacy_invalidation(
        &self,
        account_id: AccountId,
        system_id: SystemId,
        reason: ProjectionInvalidationReason,
    ) -> Result<Uuid, ProjectionError> {
        let _guard = self.writer.lock().await;
        let mut connection = self.management_connection().await?;
        let mut transaction = connection.begin().await?;
        let invalidation_id = Uuid::now_v7();
        sqlx::query(
            "INSERT INTO projection_invalidation_reservations \
             (id, account_id, system_id, reason, reserved_at) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(invalidation_id.as_bytes().as_slice())
        .bind(account_id.as_uuid().as_bytes().as_slice())
        .bind(system_id.as_uuid().as_bytes().as_slice())
        .bind(reason.as_str())
        .bind(epoch_millis()?)
        .execute(&mut *transaction)
        .await?;
        sqlx::query(
            "DELETE FROM system_discovery_projections WHERE account_id = ? AND system_id = ?",
        )
        .bind(account_id.as_uuid().as_bytes().as_slice())
        .bind(system_id.as_uuid().as_bytes().as_slice())
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        connection.close().await?;
        Ok(invalidation_id)
    }

    /// Replays the next ordered account events and marks successful deliveries locally.
    ///
    /// # Errors
    ///
    /// Returns an error for sequence gaps, conflicting duplicate events, unsafe payloads, or a
    /// database failure. Already-applied events are accepted idempotently.
    pub async fn reconcile_account(
        &self,
        account: &RoutedSqliteAccount,
        max_events: u32,
    ) -> Result<ProjectionReconciliationReport, ProjectionError> {
        if max_events == 0 {
            return Ok(ProjectionReconciliationReport::default());
        }
        let account_id = account.account_id();
        let mut source = account.acquire().await?;
        let source_sequence: i64 = sqlx::query_scalar(
            "SELECT current_sequence FROM projection_outbox_state WHERE singleton = 1",
        )
        .fetch_one(&mut *source)
        .await?;
        self.record_source_sequence(account_id, source_sequence)
            .await?;
        let applied_sequence = self.applied_sequence(account_id).await?;
        let repaired = self
            .mark_delivered_through(account, applied_sequence)
            .await?;
        let rows = sqlx::query(
            "SELECT source_sequence, event_id, event_kind, system_id, payload_json, \
                    privacy_reducing, invalidation_id, created_at \
             FROM projection_outbox_events WHERE source_sequence > ? \
             ORDER BY source_sequence LIMIT ?",
        )
        .bind(applied_sequence)
        .bind(max_events)
        .fetch_all(&mut *source)
        .await?;
        drop(source);

        let mut report = ProjectionReconciliationReport {
            duplicates: u32::try_from(repaired).unwrap_or(u32::MAX),
            ..ProjectionReconciliationReport::default()
        };
        for row in rows {
            let event = StoredProjectionEvent::from_row(&row)?;
            match self.apply_event(account_id, &event).await? {
                ApplyOutcome::Applied => report.applied += 1,
                ApplyOutcome::Duplicate => report.duplicates += 1,
            }
            self.mark_delivered(account, event.source_sequence).await?;
            report.last_sequence = Some(event.source_sequence);
        }
        Ok(report)
    }

    async fn record_source_sequence(
        &self,
        account_id: AccountId,
        source_sequence: i64,
    ) -> Result<(), ProjectionError> {
        let _guard = self.writer.lock().await;
        let mut connection = self.management_connection().await?;
        let mut transaction = connection.begin().await?;
        let now = epoch_millis()?;
        sqlx::query(
            "INSERT INTO account_projection_checkpoints \
             (account_id, source_sequence, applied_sequence, projected_at) \
             VALUES (?, ?, 0, ?) ON CONFLICT(account_id) DO UPDATE SET \
             source_sequence = MAX(source_sequence, excluded.source_sequence)",
        )
        .bind(account_id.as_uuid().as_bytes().as_slice())
        .bind(source_sequence)
        .bind(now)
        .execute(&mut *transaction)
        .await?;
        sqlx::query(
            "UPDATE account_database_registry \
             SET source_sequence = MAX(source_sequence, ?), updated_at = ? WHERE account_id = ?",
        )
        .bind(source_sequence)
        .bind(now)
        .bind(account_id.as_uuid().as_bytes().as_slice())
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        connection.close().await?;
        Ok(())
    }

    async fn applied_sequence(&self, account_id: AccountId) -> Result<i64, ProjectionError> {
        let mut connection = self.management_connection().await?;
        let sequence = sqlx::query_scalar(
            "SELECT applied_sequence FROM account_projection_checkpoints WHERE account_id = ?",
        )
        .bind(account_id.as_uuid().as_bytes().as_slice())
        .fetch_optional(&mut connection)
        .await?
        .unwrap_or(0);
        connection.close().await?;
        Ok(sequence)
    }

    async fn apply_event(
        &self,
        account_id: AccountId,
        event: &StoredProjectionEvent,
    ) -> Result<ApplyOutcome, ProjectionError> {
        let _guard = self.writer.lock().await;
        let mut connection = self.management_connection().await?;
        let mut transaction = connection.begin().await?;
        let now = epoch_millis()?;
        let applied = prepare_checkpoint(&mut transaction, account_id, now).await?;
        let payload_hash = blake3::hash(event.payload_json.as_bytes());

        if event.source_sequence <= applied {
            if !is_duplicate(&mut transaction, account_id, event, payload_hash.as_bytes()).await? {
                return Err(ProjectionError::SequenceConflict(event.source_sequence));
            }
            transaction.commit().await?;
            connection.close().await?;
            return Ok(ApplyOutcome::Duplicate);
        }
        if event.source_sequence != applied + 1 {
            return Err(ProjectionError::SequenceGap {
                expected: applied + 1,
                actual: event.source_sequence,
            });
        }
        if event.privacy_reducing && event.invalidation_id.is_none() {
            return Err(ProjectionError::PrivacyReservationRequired);
        }

        apply_projection_change(&mut transaction, account_id, event, now).await?;
        resolve_invalidation(&mut transaction, account_id, event, now).await?;
        record_applied_event(
            &mut transaction,
            account_id,
            event,
            payload_hash.as_bytes(),
            now,
        )
        .await?;
        transaction.commit().await?;
        connection.close().await?;
        Ok(ApplyOutcome::Applied)
    }

    async fn mark_delivered(
        &self,
        account: &RoutedSqliteAccount,
        sequence: i64,
    ) -> Result<(), ProjectionError> {
        let mut writer = account.acquire_writer().await?;
        sqlx::query(
            "UPDATE projection_outbox_deliveries SET delivery_attempts = delivery_attempts + 1, \
             last_attempt_at = ?, delivered_at = ?, last_error_code = NULL \
             WHERE source_sequence = ?",
        )
        .bind(epoch_millis()?)
        .bind(epoch_millis()?)
        .bind(sequence)
        .execute(writer.connection())
        .await?;
        Ok(())
    }

    async fn mark_delivered_through(
        &self,
        account: &RoutedSqliteAccount,
        applied_sequence: i64,
    ) -> Result<u64, ProjectionError> {
        if applied_sequence == 0 {
            return Ok(0);
        }
        let mut writer = account.acquire_writer().await?;
        let now = epoch_millis()?;
        let result = sqlx::query(
            "UPDATE projection_outbox_deliveries \
             SET delivery_attempts = delivery_attempts + 1, last_attempt_at = ?, \
                 delivered_at = ?, last_error_code = NULL \
             WHERE source_sequence <= ? AND delivered_at IS NULL",
        )
        .bind(now)
        .bind(now)
        .bind(applied_sequence)
        .execute(writer.connection())
        .await?;
        Ok(result.rows_affected())
    }

    async fn management_connection(&self) -> Result<SqliteConnection, sqlx::Error> {
        SqliteConnection::connect_with(
            &SqliteConnectOptions::new()
                .filename(&self.management_path)
                .create_if_missing(false)
                .foreign_keys(true)
                .busy_timeout(self.busy_timeout),
        )
        .await
    }
}

/// Summary of one bounded reconciliation pass.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectionReconciliationReport {
    /// Newly applied events.
    pub applied: u32,
    /// Events already present in the management inbox.
    pub duplicates: u32,
    /// Highest sequence observed in this pass.
    pub last_sequence: Option<i64>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ProjectionEventKind {
    Upsert,
    Invalidate,
    Delete,
}

impl ProjectionEventKind {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Upsert => "upsert",
            Self::Invalidate => "invalidate",
            Self::Delete => "delete",
        }
    }

    fn parse(value: &str) -> Result<Self, ProjectionError> {
        match value {
            "upsert" => Ok(Self::Upsert),
            "invalidate" => Ok(Self::Invalidate),
            "delete" => Ok(Self::Delete),
            _ => Err(ProjectionError::InvalidProjectionPayload),
        }
    }
}

struct StoredProjectionEvent {
    source_sequence: i64,
    event_id: Uuid,
    kind: ProjectionEventKind,
    system_id: SystemId,
    payload_json: String,
    privacy_reducing: bool,
    invalidation_id: Option<Uuid>,
}

impl StoredProjectionEvent {
    fn from_row(row: &sqlx::sqlite::SqliteRow) -> Result<Self, ProjectionError> {
        let event_id = row.get::<Vec<u8>, _>("event_id");
        let system_id = row.get::<Vec<u8>, _>("system_id");
        let invalidation_id = row.get::<Option<Vec<u8>>, _>("invalidation_id");
        Ok(Self {
            source_sequence: row.get("source_sequence"),
            event_id: parse_uuid(&event_id)?,
            kind: ProjectionEventKind::parse(row.get::<String, _>("event_kind").as_str())?,
            system_id: SystemId::from_uuid(parse_uuid(&system_id)?)
                .map_err(|_| ProjectionError::InvalidProjectionPayload)?,
            payload_json: row.get("payload_json"),
            privacy_reducing: row.get("privacy_reducing"),
            invalidation_id: invalidation_id.as_deref().map(parse_uuid).transpose()?,
        })
    }
}

async fn prepare_checkpoint(
    transaction: &mut Transaction<'_, Sqlite>,
    account_id: AccountId,
    now: i64,
) -> Result<i64, sqlx::Error> {
    sqlx::query(
        "INSERT INTO account_projection_checkpoints \
         (account_id, source_sequence, applied_sequence, projected_at) \
         VALUES (?, 0, 0, ?) ON CONFLICT(account_id) DO NOTHING",
    )
    .bind(account_id.as_uuid().as_bytes().as_slice())
    .bind(now)
    .execute(&mut **transaction)
    .await?;
    sqlx::query_scalar(
        "SELECT applied_sequence FROM account_projection_checkpoints WHERE account_id = ?",
    )
    .bind(account_id.as_uuid().as_bytes().as_slice())
    .fetch_one(&mut **transaction)
    .await
}

async fn is_duplicate(
    transaction: &mut Transaction<'_, Sqlite>,
    account_id: AccountId,
    event: &StoredProjectionEvent,
    payload_hash: &[u8; 32],
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM account_projection_inbox \
         WHERE account_id = ? AND source_sequence = ? AND event_id = ? AND payload_hash = ?)",
    )
    .bind(account_id.as_uuid().as_bytes().as_slice())
    .bind(event.source_sequence)
    .bind(event.event_id.as_bytes().as_slice())
    .bind(payload_hash.as_slice())
    .fetch_one(&mut **transaction)
    .await
}

async fn apply_projection_change(
    transaction: &mut Transaction<'_, Sqlite>,
    account_id: AccountId,
    event: &StoredProjectionEvent,
    now: i64,
) -> Result<(), ProjectionError> {
    if event.kind != ProjectionEventKind::Upsert {
        return Ok(delete_projection(transaction, account_id, event.system_id).await?);
    }
    let projection: SystemDiscoveryProjection = serde_json::from_str(&event.payload_json)?;
    let projection = projection.privacy_filtered()?;
    if projection.system_id != event.system_id {
        return Err(ProjectionError::InvalidProjectionPayload);
    }
    if projection.visibility == ProjectionVisibility::Private {
        delete_projection(transaction, account_id, event.system_id).await?;
    } else {
        upsert_projection(
            transaction,
            account_id,
            event.source_sequence,
            now,
            &projection,
        )
        .await?;
    }
    Ok(())
}

async fn resolve_invalidation(
    transaction: &mut Transaction<'_, Sqlite>,
    account_id: AccountId,
    event: &StoredProjectionEvent,
    now: i64,
) -> Result<(), ProjectionError> {
    let Some(invalidation_id) = event.invalidation_id else {
        return Ok(());
    };
    let result = sqlx::query(
        "UPDATE projection_invalidation_reservations \
         SET resolved_sequence = ?, resolved_at = ? \
         WHERE id = ? AND account_id = ? AND system_id = ? AND resolved_sequence IS NULL",
    )
    .bind(event.source_sequence)
    .bind(now)
    .bind(invalidation_id.as_bytes().as_slice())
    .bind(account_id.as_uuid().as_bytes().as_slice())
    .bind(event.system_id.as_uuid().as_bytes().as_slice())
    .execute(&mut **transaction)
    .await?;
    if event.privacy_reducing && result.rows_affected() != 1 {
        return Err(ProjectionError::InvalidPrivacyReservation);
    }
    Ok(())
}

async fn record_applied_event(
    transaction: &mut Transaction<'_, Sqlite>,
    account_id: AccountId,
    event: &StoredProjectionEvent,
    payload_hash: &[u8; 32],
    now: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO account_projection_inbox \
         (account_id, source_sequence, event_id, event_kind, system_id, payload_hash, \
          received_at, applied_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(account_id.as_uuid().as_bytes().as_slice())
    .bind(event.source_sequence)
    .bind(event.event_id.as_bytes().as_slice())
    .bind(event.kind.as_str())
    .bind(event.system_id.as_uuid().as_bytes().as_slice())
    .bind(payload_hash.as_slice())
    .bind(now)
    .bind(now)
    .execute(&mut **transaction)
    .await?;
    sqlx::query(
        "UPDATE account_projection_checkpoints SET source_sequence = MAX(source_sequence, ?), \
         applied_sequence = ?, projected_at = ?, invalidated_at = NULL WHERE account_id = ?",
    )
    .bind(event.source_sequence)
    .bind(event.source_sequence)
    .bind(now)
    .bind(account_id.as_uuid().as_bytes().as_slice())
    .execute(&mut **transaction)
    .await?;
    sqlx::query(
        "UPDATE account_database_registry \
         SET source_sequence = MAX(source_sequence, ?), projected_sequence = ?, updated_at = ? \
         WHERE account_id = ?",
    )
    .bind(event.source_sequence)
    .bind(event.source_sequence)
    .bind(now)
    .bind(account_id.as_uuid().as_bytes().as_slice())
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

async fn upsert_projection(
    transaction: &mut Transaction<'_, Sqlite>,
    account_id: AccountId,
    sequence: i64,
    now: i64,
    projection: &SystemDiscoveryProjection,
) -> Result<(), ProjectionError> {
    let result = sqlx::query(
        "INSERT INTO system_discovery_projections \
         (system_id, account_id, display_name, country_code, location_label, location_precision, \
          capacity_watts, visibility, activity_state, source_sequence, projected_at, invalidated_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, NULL) \
         ON CONFLICT(system_id) DO UPDATE SET \
          display_name = excluded.display_name, country_code = excluded.country_code, \
          location_label = excluded.location_label, location_precision = excluded.location_precision, \
          capacity_watts = excluded.capacity_watts, visibility = excluded.visibility, \
          activity_state = excluded.activity_state, source_sequence = excluded.source_sequence, \
          projected_at = excluded.projected_at, invalidated_at = NULL \
         WHERE system_discovery_projections.account_id = excluded.account_id \
           AND system_discovery_projections.source_sequence < excluded.source_sequence",
    )
    .bind(projection.system_id.as_uuid().as_bytes().as_slice())
    .bind(account_id.as_uuid().as_bytes().as_slice())
    .bind(&projection.display_name)
    .bind(&projection.country_code)
    .bind(&projection.location_label)
    .bind(projection.location_precision.as_str())
    .bind(projection.capacity_watts)
    .bind(projection.visibility.as_str())
    .bind(projection.activity_state.as_str())
    .bind(sequence)
    .bind(now)
    .execute(&mut **transaction)
    .await?;
    if result.rows_affected() != 1 {
        return Err(ProjectionError::ProjectionOwnershipConflict);
    }
    Ok(())
}

async fn delete_projection(
    transaction: &mut Transaction<'_, Sqlite>,
    account_id: AccountId,
    system_id: SystemId,
) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM system_discovery_projections WHERE account_id = ? AND system_id = ?")
        .bind(account_id.as_uuid().as_bytes().as_slice())
        .bind(system_id.as_uuid().as_bytes().as_slice())
        .execute(&mut **transaction)
        .await?;
    Ok(())
}

fn parse_uuid(bytes: &[u8]) -> Result<Uuid, ProjectionError> {
    Uuid::from_slice(bytes).map_err(|_| ProjectionError::InvalidProjectionPayload)
}

fn epoch_millis() -> Result<i64, ProjectionError> {
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| ProjectionError::ClockOutOfRange)?
        .as_millis();
    i64::try_from(millis).map_err(|_| ProjectionError::ClockOutOfRange)
}

enum ApplyOutcome {
    Applied,
    Duplicate,
}

/// Failure while recording or reconciling cross-database projections.
#[derive(Debug, Error)]
pub enum ProjectionError {
    /// Account or management database access failed.
    #[error("projection database operation failed: {0}")]
    Sqlx(#[from] sqlx::Error),
    /// Routed account access failed.
    #[error(transparent)]
    Routing(#[from] SqliteRoutingError),
    /// Projection serialization or parsing failed.
    #[error("projection payload serialization failed: {0}")]
    Json(#[from] serde_json::Error),
    /// Projection fields exceed the privacy-safe schema.
    #[error("invalid privacy-safe projection payload")]
    InvalidProjectionPayload,
    /// A privacy-reducing event was not preceded by management invalidation.
    #[error("privacy-reducing projection events require an invalidation reservation")]
    PrivacyReservationRequired,
    /// The referenced invalidation reservation is missing, mismatched, or already consumed.
    #[error("invalid or already consumed projection invalidation reservation")]
    InvalidPrivacyReservation,
    /// The next account event is absent.
    #[error("projection sequence gap: expected {expected}, received {actual}")]
    SequenceGap {
        /// Required next sequence.
        expected: i64,
        /// Observed out-of-order sequence.
        actual: i64,
    },
    /// An applied sequence was replayed with different identity or content.
    #[error("conflicting projection event at sequence {0}")]
    SequenceConflict(i64),
    /// A system identifier is already owned by another account projection.
    #[error("system projection ownership conflict")]
    ProjectionOwnershipConflict,
    /// System time cannot be represented as epoch milliseconds.
    #[error("system clock is outside the supported timestamp range")]
    ClockOutOfRange,
}
