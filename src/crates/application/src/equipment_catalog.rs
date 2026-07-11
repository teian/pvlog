//! Offline equipment catalog parsing, indexing, and bounded queries.

use std::collections::{BTreeMap, BTreeSet};

use pvlog_domain::{
    CatalogEntryId, CatalogRevision, InverterCatalogEntry, SolarModuleCatalogEntry,
};
use serde::Deserialize;
use thiserror::Error;
use url::Url;

const BUNDLED_CATALOG: &str = include_str!("../../../../assets/equipment-catalog/catalog-v1.json");
const MAXIMUM_PAGE_SIZE: usize = 100;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CatalogDocument {
    schema_version: u16,
    revision: CatalogRevision,
    inverters: Vec<InverterCatalogEntry>,
    solar_modules: Vec<SolarModuleCatalogEntry>,
}

/// Bounded catalog list query shared by inverter and module searches.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct EquipmentCatalogQuery {
    pub search: Option<String>,
    pub manufacturer: Option<String>,
    pub offset: usize,
    pub limit: usize,
}

/// Deterministically ordered page plus catalog revision metadata.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EquipmentCatalogPage<T> {
    pub revision: CatalogRevision,
    pub total: usize,
    pub offset: usize,
    pub limit: usize,
    pub items: Vec<T>,
}

/// Immutable parsed catalog used identically by all persistence profiles.
#[derive(Clone, Debug)]
pub struct EquipmentCatalog {
    revision: CatalogRevision,
    inverters: BTreeMap<CatalogEntryId, InverterCatalogEntry>,
    solar_modules: BTreeMap<CatalogEntryId, SolarModuleCatalogEntry>,
}

impl EquipmentCatalog {
    /// Parses the catalog compiled into this release without network access.
    ///
    /// # Errors
    /// Returns a safe error when the embedded asset cannot be decoded.
    pub fn bundled() -> Result<Self, EquipmentCatalogError> {
        Self::parse(BUNDLED_CATALOG)
    }

    /// Parses catalog JSON for tooling and controlled tests.
    ///
    /// # Errors
    /// Returns a safe error for malformed or unsupported documents.
    pub fn parse(document: &str) -> Result<Self, EquipmentCatalogError> {
        let document: CatalogDocument =
            serde_json::from_str(document).map_err(|_| EquipmentCatalogError::InvalidAsset)?;
        if document.schema_version != 1 {
            return Err(EquipmentCatalogError::UnsupportedSchema);
        }
        validate_document(&document)?;
        Ok(Self {
            revision: document.revision,
            inverters: document
                .inverters
                .into_iter()
                .map(|entry| (entry.id.clone(), entry))
                .collect(),
            solar_modules: document
                .solar_modules
                .into_iter()
                .map(|entry| (entry.id.clone(), entry))
                .collect(),
        })
    }

    #[must_use]
    pub fn revision(&self) -> &CatalogRevision {
        &self.revision
    }

    #[must_use]
    pub fn inverter(&self, id: &CatalogEntryId) -> Option<&InverterCatalogEntry> {
        self.inverters.get(id)
    }

    #[must_use]
    pub fn solar_module(&self, id: &CatalogEntryId) -> Option<&SolarModuleCatalogEntry> {
        self.solar_modules.get(id)
    }

    #[must_use]
    pub fn inverters(
        &self,
        query: &EquipmentCatalogQuery,
    ) -> EquipmentCatalogPage<InverterCatalogEntry> {
        page(&self.revision, self.inverters.values(), query, |entry| {
            (&entry.id, &entry.manufacturer, &entry.model)
        })
    }

    #[must_use]
    pub fn solar_modules(
        &self,
        query: &EquipmentCatalogQuery,
    ) -> EquipmentCatalogPage<SolarModuleCatalogEntry> {
        page(
            &self.revision,
            self.solar_modules.values(),
            query,
            |entry| (&entry.id, &entry.manufacturer, &entry.model),
        )
    }
}

