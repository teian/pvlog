//! Offline equipment catalog parsing, indexing, and bounded queries.

use std::collections::BTreeMap;

use pvlog_domain::{
    CatalogEntryId, CatalogRevision, InverterCatalogEntry, SolarModuleCatalogEntry,
};
use serde::Deserialize;
use thiserror::Error;

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

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum EquipmentCatalogError {
    #[error("equipment catalog asset is invalid")]
    InvalidAsset,
    #[error("equipment catalog schema is not supported")]
    UnsupportedSchema,
}
