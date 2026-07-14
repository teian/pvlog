use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use time::Date;

use crate::{
    AccountId, ChannelId, CurrencyCode, EffectiveInverterCapacity, EffectiveStringCapacity,
    EffectiveSystemCapacity, EquipmentId, ForecastCompleteness, ForecastCompletenessReason,
    ForecastInputSnapshot, ForecastSettings, IanaTimezone, InverterId, Money,
    SolarModuleSpecificationSnapshot, StringId, SystemId, TariffId, ValidationError, Visibility,
    Watts,
};

/// Half-open effective date range where an omitted end means indefinitely active.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct EffectivePeriod {
    pub valid_from: Date,
    pub valid_until: Option<Date>,
}

impl EffectivePeriod {
    /// Creates a non-empty effective range.
    ///
    /// # Errors
    ///
    /// Returns an error when the end is not later than the start.
    pub fn new(valid_from: Date, valid_until: Option<Date>) -> Result<Self, ValidationError> {
        if valid_until.is_some_and(|end| end <= valid_from) {
            Err(ValidationError::new(
                "invalid_effective_period",
                "valid_until",
                "effective period end must be later than its start",
            ))
        } else {
            Ok(Self {
                valid_from,
                valid_until,
            })
        }
    }

    /// Reports whether the date falls within the half-open range.
    #[must_use]
    pub fn contains(self, date: Date) -> bool {
        date >= self.valid_from && self.valid_until.is_none_or(|end| date < end)
    }
}

/// Photovoltaic system aggregate configuration.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PvSystem {
    pub id: SystemId,
    pub account_id: AccountId,
    pub name: String,
    pub description: Option<String>,
    pub timezone: IanaTimezone,
    pub commissioning_date: Date,
    pub country_code: Option<String>,
    pub latitude_microdegrees: Option<i32>,
    pub longitude_microdegrees: Option<i32>,
    pub status_interval_seconds: u32,
    pub lifecycle: SystemLifecycle,
    pub privacy: SystemPrivacy,
    pub calculation: CalculationSettings,
    pub inverters: Vec<Inverter>,
}

impl PvSystem {
    /// Validates ownership throughout the system → inverter → string aggregate.
    ///
    /// # Errors
    /// Returns a validation error when a child belongs to another aggregate or an inverter has no
    /// strings.
    pub fn validate_hierarchy(&self) -> Result<(), ValidationError> {
        for inverter in &self.inverters {
            if inverter.system_id != self.id {
                return Err(ValidationError::new(
                    "invalid_inverter_parent",
                    "inverters.system_id",
                    "inverter must belong to the containing system",
                ));
            }
            if inverter.strings.is_empty() {
                return Err(ValidationError::new(
                    "inverter_without_strings",
                    "inverters.strings",
                    "inverter must contain at least one PV string",
                ));
            }
            if inverter
                .strings
                .iter()
                .any(|string| string.inverter_id != inverter.id)
            {
                return Err(ValidationError::new(
                    "invalid_string_parent",
                    "inverters.strings.inverter_id",
                    "PV string must belong to the containing inverter",
                ));
            }
        }
        Ok(())
    }

