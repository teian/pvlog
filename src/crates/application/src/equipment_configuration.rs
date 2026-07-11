//! Validation and confirmation of editable catalog-prefilled equipment values.

use pvlog_domain::{
    EquipmentValueProvenance, InverterSpecificationSnapshot, PvStringModuleComposition,
    SolarModuleSpecificationSnapshot,
};
use thiserror::Error;

use crate::EquipmentCatalog;

const MAXIMUM_MODULE_COUNT: u32 = 10_000;
const MAXIMUM_MODULE_POWER_WATTS: u32 = 10_000;
const MAXIMUM_STRING_POWER_WATTS: u64 = 100_000_000;

/// Validates confirmed inverter values and classifies optional catalog provenance.
///
/// # Errors
/// Returns an error for incomplete manual identity or an unknown/stale template reference.
pub fn confirm_inverter_snapshot(
    catalog: &EquipmentCatalog,
    mut snapshot: InverterSpecificationSnapshot,
) -> Result<InverterSpecificationSnapshot, EquipmentConfigurationError> {
    validate_identity(&snapshot.manufacturer, &snapshot.model)?;
    if let Some(template) = &mut snapshot.template {
        let entry = catalog
            .inverter(&template.entry_id)
            .filter(|entry| entry.revision == template.revision)
            .ok_or(EquipmentConfigurationError::UnknownTemplate)?;
        template.value_provenance = if entry.manufacturer == snapshot.manufacturer
            && entry.model == snapshot.model
            && entry.dc == snapshot.dc
            && entry.ac == snapshot.ac
            && entry.operational == snapshot.operational
        {
            EquipmentValueProvenance::CatalogCopied
        } else {
            EquipmentValueProvenance::CatalogCustomized
        };
    }
    Ok(snapshot)
}

/// Validates confirmed module values and classifies optional catalog provenance.
///
/// # Errors
/// Returns an error for incomplete manual identity or an unknown/stale template reference.
pub fn confirm_module_snapshot(
    catalog: &EquipmentCatalog,
    mut snapshot: SolarModuleSpecificationSnapshot,
) -> Result<SolarModuleSpecificationSnapshot, EquipmentConfigurationError> {
    validate_identity(&snapshot.manufacturer, &snapshot.model)?;
    if snapshot.specification.peak_power_watts == 0
        || snapshot.specification.peak_power_watts > MAXIMUM_MODULE_POWER_WATTS
    {
        return Err(EquipmentConfigurationError::InvalidModulePower);
    }
    if let Some(template) = &mut snapshot.template {
        let entry = catalog
            .solar_module(&template.entry_id)
            .filter(|entry| entry.revision == template.revision)
            .ok_or(EquipmentConfigurationError::UnknownTemplate)?;
        template.value_provenance = if entry.manufacturer == snapshot.manufacturer
            && entry.model == snapshot.model
            && entry.specification == snapshot.specification
        {
            EquipmentValueProvenance::CatalogCopied
        } else {
            EquipmentValueProvenance::CatalogCustomized
        };
    }
    Ok(snapshot)
}

/// Derives the authoritative total nameplate power for one confirmed string composition.
///
/// # Errors
/// Returns an error for zero/out-of-range counts or an overflowing/out-of-range total.
pub fn confirm_string_composition(
    catalog: &EquipmentCatalog,
    module_count: u32,
    module: SolarModuleSpecificationSnapshot,
) -> Result<PvStringModuleComposition, EquipmentConfigurationError> {
    if module_count == 0 || module_count > MAXIMUM_MODULE_COUNT {
        return Err(EquipmentConfigurationError::InvalidModuleCount);
    }
    let module = confirm_module_snapshot(catalog, module)?;
    let total_peak_power_watts = u64::from(module_count)
        .checked_mul(u64::from(module.specification.peak_power_watts))
        .filter(|total| *total <= MAXIMUM_STRING_POWER_WATTS)
        .ok_or(EquipmentConfigurationError::InvalidStringPower)?;
    Ok(PvStringModuleComposition {
        module_count,
        module,
        total_peak_power_watts,
    })
}

fn validate_identity(manufacturer: &str, model: &str) -> Result<(), EquipmentConfigurationError> {
    if manufacturer.trim().is_empty() || model.trim().is_empty() {
        Err(EquipmentConfigurationError::InvalidIdentity)
    } else {
        Ok(())
    }
}

/// Safe equipment confirmation failures.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum EquipmentConfigurationError {
    #[error("equipment manufacturer and model are required")]
    InvalidIdentity,
    #[error("equipment catalog template is unknown or has a different revision")]
    UnknownTemplate,
    #[error("module count is outside supported bounds")]
    InvalidModuleCount,
    #[error("module peak power is outside supported bounds")]
    InvalidModulePower,
    #[error("derived string peak power is outside supported bounds")]
    InvalidStringPower,
}