fn page<'a, T: Clone + 'a>(
    revision: &CatalogRevision,
    values: impl Iterator<Item = &'a T>,
    query: &EquipmentCatalogQuery,
    identity: impl Fn(&T) -> (&CatalogEntryId, &str, &str),
) -> EquipmentCatalogPage<T> {
    let search = query.search.as_deref().map(normalize);
    let manufacturer = query.manufacturer.as_deref().map(normalize);
    let matches = values
        .filter(|entry| {
            let (id, entry_manufacturer, model) = identity(entry);
            manufacturer
                .as_ref()
                .is_none_or(|expected| normalize(entry_manufacturer) == *expected)
                && search.as_ref().is_none_or(|needle| {
                    normalize(&format!("{} {entry_manufacturer} {model}", id.0)).contains(needle)
                })
        })
        .cloned()
        .collect::<Vec<_>>();
    let total = matches.len();
    let limit = query.limit.clamp(1, MAXIMUM_PAGE_SIZE);
    EquipmentCatalogPage {
        revision: revision.clone(),
        total,
        offset: query.offset,
        limit,
        items: matches.into_iter().skip(query.offset).take(limit).collect(),
    }
}

fn normalize(value: &str) -> String {
    value.trim().to_lowercase()
}

fn validate_document(document: &CatalogDocument) -> Result<(), EquipmentCatalogError> {
    if document.revision.0.trim().is_empty() {
        return invalid("catalog revision is empty");
    }
    validate_order_and_ids(document.inverters.iter().map(|entry| &entry.id), "inverter")?;
    validate_order_and_ids(
        document.solar_modules.iter().map(|entry| &entry.id),
        "solar module",
    )?;
    for entry in &document.inverters {
        if entry.revision != document.revision {
            return invalid(format!("{} has a mismatching revision", entry.id.0));
        }
        validate_identity(
            &entry.id,
            &entry.manufacturer,
            &entry.model,
            &entry.provenance,
        )?;
        validate_inverter(entry)?;
    }
    for entry in &document.solar_modules {
        if entry.revision != document.revision {
            return invalid(format!("{} has a mismatching revision", entry.id.0));
        }
        validate_identity(
            &entry.id,
            &entry.manufacturer,
            &entry.model,
            &entry.provenance,
        )?;
        validate_module(entry)?;
    }
    Ok(())
}

fn validate_order_and_ids<'a>(
    values: impl Iterator<Item = &'a CatalogEntryId>,
    kind: &str,
) -> Result<(), EquipmentCatalogError> {
    let mut previous: Option<&str> = None;
    let mut seen = BTreeSet::new();
    for id in values {
        if id.0.is_empty()
            || !id.0.chars().all(|character| {
                character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
            })
            || id.0.starts_with('-')
            || id.0.ends_with('-')
            || id.0.contains("--")
        {
            return invalid(format!("{} has an invalid {kind} ID", id.0));
        }
        if !seen.insert(id.0.as_str()) {
            return invalid(format!("duplicate {kind} ID {}", id.0));
        }
        if previous.is_some_and(|value| value >= id.0.as_str()) {
            return invalid(format!("{kind} entries are not deterministically ordered"));
        }
        previous = Some(&id.0);
    }
    Ok(())
}

fn validate_identity(
    id: &CatalogEntryId,
    manufacturer: &str,
    model: &str,
    provenance: &pvlog_domain::CatalogProvenance,
) -> Result<(), EquipmentCatalogError> {
    if manufacturer.trim().is_empty() || model.trim().is_empty() {
        return invalid(format!("{} has incomplete identity", id.0));
    }
    if provenance.source_name.trim().is_empty() || Url::parse(&provenance.source_reference).is_err()
    {
        return invalid(format!("{} has invalid provenance", id.0));
    }
    Ok(())
}

