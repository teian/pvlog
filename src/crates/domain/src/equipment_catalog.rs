//! Typed, unit-explicit equipment catalog and configured snapshot models.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct CatalogEntryId(pub String);

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct CatalogRevision(pub String);

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogProvenance {
    pub source_name: String,
    pub source_reference: String,
    pub retrieved_on: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EquipmentValueProvenance {
    Manual,
    CatalogCopied,
    CatalogCustomized,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EquipmentTemplateReference {
    pub entry_id: CatalogEntryId,
    pub revision: CatalogRevision,
    pub value_provenance: EquipmentValueProvenance,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TemperatureRange {
    pub minimum_milli_celsius: i32,
    pub maximum_milli_celsius: i32,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DimensionsMillimetres {
    pub length: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InverterTopology {
    Transformerless,
    HighFrequencyTransformer,
    LineFrequencyTransformer,
    Microinverter,
    Hybrid,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CoolingMethod {
    NaturalConvection,
    ForcedAir,
    Liquid,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MpptInputSpecification {
    pub tracker_index: u16,
    pub string_input_count: u16,
    pub maximum_operating_current_milliamperes: Option<u32>,
    pub maximum_short_circuit_current_milliamperes: Option<u32>,
    pub minimum_voltage_millivolts: Option<u32>,
    pub maximum_voltage_millivolts: Option<u32>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InverterDcSpecification {
    pub topology: Option<InverterTopology>,
    pub total_string_input_count: u16,
    pub maximum_input_voltage_millivolts: Option<u32>,
    pub start_voltage_millivolts: Option<u32>,
    pub nominal_input_voltage_millivolts: Option<u32>,
    pub minimum_mppt_voltage_millivolts: Option<u32>,
    pub maximum_mppt_voltage_millivolts: Option<u32>,
    pub maximum_input_current_milliamperes: Option<u32>,
    pub maximum_short_circuit_current_milliamperes: Option<u32>,
    pub mppt_inputs: Vec<MpptInputSpecification>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InverterAcSpecification {
    pub phase_count: u8,
    pub nominal_grid_voltage_millivolts: Option<u32>,
    pub minimum_grid_voltage_millivolts: Option<u32>,
    pub maximum_grid_voltage_millivolts: Option<u32>,
    pub minimum_grid_frequency_millihertz: Option<u32>,
    pub maximum_grid_frequency_millihertz: Option<u32>,
    pub rated_active_power_watts: u32,
    pub maximum_active_power_watts: Option<u32>,
    pub maximum_apparent_power_volt_amperes: Option<u32>,
    pub maximum_output_current_milliamperes: Option<u32>,
    pub minimum_power_factor_basis_points: Option<i16>,
    pub maximum_power_factor_basis_points: Option<i16>,
    pub total_harmonic_distortion_basis_points: Option<u16>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InverterOperationalSpecification {
    pub maximum_efficiency_basis_points: Option<u16>,
    pub european_efficiency_basis_points: Option<u16>,
    pub standby_consumption_watts: Option<u32>,
    pub operating_temperature: Option<TemperatureRange>,
    pub derating_start_milli_celsius: Option<i32>,
    pub cooling_method: Option<CoolingMethod>,
    pub acoustic_noise_millidecibels: Option<u32>,
    pub ingress_protection_rating: Option<String>,
    pub minimum_humidity_basis_points: Option<u16>,
    pub maximum_humidity_basis_points: Option<u16>,
    pub maximum_altitude_metres: Option<u32>,
    pub communication_interfaces: Vec<String>,
    pub dimensions_millimetres: Option<DimensionsMillimetres>,
    pub weight_grams: Option<u32>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InverterCatalogEntry {
    pub id: CatalogEntryId,
    pub revision: CatalogRevision,
    pub manufacturer: String,
    pub model: String,
    pub dc: InverterDcSpecification,
    pub ac: InverterAcSpecification,
    pub operational: InverterOperationalSpecification,
    pub provenance: CatalogProvenance,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SolarCellTechnology {
    Monocrystalline,
    Polycrystalline,
    NTypeMonocrystalline,
    PTypeMonocrystalline,
    Heterojunction,
    ThinFilm,
    Other,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SolarModuleSpecification {
    pub cell_technology: SolarCellTechnology,
    pub cell_description: Option<String>,
    pub bifacial: bool,
    pub bifaciality_factor_basis_points: Option<u16>,
    pub bifaciality_tolerance_basis_points: Option<u16>,
    pub peak_power_watts: u32,
    pub open_circuit_voltage_millivolts: u32,
    pub maximum_power_voltage_millivolts: u32,
    pub short_circuit_current_milliamperes: u32,
    pub maximum_power_current_milliamperes: u32,
    pub efficiency_basis_points: u16,
    pub short_circuit_current_temperature_coefficient_ppm_per_celsius: i32,
    pub open_circuit_voltage_temperature_coefficient_ppm_per_celsius: i32,
    pub peak_power_temperature_coefficient_ppm_per_celsius: i32,
    pub maximum_system_voltage_millivolts: Option<u32>,
    pub operating_temperature: Option<TemperatureRange>,
    pub maximum_series_fuse_milliamperes: Option<u32>,
    pub maximum_front_static_load_pascals: Option<u32>,
    pub maximum_rear_static_load_pascals: Option<u32>,
    pub dimensions_millimetres: Option<DimensionsMillimetres>,
    pub weight_grams: Option<u32>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SolarModuleCatalogEntry {
    pub id: CatalogEntryId,
    pub revision: CatalogRevision,
    pub manufacturer: String,
    pub model: String,
    pub specification: SolarModuleSpecification,
    pub provenance: CatalogProvenance,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InverterSpecificationSnapshot {
    pub manufacturer: String,
    pub model: String,
    pub dc: InverterDcSpecification,
    pub ac: InverterAcSpecification,
    pub operational: InverterOperationalSpecification,
    pub template: Option<EquipmentTemplateReference>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SolarModuleSpecificationSnapshot {
    pub manufacturer: String,
    pub model: String,
    pub specification: SolarModuleSpecification,
    pub template: Option<EquipmentTemplateReference>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PvStringModuleComposition {
    pub module_count: u32,
    pub module: SolarModuleSpecificationSnapshot,
    pub total_peak_power_watts: u64,
}