    /// Selects the equipment configuration effective on `date` and aggregates DC capacity.
    ///
    /// # Errors
    ///
    /// Returns an error for overlapping active versions, non-positive nameplate capacity, or
    /// arithmetic overflow.
    pub fn effective_capacity_snapshot(
        &self,
        date: Date,
    ) -> Result<EffectiveSystemCapacity, ValidationError> {
        let mut seen_inverters = BTreeSet::new();
        let mut system_capacity = 0_i64;
        let mut ready_capacity = 0_i64;
        let mut missing_reasons = BTreeSet::new();
        let mut inverters = Vec::new();

        for inverter in self
            .inverters
            .iter()
            .filter(|inverter| inverter.period.contains(date))
        {
            if !seen_inverters.insert(inverter.id) {
                return Err(ValidationError::new(
                    "overlapping_inverter_configuration",
                    "inverters.period",
                    "multiple versions of one inverter are effective on the same date",
                ));
            }

            let mut seen_strings = BTreeSet::new();
            let mut inverter_capacity = 0_i64;
            let mut inverter_ready_capacity = 0_i64;
            let mut strings = Vec::new();
            for string in inverter
                .strings
                .iter()
                .filter(|string| string.period.contains(date))
            {
                if !seen_strings.insert(string.id) {
                    return Err(ValidationError::new(
                        "overlapping_string_configuration",
                        "inverters.strings.period",
                        "multiple versions of one PV string are effective on the same date",
                    ));
                }
                let capacity = string.rated_power.value();
                if capacity <= 0 {
                    return Err(ValidationError::new(
                        "invalid_string_capacity",
                        "inverters.strings.rated_power",
                        "effective PV string peak power must be positive",
                    ));
                }
                inverter_capacity = checked_capacity_add(inverter_capacity, capacity)?;

                let reasons = self.forecast_incomplete_reasons(string, date);
                let forecast_ready = reasons.is_empty();
                if forecast_ready {
                    inverter_ready_capacity =
                        checked_capacity_add(inverter_ready_capacity, capacity)?;
                } else {
                    missing_reasons.extend(reasons.iter().copied());
                }
                strings.push(EffectiveStringCapacity {
                    string_id: string.id,
                    total_peak_power: string.rated_power,
                    forecast_ready,
                    incomplete_reasons: reasons,
                });
            }

            system_capacity = checked_capacity_add(system_capacity, inverter_capacity)?;
            ready_capacity = checked_capacity_add(ready_capacity, inverter_ready_capacity)?;
            inverters.push(EffectiveInverterCapacity {
                inverter_id: inverter.id,
                total_peak_power: Watts::new(inverter_capacity),
                forecast_ready_peak_power: Watts::new(inverter_ready_capacity),
                strings,
            });
        }

        if inverters.is_empty() || system_capacity == 0 {
            missing_reasons.insert(ForecastCompletenessReason::NoEffectiveEquipment);
        }
        let completeness = if missing_reasons.is_empty() {
            ForecastCompleteness::Complete
        } else if ready_capacity > 0 {
            ForecastCompleteness::Partial {
                reasons: missing_reasons.into_iter().collect(),
            }
        } else {
            ForecastCompleteness::Unavailable {
                reasons: missing_reasons.into_iter().collect(),
            }
        };

        Ok(EffectiveSystemCapacity {
            system_id: self.id,
            effective_at: date,
            next_configuration_boundary: self
                .effective_configuration_boundaries()
                .into_iter()
                .find(|boundary| *boundary > date),
            total_peak_power: Watts::new(system_capacity),
            forecast_ready_peak_power: Watts::new(ready_capacity),
            inverters,
            completeness,
        })
    }

    /// Returns sorted unique equipment and forecast-setting effective-date boundaries.
    #[must_use]
    pub fn effective_configuration_boundaries(&self) -> Vec<Date> {
        let mut boundaries = BTreeSet::new();
        for inverter in &self.inverters {
            boundaries.insert(inverter.period.valid_from);
            boundaries.extend(inverter.period.valid_until);
            for string in &inverter.strings {
                boundaries.insert(string.period.valid_from);
                boundaries.extend(string.period.valid_until);
                if let Some(settings) = &string.forecast_settings {
                    boundaries.insert(settings.period.valid_from);
                    boundaries.extend(settings.period.valid_until);
                }
            }
        }
        boundaries.into_iter().collect()
    }

    fn forecast_incomplete_reasons(
        &self,
        string: &PvString,
        date: Date,
    ) -> Vec<ForecastCompletenessReason> {
        let mut reasons = BTreeSet::new();
        if self.latitude_microdegrees.is_none() || self.longitude_microdegrees.is_none() {
            reasons.insert(ForecastCompletenessReason::MissingSystemLocation);
        }
        if string.panel_manufacturer.is_none() || string.panel_model.is_none() {
            reasons.insert(ForecastCompletenessReason::MissingModuleIdentity);
        }
        if string.panel_count == 0 || string.module_peak_power.is_none() {
            reasons.insert(ForecastCompletenessReason::MissingModuleCapacity);
        }
        if string.module_specification_snapshot.is_none() {
            reasons.insert(ForecastCompletenessReason::MissingModuleSpecification);
        }
        if string.orientation_degrees.is_none() {
            reasons.insert(ForecastCompletenessReason::MissingOrientation);
        }
        if string.tilt_degrees.is_none() {
            reasons.insert(ForecastCompletenessReason::MissingTilt);
        }
        if string
            .forecast_settings
            .as_ref()
            .is_none_or(|settings| !settings.period.contains(date))
        {
            reasons.insert(ForecastCompletenessReason::MissingForecastSettings);
        }
        reasons.into_iter().collect()
    }
}

fn checked_capacity_add(current: i64, value: i64) -> Result<i64, ValidationError> {
    current.checked_add(value).ok_or_else(|| {
        ValidationError::new(
            "capacity_overflow",
            "inverters.strings.rated_power",
            "effective PV capacity exceeds the supported range",
        )
    })
}

/// Effective-dated inverter contained by one photovoltaic system aggregate.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Inverter {
    pub id: InverterId,
    pub system_id: SystemId,
    pub name: String,
    pub manufacturer: Option<String>,
    pub model: Option<String>,
    pub serial_reference: Option<String>,
    pub rated_power: Option<Watts>,
    pub period: EffectivePeriod,
    pub strings: Vec<PvString>,
}

