//! Runtime adapter for nested inverter/string aggregate resources.

use async_trait::async_trait;
use pvlog_api::{
    InverterApiError, InverterApiUseCases, InverterInput, InverterResponse, PvStringResponse,
};
use pvlog_domain::{AccountId, InverterId, StringId, SystemId, UserId};
use pvlog_storage::{
    AccountConfigurationRepository, DatabaseTarget, InverterRecord,
    PostgresAccountConfigurationRepository, PvStringRecord, SqliteAccountConfigurationRepository,
    SqliteAccountPoolConfig, SqliteAccountPoolRouter,
};

#[derive(Clone, Debug)]
pub struct ManagementInverterApi {
    target: DatabaseTarget,
}

impl ManagementInverterApi {
    #[must_use]
    pub fn new(target: DatabaseTarget) -> Self {
        Self { target }
    }

    async fn repository(
        &self,
        account_id: AccountId,
    ) -> Result<Box<dyn AccountConfigurationRepository>, InverterApiError> {
        match &self.target {
            DatabaseTarget::Sqlite {
                management_path,
                accounts_dir,
            } => {
                #[cfg(feature = "sqlite")]
                {
                    let router = SqliteAccountPoolRouter::new(
                        management_path.clone(),
                        accounts_dir.clone(),
                        SqliteAccountPoolConfig::default(),
                    )
                    .map_err(|_| InverterApiError::Unavailable)?;
                    let account = router
                        .route(account_id)
                        .await
                        .map_err(|_| InverterApiError::Unavailable)?;
                    Ok(Box::new(SqliteAccountConfigurationRepository::new(account)))
                }
                #[cfg(not(feature = "sqlite"))]
                {
                    let _ = (management_path, accounts_dir);
                    Err(InverterApiError::Unavailable)
                }
            }
            DatabaseTarget::Postgres { url } => {
                #[cfg(feature = "postgres")]
                {
                    Ok(Box::new(PostgresAccountConfigurationRepository::new(
                        url.clone(),
                        account_id,
                    )))
                }
                #[cfg(not(feature = "postgres"))]
                {
                    let _ = url;
                    Err(InverterApiError::Unavailable)
                }
            }
        }
    }
}

#[async_trait]
impl InverterApiUseCases for ManagementInverterApi {
    async fn list(
        &self,
        account_id: AccountId,
        system_id: SystemId,
        at: i64,
    ) -> Result<Vec<InverterResponse>, InverterApiError> {
        let records = self
            .repository(account_id)
            .await?
            .effective_inverters(system_id, at)
            .await
            .map_err(|_| InverterApiError::Unavailable)?;
        Ok(records.into_iter().map(response).collect())
    }

    async fn create(
        &self,
        _actor: UserId,
        account_id: AccountId,
        system_id: SystemId,
        input: InverterInput,
    ) -> Result<InverterResponse, InverterApiError> {
        validate(&input)?;
        let now = now();
        let id = InverterId::new();
        let record = InverterRecord {
            id,
            system_id,
            name: input.name,
            manufacturer: input.manufacturer,
            model: input.model,
            serial_reference: input.serial_reference,
            rated_power_watts: input.rated_power_watts,
            effective_from: input.effective_from,
            effective_to: input.effective_to,
            created_at: now,
            updated_at: now,
            strings: input
                .strings
                .into_iter()
                .map(|string| PvStringRecord {
                    id: StringId::new(),
                    inverter_id: id,
                    name: string.name,
                    panel_count: string.panel_count,
                    panel_manufacturer: string.panel_manufacturer,
                    panel_model: string.panel_model,
                    rated_power_watts: string.rated_power_watts,
                    orientation_degrees: string.orientation_degrees,
                    tilt_degrees: string.tilt_degrees,
                    effective_from: string.effective_from,
                    effective_to: string.effective_to,
                    created_at: now,
                    updated_at: now,
                })
                .collect(),
        };
        self.repository(account_id)
            .await?
            .save_inverter_aggregate(&record)
            .await
            .map_err(|_| InverterApiError::Unavailable)?;
        Ok(response(record))
    }
}

fn validate(input: &InverterInput) -> Result<(), InverterApiError> {
    if input.name.trim().is_empty()
        || input.strings.is_empty()
        || input
            .effective_to
            .is_some_and(|value| value <= input.effective_from)
        || input.strings.iter().any(|string| {
            string.name.trim().is_empty()
                || string.panel_count == 0
                || string.rated_power_watts <= 0
                || string.orientation_degrees.is_some_and(|value| value > 359)
                || string.tilt_degrees.is_some_and(|value| value > 90)
                || string
                    .effective_to
                    .is_some_and(|value| value <= string.effective_from)
        })
    {
        return Err(InverterApiError::InvalidInput);
    }
    Ok(())
}

fn response(record: InverterRecord) -> InverterResponse {
    InverterResponse {
        id: record.id,
        system_id: record.system_id,
        name: record.name,
        manufacturer: record.manufacturer,
        model: record.model,
        serial_reference: record.serial_reference,
        rated_power_watts: record.rated_power_watts,
        effective_from: record.effective_from,
        effective_to: record.effective_to,
        version: 1,
        strings: record
            .strings
            .into_iter()
            .map(|string| PvStringResponse {
                id: string.id,
                inverter_id: string.inverter_id,
                name: string.name,
                panel_count: string.panel_count,
                panel_manufacturer: string.panel_manufacturer,
                panel_model: string.panel_model,
                rated_power_watts: string.rated_power_watts,
                orientation_degrees: string.orientation_degrees,
                tilt_degrees: string.tilt_degrees,
                effective_from: string.effective_from,
                effective_to: string.effective_to,
            })
            .collect(),
    }
}

fn now() -> i64 {
    let value = time::OffsetDateTime::now_utc();
    value.unix_timestamp() * 1_000 + i64::from(value.nanosecond() / 1_000_000)
}
