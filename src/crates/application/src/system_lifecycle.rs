//! Audited PV-system lifecycle with safe defaults and optimistic concurrency.

use crate::{Clock, PortError, RbacRepository, timezone_for_country};
use async_trait::async_trait;
use pvlog_domain::{
    AccountId, BuiltInRole, IanaTimezone, PrincipalId, RoleAssignment, RoleAssignmentId, RoleKind,
    RoleScope, SystemId, SystemLifecycle, UserId, Visibility,
};
use serde::Serialize;
use std::{str::FromStr as _, sync::Arc};
use thiserror::Error;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemLifecycleRecord {
    pub id: SystemId,
    pub account_id: AccountId,
    pub name: String,
    /// Free-text placement shown beside the name; empty when unset.
    pub description: String,
    /// Local commissioning date as `YYYY-MM-DD`.
    pub commissioning_date: Option<String>,
    pub timezone: String,
    /// Label of the place the owner picked, shown next to the coordinates.
    pub location_label: Option<String>,
    /// ISO 3166-1 alpha-2 code of the country the location lies in.
    pub country_code: Option<String>,
    pub latitude_microdegrees: Option<i32>,
    pub longitude_microdegrees: Option<i32>,
    /// Expected seconds between readings; drives staleness detection.
    pub status_interval_seconds: u32,
    /// Capacity the owner stated; used until strings define the capacity.
    pub declared_capacity_watts: Option<i64>,
    pub visibility: Visibility,
    pub lifecycle: SystemLifecycle,
    pub version: u64,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Optional system fields that can be supplied at creation and changed later.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SystemDetails {
    pub description: String,
    pub commissioning_date: Option<String>,
    pub location_label: Option<String>,
    /// ISO 3166-1 alpha-2 code of the country the location lies in.
    pub country_code: Option<String>,
    pub latitude_microdegrees: Option<i32>,
    pub longitude_microdegrees: Option<i32>,
    /// `None` keeps the current interval, or the default for a new system.
    pub status_interval_seconds: Option<u32>,
    pub declared_capacity_watts: Option<i64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreateSystem {
    pub account_id: AccountId,
    pub actor: UserId,
    pub name: String,
    /// Timezone the owner chose explicitly; wins over every derived value.
    pub timezone: Option<String>,
    /// Timezone the client runs in; used when neither an explicit timezone nor
    /// the location's country decides it.
    pub client_timezone: Option<String>,
    pub details: SystemDetails,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UpdateSystem {
    pub id: SystemId,
    pub actor: UserId,
    pub expected_version: u64,
    pub name: String,
    pub timezone: String,
    pub visibility: Visibility,
    pub details: SystemDetails,
}

/// Default interval between readings for a system that states none.
pub const DEFAULT_STATUS_INTERVAL_SECONDS: u32 = 300;
/// Bounds the storage schema enforces for the status interval.
pub const STATUS_INTERVAL_SECONDS_RANGE: std::ops::RangeInclusive<u32> = 30..=86_400;

#[async_trait]
pub trait SystemLifecycleRepository: Send + Sync {
    async fn system(&self, id: SystemId) -> Result<Option<SystemLifecycleRecord>, PortError>;
    async fn create(&self, record: SystemLifecycleRecord) -> Result<(), PortError>;
    async fn save(
        &self,
        record: SystemLifecycleRecord,
        expected_version: u64,
    ) -> Result<bool, PortError>;
    async fn delete(&self, id: SystemId, expected_version: u64) -> Result<bool, PortError>;
    async fn audit(
        &self,
        actor: UserId,
        id: SystemId,
        action: &'static str,
        outcome: &'static str,
        now: i64,
    ) -> Result<(), PortError>;
}

#[async_trait]
pub trait SystemLifecycleUseCases: Send + Sync {
    async fn system(&self, id: SystemId) -> Result<SystemLifecycleRecord, SystemLifecycleError>;
    async fn create_system(
        &self,
        request: CreateSystem,
    ) -> Result<SystemLifecycleRecord, SystemLifecycleError>;
    async fn update_system(
        &self,
        request: UpdateSystem,
    ) -> Result<SystemLifecycleRecord, SystemLifecycleError>;
    async fn archive_system(
        &self,
        id: SystemId,
        actor: UserId,
        version: u64,
    ) -> Result<SystemLifecycleRecord, SystemLifecycleError>;
    async fn restore_system(
        &self,
        id: SystemId,
        actor: UserId,
        version: u64,
    ) -> Result<SystemLifecycleRecord, SystemLifecycleError>;
    async fn delete_system(
        &self,
        id: SystemId,
        actor: UserId,
        version: u64,
        confirmed: bool,
    ) -> Result<(), SystemLifecycleError>;
}

pub struct SystemLifecycleService {
    repository: Arc<dyn SystemLifecycleRepository>,
    rbac_repository: Arc<dyn RbacRepository>,
    clock: Arc<dyn Clock>,
}
impl SystemLifecycleService {
    #[must_use]
    pub fn new(
        repository: Arc<dyn SystemLifecycleRepository>,
        rbac_repository: Arc<dyn RbacRepository>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            repository,
            rbac_repository,
            clock,
        }
    }

    /// Creates a private active system with validated required fields.
    /// # Errors
    /// Returns an error for invalid input, time, or persistence failure.
    pub async fn create(
        &self,
        request: CreateSystem,
    ) -> Result<SystemLifecycleRecord, SystemLifecycleError> {
        let timezone = resolve_timezone(&request)?;
        validate(&request.name, &timezone, &request.details)?;
        let owner_role_id = self.owner_role_id(request.account_id).await?;
        let granted_at = self.clock.now();
        let now =
            i64::try_from(granted_at.epoch_millis()).map_err(|_| SystemLifecycleError::Time)?;
        let record = SystemLifecycleRecord {
            id: SystemId::new(),
            account_id: request.account_id,
            name: request.name,
            description: request.details.description,
            commissioning_date: request.details.commissioning_date,
            timezone,
            location_label: normalized_label(request.details.location_label),
            country_code: normalized_country(request.details.country_code),
            latitude_microdegrees: request.details.latitude_microdegrees,
            longitude_microdegrees: request.details.longitude_microdegrees,
            status_interval_seconds: request
                .details
                .status_interval_seconds
                .unwrap_or(DEFAULT_STATUS_INTERVAL_SECONDS),
            declared_capacity_watts: request.details.declared_capacity_watts,
            visibility: Visibility::Private,
            lifecycle: SystemLifecycle::Active,
            version: 1,
            created_at: now,
            updated_at: now,
        };
        self.repository
            .create(record.clone())
            .await
            .map_err(SystemLifecycleError::Repository)?;
        if let Err(error) = self
            .rbac_repository
            .save_assignment(&RoleAssignment {
                id: RoleAssignmentId::new(),
                principal: PrincipalId::User(request.actor),
                role_id: owner_role_id,
                scope: RoleScope::System {
                    account_id: request.account_id,
                    system_id: record.id,
                },
                granted_by: request.actor,
                granted_at,
                expires_at: None,
            })
            .await
        {
            let _ = self.repository.delete(record.id, record.version).await;
            return Err(SystemLifecycleError::Repository(error));
        }
        self.repository
            .audit(request.actor, record.id, "system.created", "succeeded", now)
            .await
            .map_err(SystemLifecycleError::Repository)?;
        Ok(record)
    }

    async fn owner_role_id(
        &self,
        account_id: AccountId,
    ) -> Result<pvlog_domain::RoleId, SystemLifecycleError> {
        self.rbac_repository
            .roles(Some(account_id))
            .await
            .map_err(SystemLifecycleError::Repository)?
            .into_iter()
            .find(|record| record.role.kind == RoleKind::BuiltIn(BuiltInRole::AccountOwner))
            .map(|record| record.role.id)
            .ok_or_else(|| {
                SystemLifecycleError::Repository(PortError::Rejected(
                    "account_owner_role_missing".to_owned(),
                ))
            })
    }
    /// Updates mutable system fields using an expected version.
    /// # Errors
    /// Returns an error for invalid input, missing system, conflict, or persistence failure.
    pub async fn update(
        &self,
        request: UpdateSystem,
    ) -> Result<SystemLifecycleRecord, SystemLifecycleError> {
        validate(&request.name, &request.timezone, &request.details)?;
        let mut record = self.load(request.id).await?;
        record.name = request.name;
        record.description = request.details.description;
        record.commissioning_date = request.details.commissioning_date;
        record.timezone = request.timezone;
        record.location_label = normalized_label(request.details.location_label);
        record.country_code = normalized_country(request.details.country_code);
        record.latitude_microdegrees = request.details.latitude_microdegrees;
        record.longitude_microdegrees = request.details.longitude_microdegrees;
        if let Some(interval) = request.details.status_interval_seconds {
            record.status_interval_seconds = interval;
        }
        record.declared_capacity_watts = request.details.declared_capacity_watts;
        record.visibility = request.visibility;
        record.updated_at = self.now()?;
        record.version = request.expected_version.saturating_add(1);
        self.persist(
            record,
            request.expected_version,
            request.actor,
            "system.updated",
        )
        .await
    }
    /// Archives an active system without deleting data.
    /// # Errors
    /// Returns an error for missing system, conflict, or persistence failure.
    pub async fn archive(
        &self,
        id: SystemId,
        actor: UserId,
        version: u64,
    ) -> Result<SystemLifecycleRecord, SystemLifecycleError> {
        self.transition(
            id,
            actor,
            version,
            SystemLifecycle::Archived,
            "system.archived",
        )
        .await
    }
    /// Restores an archived system.
    /// # Errors
    /// Returns an error for missing system, conflict, or persistence failure.
    pub async fn restore(
        &self,
        id: SystemId,
        actor: UserId,
        version: u64,
    ) -> Result<SystemLifecycleRecord, SystemLifecycleError> {
        self.transition(
            id,
            actor,
            version,
            SystemLifecycle::Active,
            "system.restored",
        )
        .await
    }
    /// Permanently deletes only after explicit confirmation.
    /// # Errors
    /// Returns an error for missing confirmation, conflict, or persistence failure.
    pub async fn delete(
        &self,
        id: SystemId,
        actor: UserId,
        version: u64,
        confirmed: bool,
    ) -> Result<(), SystemLifecycleError> {
        if !confirmed {
            return Err(SystemLifecycleError::ConfirmationRequired);
        }
        if !self
            .repository
            .delete(id, version)
            .await
            .map_err(SystemLifecycleError::Repository)?
        {
            return Err(SystemLifecycleError::Conflict);
        }
        self.repository
            .audit(actor, id, "system.deleted", "succeeded", self.now()?)
            .await
            .map_err(SystemLifecycleError::Repository)
    }
    async fn transition(
        &self,
        id: SystemId,
        actor: UserId,
        version: u64,
        lifecycle: SystemLifecycle,
        action: &'static str,
    ) -> Result<SystemLifecycleRecord, SystemLifecycleError> {
        let mut record = self.load(id).await?;
        record.lifecycle = lifecycle;
        record.version = version.saturating_add(1);
        record.updated_at = self.now()?;
        self.persist(record, version, actor, action).await
    }
    async fn persist(
        &self,
        record: SystemLifecycleRecord,
        version: u64,
        actor: UserId,
        action: &'static str,
    ) -> Result<SystemLifecycleRecord, SystemLifecycleError> {
        if !self
            .repository
            .save(record.clone(), version)
            .await
            .map_err(SystemLifecycleError::Repository)?
        {
            return Err(SystemLifecycleError::Conflict);
        }
        self.repository
            .audit(actor, record.id, action, "succeeded", record.updated_at)
            .await
            .map_err(SystemLifecycleError::Repository)?;
        Ok(record)
    }
    async fn load(&self, id: SystemId) -> Result<SystemLifecycleRecord, SystemLifecycleError> {
        self.repository
            .system(id)
            .await
            .map_err(SystemLifecycleError::Repository)?
            .ok_or(SystemLifecycleError::NotFound)
    }
    fn now(&self) -> Result<i64, SystemLifecycleError> {
        i64::try_from(self.clock.now().epoch_millis()).map_err(|_| SystemLifecycleError::Time)
    }
}

#[async_trait]
impl SystemLifecycleUseCases for SystemLifecycleService {
    async fn system(&self, id: SystemId) -> Result<SystemLifecycleRecord, SystemLifecycleError> {
        self.load(id).await
    }

    async fn create_system(
        &self,
        request: CreateSystem,
    ) -> Result<SystemLifecycleRecord, SystemLifecycleError> {
        self.create(request).await
    }
    async fn update_system(
        &self,
        request: UpdateSystem,
    ) -> Result<SystemLifecycleRecord, SystemLifecycleError> {
        self.update(request).await
    }
    async fn archive_system(
        &self,
        id: SystemId,
        actor: UserId,
        version: u64,
    ) -> Result<SystemLifecycleRecord, SystemLifecycleError> {
        self.archive(id, actor, version).await
    }
    async fn restore_system(
        &self,
        id: SystemId,
        actor: UserId,
        version: u64,
    ) -> Result<SystemLifecycleRecord, SystemLifecycleError> {
        self.restore(id, actor, version).await
    }
    async fn delete_system(
        &self,
        id: SystemId,
        actor: UserId,
        version: u64,
        confirmed: bool,
    ) -> Result<(), SystemLifecycleError> {
        self.delete(id, actor, version, confirmed).await
    }
}
/// Picks the explicit timezone, else the one of the location's country, else the client's.
fn resolve_timezone(request: &CreateSystem) -> Result<String, SystemLifecycleError> {
    if let Some(timezone) = &request.timezone {
        return Ok(timezone.clone());
    }
    let position = request
        .details
        .latitude_microdegrees
        .zip(request.details.longitude_microdegrees);
    request
        .details
        .country_code
        .as_deref()
        .and_then(|country| {
            timezone_for_country(country, position, request.client_timezone.as_deref())
        })
        .or_else(|| request.client_timezone.clone())
        .ok_or(SystemLifecycleError::InvalidField("timezone_required"))
}

/// Checks every supplied field and names the first one that is invalid.
fn validate(
    name: &str,
    timezone: &str,
    details: &SystemDetails,
) -> Result<(), SystemLifecycleError> {
    let invalid = SystemLifecycleError::InvalidField;
    if name.trim().is_empty() {
        return Err(invalid("name_required"));
    }
    if IanaTimezone::from_str(timezone).is_err() {
        return Err(invalid("timezone_unknown"));
    }
    match (
        details.latitude_microdegrees,
        details.longitude_microdegrees,
    ) {
        (None, None) => {}
        (Some(latitude), Some(longitude)) => {
            if !(-90_000_000..=90_000_000).contains(&latitude) {
                return Err(invalid("latitude_out_of_range"));
            }
            if !(-180_000_000..=180_000_000).contains(&longitude) {
                return Err(invalid("longitude_out_of_range"));
            }
        }
        _ => return Err(invalid("coordinates_incomplete")),
    }
    if details
        .country_code
        .as_deref()
        .is_some_and(|code| code.len() != 2 || !code.bytes().all(|byte| byte.is_ascii_alphabetic()))
    {
        return Err(invalid("country_code_invalid"));
    }
    if details
        .status_interval_seconds
        .is_some_and(|interval| !STATUS_INTERVAL_SECONDS_RANGE.contains(&interval))
    {
        return Err(invalid("status_interval_out_of_range"));
    }
    if details
        .declared_capacity_watts
        .is_some_and(|capacity| capacity <= 0)
    {
        return Err(invalid("declared_capacity_not_positive"));
    }
    if details.commissioning_date.as_deref().is_some_and(|date| {
        time::Date::parse(
            date,
            time::macros::format_description!("[year]-[month]-[day]"),
        )
        .is_err()
    }) {
        return Err(invalid("commissioning_date_invalid"));
    }
    Ok(())
}

/// Stores country codes in upper case.
fn normalized_country(code: Option<String>) -> Option<String> {
    code.map(|value| value.to_ascii_uppercase())
}

/// Stores a blank location label as absent.
fn normalized_label(label: Option<String>) -> Option<String> {
    label
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

#[derive(Debug, Error)]
pub enum SystemLifecycleError {
    #[error("system input is invalid")]
    InvalidInput,
    /// A named field failed validation; the code identifies field and reason.
    #[error("system field is invalid: {0}")]
    InvalidField(&'static str),
    #[error("system was not found")]
    NotFound,
    #[error("system version conflict")]
    Conflict,
    #[error("explicit destructive confirmation is required")]
    ConfirmationRequired,
    #[error("clock value is invalid")]
    Time,
    #[error("system persistence is unavailable")]
    Repository(PortError),
}
