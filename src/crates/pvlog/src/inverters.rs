//! Runtime adapter for nested inverter/string aggregate resources.

use std::sync::Arc;

use async_trait::async_trait;
use pvlog_api::{
    InverterApiError, InverterApiUseCases, InverterInput, InverterResponse, PvStringInput,
    PvStringResponse,
};
use pvlog_application::{EquipmentCatalog, confirm_inverter_snapshot, confirm_string_composition};
use pvlog_domain::{AccountId, EquipmentValueProvenance, InverterId, StringId, SystemId, UserId};
use pvlog_storage::{
    AccountConfigurationRepository, DatabaseTarget, InverterRecord,
    PostgresAccountConfigurationRepository, PvStringRecord, SqliteAccountConfigurationRepository,
    SqliteAccountPoolConfig, SqliteAccountPoolRouter,
};

#[derive(Clone, Debug)]
pub struct ManagementInverterApi {
    target: DatabaseTarget,
    catalog: Arc<EquipmentCatalog>,
}

impl ManagementInverterApi {
    #[must_use]
    pub fn new(target: DatabaseTarget, catalog: Arc<EquipmentCatalog>) -> Self {
        Self { target, catalog }
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

    async fn save(
        &self,
        account_id: AccountId,
        system_id: SystemId,
        inverter_id: Option<InverterId>,
        input: InverterInput,
    ) -> Result<InverterResponse, InverterApiError> {
        validate(&input)?;
        let now = now();
        let id = inverter_id.unwrap_or_default();
        let repository = self.repository(account_id).await?;
        if inverter_id.is_some()
            && !repository
                .effective_inverters(system_id, now)
                .await
                .map_err(|_| InverterApiError::Unavailable)?
                .iter()
                .any(|record| record.id == id)
        {
            return Err(InverterApiError::Forbidden);
        }
        let inverter_snapshot = input
            .specification_snapshot
            .map(|snapshot| confirm_inverter_snapshot(&self.catalog, snapshot))
            .transpose()
            .map_err(|_| InverterApiError::InvalidInput("specificationSnapshot"))?;
        let template = inverter_snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.template.as_ref());
        let value_provenance = template.map_or(EquipmentValueProvenance::Manual, |template| {
            template.value_provenance
        });
        if input
            .value_provenance
            .is_some_and(|value| value != value_provenance)
        {
            return Err(InverterApiError::InvalidInput("valueProvenance"));
        }
        let strings = input
            .strings
            .into_iter()
            .map(|string| build_string(&self.catalog, id, string, now))
            .collect::<Result<Vec<_>, _>>()?;
        let record = InverterRecord {
            id,
            system_id,
            name: input.name,
            manufacturer: input.manufacturer,
            model: input.model,
            serial_reference: input.serial_reference,
            rated_power_watts: input.rated_power_watts,
            catalog_entry_id: template.map(|template| template.entry_id.0.clone()),
            catalog_revision: template.map(|template| template.revision.0.clone()),
            value_provenance,
            specification_snapshot: inverter_snapshot,
            effective_from: input.effective_from,
            effective_to: input.effective_to,
            created_at: now,
            updated_at: now,
            strings,
        };
        repository
            .save_inverter_aggregate(&record)
            .await
            .map_err(|error| match error {
                pvlog_storage::AccountRepositoryError::InvalidRecord(_)
                | pvlog_storage::AccountRepositoryError::InvalidEffectiveRange => {
                    InverterApiError::InvalidInput("equipment")
                }
                _ => InverterApiError::Unavailable,
            })?;
        Ok(response(record))
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
        self.save(account_id, system_id, None, input).await
    }

    async fn update(
        &self,
        _actor: UserId,
        account_id: AccountId,
        system_id: SystemId,
        inverter_id: InverterId,
        input: InverterInput,
    ) -> Result<InverterResponse, InverterApiError> {
        self.save(account_id, system_id, Some(inverter_id), input)
            .await
    }

    async fn delete(
        &self,
        _actor: UserId,
        account_id: AccountId,
        system_id: SystemId,
        inverter_id: InverterId,
    ) -> Result<(), InverterApiError> {
        if self
            .repository(account_id)
            .await?
            .delete_inverter_aggregate(system_id, inverter_id)
            .await
            .map_err(|_| InverterApiError::Unavailable)?
        {
            Ok(())
        } else {
            Err(InverterApiError::NotFound)
        }
    }
}

fn build_string(
    catalog: &EquipmentCatalog,
    inverter_id: InverterId,
    string: PvStringInput,
    now: i64,
) -> Result<PvStringRecord, InverterApiError> {
    let (snapshot, value_provenance) = if let Some(snapshot) = string.module_specification_snapshot
    {
        let composition = confirm_string_composition(catalog, string.panel_count, snapshot)
            .map_err(|_| InverterApiError::InvalidInput("moduleSpecificationSnapshot"))?;
        let peak = i64::from(composition.module.specification.peak_power_watts);
        if string.module_peak_power_watts != peak {
            return Err(InverterApiError::InvalidInput("modulePeakPowerWatts"));
        }
        let provenance = composition
            .module
            .template
            .as_ref()
            .map_or(EquipmentValueProvenance::Manual, |template| {
                template.value_provenance
            });
        (Some(composition.module), provenance)
    } else {
        (None, EquipmentValueProvenance::Manual)
    };
    let total_peak_power_watts = i64::from(string.panel_count)
        .checked_mul(string.module_peak_power_watts)
        .ok_or(InverterApiError::InvalidInput("modulePeakPowerWatts"))?;
    if string
        .value_provenance
        .is_some_and(|value| value != value_provenance)
    {
        return Err(InverterApiError::InvalidInput("valueProvenance"));
    }
    let template = snapshot
        .as_ref()
        .and_then(|snapshot| snapshot.template.as_ref());
    Ok(PvStringRecord {
        id: StringId::new(),
        inverter_id,
        name: string.name,
        panel_count: string.panel_count,
        panel_manufacturer: string.panel_manufacturer,
        panel_model: string.panel_model,
        rated_power_watts: total_peak_power_watts,
        module_catalog_entry_id: template.map(|template| template.entry_id.0.clone()),
        module_catalog_revision: template.map(|template| template.revision.0.clone()),
        value_provenance,
        module_specification_snapshot: snapshot,
        module_peak_power_watts: Some(string.module_peak_power_watts),
        total_peak_power_watts: Some(total_peak_power_watts),
        orientation_degrees: string.orientation_degrees,
        tilt_degrees: string.tilt_degrees,
        effective_from: string.effective_from,
        effective_to: string.effective_to,
        created_at: now,
        updated_at: now,
    })
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
                || string.module_peak_power_watts <= 0
                || string.orientation_degrees.is_some_and(|value| value > 359)
                || string.tilt_degrees.is_some_and(|value| value > 90)
                || string
                    .effective_to
                    .is_some_and(|value| value <= string.effective_from)
        })
    {
        return Err(InverterApiError::InvalidInput("equipment"));
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
        catalog_entry_id: record.catalog_entry_id,
        catalog_revision: record.catalog_revision,
        value_provenance: record.value_provenance,
        specification_snapshot: record.specification_snapshot,
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
                module_catalog_entry_id: string.module_catalog_entry_id,
                module_catalog_revision: string.module_catalog_revision,
                value_provenance: string.value_provenance,
                module_specification_snapshot: string.module_specification_snapshot,
                module_peak_power_watts: string.module_peak_power_watts,
                total_peak_power_watts: string.total_peak_power_watts,
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
