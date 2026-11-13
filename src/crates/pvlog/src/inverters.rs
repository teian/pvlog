//! Runtime adapter for nested inverter/string aggregate resources.

use std::sync::Arc;

use async_trait::async_trait;
use pvlog_api::{
    InverterApiError, InverterApiUseCases, InverterInput, InverterResponse, PvStringInput,
    PvStringResponse,
};
use pvlog_application::{EquipmentCatalog, confirm_inverter_snapshot, confirm_string_composition};
use pvlog_domain::{
    AccountId, EquipmentValueProvenance, InverterId, StringId, StringShading, SystemId,
    TEMPERATURE_COEFFICIENT_PPM_RANGE, UserId,
};
use pvlog_storage::{
    AccountConfigurationRepository, DatabaseTarget, InverterRecord,
    PostgresAccountConfigurationRepository, PvStringRecord, SqliteAccountConfigurationRepository,
    SqliteAccountPoolRouter,
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
                    let router = SqliteAccountPoolRouter::shared(
                        management_path.clone(),
                        accounts_dir.clone(),
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

    /// Builds the record an input would be stored as, checking it against the catalog.
    fn prepare(
        &self,
        system_id: SystemId,
        id: InverterId,
        input: InverterInput,
        now: i64,
    ) -> Result<InverterRecord, InverterApiError> {
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
        Ok(InverterRecord {
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
            firmware_version: None,
            firmware_changed_at: None,
            strings,
        })
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
        // A string identifier must name a string this inverter already has;
        // anything else would adopt or invent a string under a chosen identifier.
        let submitted: Vec<StringId> = input
            .strings
            .iter()
            .filter_map(|string| string.id)
            .collect();
        if !submitted.is_empty() {
            let known = match inverter_id {
                Some(_) => repository
                    .inverter_string_ids(id)
                    .await
                    .map_err(|_| InverterApiError::Unavailable)?,
                None => Vec::new(),
            };
            let mut seen = std::collections::BTreeSet::new();
            if submitted
                .iter()
                .any(|string| !known.contains(string) || !seen.insert(*string))
            {
                return Err(InverterApiError::InvalidInput("strings.id"));
            }
        }
        let record = self.prepare(system_id, id, input, now)?;
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
    fn validate(&self, input: &InverterInput) -> Result<(), InverterApiError> {
        validate(input)?;
        self.prepare(SystemId::new(), InverterId::new(), input.clone(), now())
            .map(|_| ())
    }

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
        id: string.id.unwrap_or_default(),
        inverter_id,
        name: string.name,
        panel_count: string.panel_count,
        // A blank identity is no identity; forecast readiness reports it as missing.
        panel_manufacturer: string
            .panel_manufacturer
            .filter(|value| !value.trim().is_empty()),
        panel_model: string.panel_model.filter(|value| !value.trim().is_empty()),
        rated_power_watts: total_peak_power_watts,
        module_catalog_entry_id: template.map(|template| template.entry_id.0.clone()),
        module_catalog_revision: template.map(|template| template.revision.0.clone()),
        value_provenance,
        module_specification_snapshot: snapshot,
        module_peak_power_watts: Some(string.module_peak_power_watts),
        total_peak_power_watts: Some(total_peak_power_watts),
        orientation_degrees: string.orientation_degrees,
        tilt_degrees: string.tilt_degrees,
        temperature_coefficient_ppm_per_celsius: string.temperature_coefficient_ppm_per_celsius,
        shading: string.shading,
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
    for string in &input.strings {
        if string
            .temperature_coefficient_ppm_per_celsius
            .is_some_and(|value| !TEMPERATURE_COEFFICIENT_PPM_RANGE.contains(&value))
        {
            return Err(InverterApiError::InvalidInput(
                "temperatureCoefficientPpmPerCelsius",
            ));
        }
        if string.shading.len() > StringShading::MAXIMUM_ENTRIES
            || string.shading.iter().any(|entry| !entry.is_valid())
        {
            return Err(InverterApiError::InvalidInput("shading"));
        }
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
        firmware_version: record.firmware_version,
        firmware_changed_at: record.firmware_changed_at,
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
                temperature_coefficient_ppm_per_celsius: string
                    .temperature_coefficient_ppm_per_celsius,
                shading: string.shading,
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