/// Effective-dated photovoltaic string contained by one inverter.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PvString {
    pub id: StringId,
    pub inverter_id: InverterId,
    pub name: String,
    pub panel_count: u32,
    pub panel_manufacturer: Option<String>,
    pub panel_model: Option<String>,
    pub module_peak_power: Option<Watts>,
    pub rated_power: Watts,
    pub module_specification_snapshot: Option<SolarModuleSpecificationSnapshot>,
    pub orientation_degrees: Option<u16>,
    pub tilt_degrees: Option<u8>,
    pub period: EffectivePeriod,
    pub forecast_settings: Option<ForecastSettings>,
}

impl PvString {
    /// Captures the configured forecast inputs when explicit effective settings exist.
    #[must_use]
    pub fn forecast_input_snapshot(&self) -> Option<ForecastInputSnapshot> {
        self.forecast_settings
            .clone()
            .map(|settings| ForecastInputSnapshot::from_pv_string(self, settings))
    }
}

/// System lifecycle independent from public visibility.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SystemLifecycle {
    Active,
    Archived,
    PendingDeletion,
}

/// Privacy and discovery policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct SystemPrivacy {
    pub visibility: Visibility,
    pub discoverable: bool,
    pub location_precision: GeographicPrecision,
}

impl Default for SystemPrivacy {
    fn default() -> Self {
        Self {
            visibility: Visibility::Private,
            discoverable: false,
            location_precision: GeographicPrecision::Hidden,
        }
    }
}

/// Maximum location detail exposed outside the owning account.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GeographicPrecision {
    #[default]
    Hidden,
    Country,
    Region,
    Approximate,
    Exact,
}

/// Effective nameplate capacity.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct CapacityPeriod {
    pub period: EffectivePeriod,
    pub capacity: Watts,
}

/// Effective-dated equipment attached to a system.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Equipment {
    pub id: EquipmentId,
    pub system_id: SystemId,
    pub kind: EquipmentKind,
    pub manufacturer: Option<String>,
    pub model: Option<String>,
    pub serial_reference: Option<String>,
    pub rated_power: Option<Watts>,
    pub period: EffectivePeriod,
}

/// Portable equipment classification.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EquipmentKind {
    Battery,
    Meter,
    Sensor,
    Other,
}

/// Effective-dated import or export electricity price.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Tariff {
    pub id: TariffId,
    pub system_id: SystemId,
    pub name: String,
    pub direction: TariffDirection,
    pub price_per_kilowatt_hour: Money,
    pub standing_charge_per_day: Option<Money>,
    pub period: EffectivePeriod,
}

/// Energy-flow direction to which a tariff applies.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TariffDirection {
    Import,
    Export,
}

/// Explicit deterministic calculation policy for missing and net values.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CalculationSettings {
    pub power: PowerCalculationMode,
    pub net: NetCalculationMode,
    pub derive_interval_energy: bool,
    pub currency: Option<CurrencyCode>,
}

/// Whether power may be derived from energy deltas.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PowerCalculationMode {
    MeasuredOnly,
    DeriveFromEnergy,
}

/// Sign and source semantics for net grid power.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NetCalculationMode {
    Measured,
    ImportPositive,
    ExportPositive,
}

/// Administrator-defined typed extended measurement channel.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ChannelDefinition {
    pub id: ChannelId,
    pub system_id: SystemId,
    pub stable_key: String,
    pub name: String,
    pub data_type: ChannelDataType,
    pub unit: String,
    pub scale: ChannelScale,
    pub minimum_scaled: Option<i64>,
    pub maximum_scaled: Option<i64>,
    pub display: ChannelDisplay,
    pub lifecycle: ChannelLifecycle,
}

/// Storage value type for an extended channel.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelDataType {
    Integer,
    Decimal,
    Boolean,
}

/// Base-ten exponent applied to a stored extended integer.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct ChannelScale(i8);

impl ChannelScale {
    /// Creates a bounded base-ten exponent.
    ///
    /// # Errors
    ///
    /// Returns an error outside -9 through +9.
    pub fn new(exponent: i8) -> Result<Self, ValidationError> {
        if (-9..=9).contains(&exponent) {
            Ok(Self(exponent))
        } else {
            Err(ValidationError::new(
                "channel_scale_out_of_range",
                "scale",
                "channel scale must be between -9 and 9",
            ))
        }
    }

    #[must_use]
    pub const fn exponent(self) -> i8 {
        self.0
    }
}

/// Presentation hints that do not change channel semantics.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ChannelDisplay {
    pub color_token: Option<String>,
    pub chart_by_default: bool,
    pub decimal_places: u8,
}

/// Extended channel lifecycle.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelLifecycle {
    Active,
    Retired { retired_on: Date },
}