fn validate_inverter(entry: &InverterCatalogEntry) -> Result<(), EquipmentCatalogError> {
    let dc = &entry.dc;
    if dc.mppt_inputs.is_empty()
        || dc.total_string_input_count
            != dc
                .mppt_inputs
                .iter()
                .map(|tracker| tracker.string_input_count)
                .sum::<u16>()
    {
        return invalid(format!("{} has inconsistent string topology", entry.id.0));
    }
    for (index, tracker) in dc.mppt_inputs.iter().enumerate() {
        if usize::from(tracker.tracker_index) != index + 1 || tracker.string_input_count == 0 {
            return invalid(format!("{} has invalid MPPT ordering", entry.id.0));
        }
        validate_optional_range(
            tracker.minimum_voltage_millivolts,
            tracker.maximum_voltage_millivolts,
            &entry.id,
            "tracker voltage",
        )?;
        if matches!(
            (tracker.maximum_operating_current_milliamperes, tracker.maximum_short_circuit_current_milliamperes),
            (Some(operating), Some(short_circuit)) if operating > short_circuit
        ) {
            return invalid(format!("{} has invalid MPPT currents", entry.id.0));
        }
    }
    validate_optional_range(
        dc.minimum_mppt_voltage_millivolts,
        dc.maximum_mppt_voltage_millivolts,
        &entry.id,
        "MPPT voltage",
    )?;
    if let Some(maximum) = dc.maximum_input_voltage_millivolts
        && [
            dc.start_voltage_millivolts,
            dc.nominal_input_voltage_millivolts,
            dc.maximum_mppt_voltage_millivolts,
        ]
        .into_iter()
        .flatten()
        .any(|value| value > maximum)
    {
        return invalid(format!("{} exceeds maximum DC voltage", entry.id.0));
    }
    if entry.ac.phase_count == 0
        || entry.ac.phase_count > 3
        || entry.ac.rated_active_power_watts == 0
    {
        return invalid(format!("{} has invalid AC ratings", entry.id.0));
    }
    for efficiency in [
        entry.operational.maximum_efficiency_basis_points,
        entry.operational.european_efficiency_basis_points,
    ]
    .into_iter()
    .flatten()
    {
        if efficiency == 0 || efficiency > 10_000 {
            return invalid(format!("{} has invalid efficiency", entry.id.0));
        }
    }
    Ok(())
}

fn validate_module(entry: &SolarModuleCatalogEntry) -> Result<(), EquipmentCatalogError> {
    let specification = &entry.specification;
    if specification.peak_power_watts == 0
        || specification.maximum_power_voltage_millivolts
            >= specification.open_circuit_voltage_millivolts
        || specification.maximum_power_current_milliamperes
            >= specification.short_circuit_current_milliamperes
        || specification.efficiency_basis_points == 0
        || specification.efficiency_basis_points > 10_000
        || specification.operating_temperature.minimum_milli_celsius
            >= specification.operating_temperature.maximum_milli_celsius
        || specification.short_circuit_current_temperature_coefficient_ppm_per_celsius < 0
        || specification.open_circuit_voltage_temperature_coefficient_ppm_per_celsius > 0
        || specification.peak_power_temperature_coefficient_ppm_per_celsius > 0
    {
        return invalid(format!("{} has invalid electrical ratings", entry.id.0));
    }
    let calculated_power = u64::from(specification.maximum_power_voltage_millivolts)
        * u64::from(specification.maximum_power_current_milliamperes)
        / 1_000_000;
    if calculated_power.abs_diff(u64::from(specification.peak_power_watts))
        > u64::from(specification.peak_power_watts) / 20
    {
        return invalid(format!("{} has inconsistent peak power", entry.id.0));
    }
    if specification.bifacial != specification.bifaciality_factor_basis_points.is_some()
        || specification.dimensions_millimetres.length == 0
        || specification.dimensions_millimetres.width == 0
        || specification.dimensions_millimetres.height == 0
        || specification.weight_grams == 0
    {
        return invalid(format!("{} has invalid physical ratings", entry.id.0));
    }
    Ok(())
}

fn validate_optional_range(
    minimum: Option<u32>,
    maximum: Option<u32>,
    id: &CatalogEntryId,
    name: &str,
) -> Result<(), EquipmentCatalogError> {
    if matches!((minimum, maximum), (Some(minimum), Some(maximum)) if minimum >= maximum) {
        invalid(format!("{} has invalid {name} range", id.0))
    } else {
        Ok(())
    }
}

fn invalid<T>(message: impl Into<String>) -> Result<T, EquipmentCatalogError> {
    Err(EquipmentCatalogError::InvalidEntry(message.into()))
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum EquipmentCatalogError {
    #[error("equipment catalog asset is invalid")]
    InvalidAsset,
    #[error("equipment catalog schema is not supported")]
    UnsupportedSchema,
    #[error("equipment catalog entry is invalid: {0}")]
    InvalidEntry(String),
}
