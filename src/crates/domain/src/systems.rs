use serde::{Deserialize, Serialize};
use time::Date;

use crate::{
    AccountId, ChannelId, CurrencyCode, EquipmentId, IanaTimezone, InverterId, Money, StringId,
    SystemId, TariffId, ValidationError, Visibility, Watts,
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
    pub rated_power: Watts,
    pub orientation_degrees: Option<u16>,
    pub tilt_degrees: Option<u8>,
    pub period: EffectivePeriod,
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
