//! Typed measurement-channel values: what a channel key means and how a
//! submitted value becomes the scaled integer that is stored.
//!
//! A channel stores integers. Its `scale` counts decimal places, so 12.34 on
//! a channel of scale 2 is stored as 1234 and nothing is rounded. A key that
//! follows the shared device key scheme is registered with the definition the
//! scheme gives it; any other key is registered from the first value it carries.

use serde::Serialize;

/// Decimal places an unknown channel keeps when its first value arrives as a
/// number with a unit. Later values with more places would otherwise be
/// refused only because the first one happened to be a whole number.
const DEFAULT_DECIMAL_SCALE: u8 = 3;

/// Largest scale a channel may have.
pub const MAXIMUM_CHANNEL_SCALE: u8 = 9;

/// A decimal number exactly as it was written, without floating point.
///
/// The value is `mantissa / 10^scale`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DecimalValue {
    pub mantissa: i64,
    pub scale: u8,
}

impl DecimalValue {
    /// A whole number.
    #[must_use]
    pub const fn integer(value: i64) -> Self {
        Self {
            mantissa: value,
            scale: 0,
        }
    }

    /// Parses plain decimal notation such as `-12.340`.
    ///
    /// Returns `None` for exponent notation, for more than nine decimal places
    /// and for a number that does not fit a 64-bit integer at its own scale.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let (negative, digits) = match text.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, text),
        };
        let (whole, fraction) = digits.split_once('.').unwrap_or((digits, ""));
        let fraction = fraction.trim_end_matches('0');
        if whole.is_empty()
            || !whole.bytes().all(|byte| byte.is_ascii_digit())
            || !fraction.bytes().all(|byte| byte.is_ascii_digit())
            || fraction.len() > usize::from(MAXIMUM_CHANNEL_SCALE)
        {
            return None;
        }
        let mut mantissa: i64 = 0;
        for byte in whole.bytes().chain(fraction.bytes()) {
            mantissa = mantissa
                .checked_mul(10)?
                .checked_add(i64::from(byte - b'0'))?;
        }
        Some(Self {
            mantissa: if negative { -mantissa } else { mantissa },
            scale: u8::try_from(fraction.len()).ok()?,
        })
    }

    /// Expresses the number as an integer with `scale` decimal places.
    ///
    /// Returns `None` when that would drop a digit or overflow.
    #[must_use]
    pub fn at_scale(self, scale: u8) -> Option<i64> {
        if scale >= self.scale {
            self.mantissa
                .checked_mul(10_i64.checked_pow(u32::from(scale - self.scale))?)
        } else {
            let divisor = 10_i64.checked_pow(u32::from(self.scale - scale))?;
            (self.mantissa % divisor == 0).then_some(self.mantissa / divisor)
        }
    }
}

/// One reading reported under a source-defined channel key.
///
/// `value` with an optional `unit` is the typed form. `power_watts` and
/// `energy_watt_hours` are the earlier form and stay accepted.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ChannelReading {
    pub power_watts: Option<i64>,
    pub energy_watt_hours: Option<i64>,
    pub value: Option<DecimalValue>,
    pub unit: Option<String>,
    pub estimated: bool,
}

/// Storage type of a channel's values.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelValueType {
    Integer,
    Decimal,
    Boolean,
    /// An integer that only grows, such as a lifetime energy counter.
    Counter,
}

impl ChannelValueType {
    /// Name the type is stored under.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Integer => "integer",
            Self::Decimal => "decimal",
            Self::Boolean => "boolean",
            Self::Counter => "counter",
        }
    }

    /// Reads a stored type name.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "integer" => Some(Self::Integer),
            "decimal" => Some(Self::Decimal),
            "boolean" => Some(Self::Boolean),
            "counter" => Some(Self::Counter),
            _ => None,
        }
    }
}

/// How the values of a channel are reduced to one value per period.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelAggregation {
    Mean,
    Sum,
    Min,
    Max,
    Last,
    /// Growth of a counter over the period, ignoring resets.
    CounterDelta,
}

impl ChannelAggregation {
    /// Name the kind is stored under.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Mean => "mean",
            Self::Sum => "sum",
            Self::Min => "min",
            Self::Max => "max",
            Self::Last => "last",
            Self::CounterDelta => "counter_delta",
        }
    }

    /// Reads a stored kind name.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "mean" => Some(Self::Mean),
            "sum" => Some(Self::Sum),
            "min" => Some(Self::Min),
            "max" => Some(Self::Max),
            "last" => Some(Self::Last),
            "counter_delta" => Some(Self::CounterDelta),
            _ => None,
        }
    }
}

/// What the stored integers of a channel mean.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChannelTyping {
    pub value_type: ChannelValueType,
    pub unit: String,
    /// Decimal places of the stored integer.
    pub scale: u8,
    pub aggregation: ChannelAggregation,
}

/// Equipment a key of the device key scheme belongs to.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeviceChannelTarget<'a> {
    /// The inverter with this serial.
    Inverter { serial: &'a str },
    /// The DC input with this tracker number on the inverter with this serial.
    Tracker { serial: &'a str, tracker: u8 },
}

impl<'a> DeviceChannelTarget<'a> {
    /// Serial of the inverter the key names.
    #[must_use]
    pub const fn serial(self) -> &'a str {
        match self {
            Self::Inverter { serial } | Self::Tracker { serial, .. } => serial,
        }
    }
}

/// A key of the device key scheme with the definition the scheme gives it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeviceChannel<'a> {
    pub target: DeviceChannelTarget<'a>,
    /// Whether the key carries the DC power of a tracker.
    pub tracker_power: bool,
    pub typing: ChannelTyping,
    /// The pattern of the scheme the key matched.
    pub definition: &'static DeviceChannelKey,
}

/// Label of one numeric code a status channel reports.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusCodeLabel {
    pub code: i64,
    pub label: &'static str,
}

/// One key pattern of the device key scheme with its definition.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceChannelKey {
    /// Key with `{serial}`, `{phase}` and `{tracker}` standing for the
    /// inverter serial, the AC phase 1 to 3 and the tracker number 1 to 255.
    pub key: &'static str,
    pub value_type: ChannelValueType,
    pub unit: &'static str,
    /// Decimal places of the stored integer.
    pub scale: u8,
    /// Equipment the channel is bound to once it is known: `inverter` or `string`.
    pub target_kind: &'static str,
    pub aggregation: ChannelAggregation,
    /// Meaning of the codes a status channel reports; empty for measurements.
    #[serde(skip_serializing_if = "<[StatusCodeLabel]>::is_empty")]
    pub status_labels: &'static [StatusCodeLabel],
}

const fn device_key(
    key: &'static str,
    value_type: ChannelValueType,
    unit: &'static str,
    target_kind: &'static str,
    aggregation: ChannelAggregation,
) -> DeviceChannelKey {
    DeviceChannelKey {
        key,
        value_type,
        unit,
        scale: 0,
        target_kind,
        aggregation,
        status_labels: &[],
    }
}

/// SMA's code for a status the device did not report.
const STATUS_NOT_AVAILABLE: StatusCodeLabel = StatusCodeLabel {
    code: 16_777_213,
    label: "Information not available",
};

/// SMA codes of the device status.
const DEVICE_STATUS_LABELS: &[StatusCodeLabel] = &[
    StatusCodeLabel {
        code: 35,
        label: "Fault",
    },
    StatusCodeLabel {
        code: 303,
        label: "Off",
    },
    StatusCodeLabel {
        code: 307,
        label: "Ok",
    },
    StatusCodeLabel {
        code: 455,
        label: "Warning",
    },
    STATUS_NOT_AVAILABLE,
];

/// SMA codes of the grid relay state.
const GRID_RELAY_LABELS: &[StatusCodeLabel] = &[
    StatusCodeLabel {
        code: 51,
        label: "Closed",
    },
    StatusCodeLabel {
        code: 311,
        label: "Open",
    },
    STATUS_NOT_AVAILABLE,
];

/// Number of leading [`DEVICE_CHANNEL_KEYS`] that `SBFspot` records too.
const SBFSPOT_KEY_COUNT: usize = 15;

/// Every key of the device key scheme the source profiles share.
///
/// Values are integers in the unit the logger stores, so nothing is converted
/// or rounded. The keys `SBFspot` does not record come last.
pub const DEVICE_CHANNEL_KEYS: &[DeviceChannelKey] = {
    use ChannelAggregation::{CounterDelta, Last, Max, Mean};
    use ChannelValueType::{Counter, Integer};
    &[
        device_key(
            "inv.{serial}.ac.l{phase}.power",
            Integer,
            "W",
            "inverter",
            Mean,
        ),
        device_key(
            "inv.{serial}.ac.l{phase}.current",
            Integer,
            "mA",
            "inverter",
            Mean,
        ),
        device_key(
            "inv.{serial}.ac.l{phase}.voltage",
            Integer,
            "mV",
            "inverter",
            Mean,
        ),
        device_key(
            "inv.{serial}.grid.frequency",
            Integer,
            "mHz",
            "inverter",
            Mean,
        ),
        device_key("inv.{serial}.energy.today", Integer, "Wh", "inverter", Last),
        device_key(
            "inv.{serial}.energy.lifetime",
            Counter,
            "Wh",
            "inverter",
            CounterDelta,
        ),
        device_key(
            "inv.{serial}.operating_time",
            Integer,
            "s",
            "inverter",
            Last,
        ),
        device_key("inv.{serial}.feed_in_time", Integer, "s", "inverter", Last),
        DeviceChannelKey {
            status_labels: DEVICE_STATUS_LABELS,
            ..device_key("inv.{serial}.status", Integer, "code", "inverter", Last)
        },
        DeviceChannelKey {
            status_labels: GRID_RELAY_LABELS,
            ..device_key("inv.{serial}.grid_relay", Integer, "code", "inverter", Last)
        },
        device_key("inv.{serial}.temperature", Integer, "m°C", "inverter", Max),
        device_key("inv.{serial}.signal", Integer, "‰", "inverter", Mean),
        device_key(
            "inv.{serial}.mppt.{tracker}.power",
            Integer,
            "W",
            "string",
            Mean,
        ),
        device_key(
            "inv.{serial}.mppt.{tracker}.current",
            Integer,
            "mA",
            "string",
            Mean,
        ),
        device_key(
            "inv.{serial}.mppt.{tracker}.voltage",
            Integer,
            "mV",
            "string",
            Mean,
        ),
        device_key(
            "inv.{serial}.grid.import_power",
            Integer,
            "W",
            "inverter",
            Mean,
        ),
        device_key(
            "inv.{serial}.grid.export_power",
            Integer,
            "W",
            "inverter",
            Mean,
        ),
        device_key("inv.{serial}.battery.soc", Integer, "‰", "inverter", Last),
        device_key(
            "inv.{serial}.battery.voltage",
            Integer,
            "mV",
            "inverter",
            Mean,
        ),
        device_key(
            "inv.{serial}.battery.current",
            Integer,
            "mA",
            "inverter",
            Mean,
        ),
        device_key(
            "inv.{serial}.battery.temperature",
            Integer,
            "m°C",
            "inverter",
            Mean,
        ),
    ]
};

/// The part of [`DEVICE_CHANNEL_KEYS`] `SBFspot` records: no grid import and
/// export power and no battery.
pub const SBFSPOT_CHANNEL_KEYS: &[DeviceChannelKey] =
    DEVICE_CHANNEL_KEYS.split_at(SBFSPOT_KEY_COUNT).0;

/// Label of a status code reported under a key of the device key scheme.
///
/// Returns `None` for a key that carries no status and for a code the scheme
/// does not know, which is then shown as the number it is.
#[must_use]
pub fn status_code_label(key: &str, code: i64) -> Option<&'static str> {
    device_channel(key)?
        .definition
        .status_labels
        .iter()
        .find(|label| label.code == code)
        .map(|label| label.label)
}

/// Resolves a key of the device key scheme shared by the source profiles.
///
/// Keys are `inv.<serial>.<measurement>`. The serial consists of letters,
/// digits, `-` and `_`; trackers are numbered 1 to 255 and phases 1 to 3.
/// Returns `None` for any other key.
#[must_use]
pub fn device_channel(key: &str) -> Option<DeviceChannel<'_>> {
    let rest = key.strip_prefix("inv.")?;
    let (serial, measurement) = rest.split_once('.')?;
    if serial.is_empty()
        || !serial
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    {
        return None;
    }
    let (target, pattern) = if let Some(rest) = measurement.strip_prefix("mppt.") {
        let (number, quantity) = rest.split_once('.')?;
        let tracker = number.parse::<u8>().ok().filter(|number| *number > 0)?;
        // "01" would be a second key for tracker 1.
        if number != tracker.to_string() {
            return None;
        }
        (
            DeviceChannelTarget::Tracker { serial, tracker },
            format!("inv.{{serial}}.mppt.{{tracker}}.{quantity}"),
        )
    } else {
        let pattern = match measurement
            .strip_prefix("ac.l")
            .map(|rest| rest.split_once('.'))
        {
            Some(Some(("1" | "2" | "3", quantity))) => {
                format!("inv.{{serial}}.ac.l{{phase}}.{quantity}")
            }
            Some(_) => return None,
            None => format!("inv.{{serial}}.{measurement}"),
        };
        (DeviceChannelTarget::Inverter { serial }, pattern)
    };
    let definition = DEVICE_CHANNEL_KEYS
        .iter()
        .find(|definition| definition.key == pattern)?;
    Some(DeviceChannel {
        target,
        tracker_power: definition.key == "inv.{serial}.mppt.{tracker}.power",
        typing: ChannelTyping {
            value_type: definition.value_type,
            unit: definition.unit.to_owned(),
            scale: definition.scale,
            aggregation: definition.aggregation,
        },
        definition,
    })
}

/// The definition a key is registered with when it is first seen.
///
/// A key of the device key scheme takes the scheme's definition. Any other
/// key takes its definition from the reading: the unit the client supplied,
/// or watts and watt-hours for the earlier power and energy members.
#[must_use]
pub fn channel_typing_for(key: &str, reading: &ChannelReading) -> ChannelTyping {
    if let Some(device) = device_channel(key) {
        return device.typing;
    }
    if let Some(value) = reading.value {
        return ChannelTyping {
            value_type: ChannelValueType::Decimal,
            unit: reading.unit.clone().unwrap_or_default(),
            scale: value.scale.max(DEFAULT_DECIMAL_SCALE),
            aggregation: ChannelAggregation::Mean,
        };
    }
    let energy_only = reading.power_watts.is_none() && reading.energy_watt_hours.is_some();
    ChannelTyping {
        value_type: ChannelValueType::Integer,
        unit: if energy_only { "Wh" } else { "W" }.to_owned(),
        scale: 0,
        aggregation: if energy_only {
            ChannelAggregation::Sum
        } else {
            ChannelAggregation::Mean
        },
    }
}

/// Why a submitted value cannot be stored in its channel.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelValueError {
    /// The reading carries no value for the channel.
    ValueMissing,
    /// The unit differs from the channel's unit.
    UnitMismatch,
    /// The value has more decimal places than the channel keeps, or is not a
    /// whole number on an integer channel.
    TypeMismatch,
    /// The value does not fit the stored integer.
    OutOfRange,
}

impl ChannelValueError {
    /// Stable code reported to the client.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::ValueMissing => "value_missing",
            Self::UnitMismatch => "unit_mismatch",
            Self::TypeMismatch => "type_mismatch",
            Self::OutOfRange => "value_out_of_range",
        }
    }
}

/// A channel value of a reading that was not stored, while the rest was.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RejectedChannel {
    pub channel_key: String,
    /// One of `value_missing`, `unit_mismatch`, `type_mismatch`, `value_out_of_range`.
    pub reason: &'static str,
}

/// Converts a submitted value into the scaled integer a channel stores.
///
/// # Errors
///
/// Returns why the value contradicts the channel's definition.
pub fn scaled_channel_value(
    typing: &ChannelTyping,
    reading: &ChannelReading,
) -> Result<i64, ChannelValueError> {
    let value = if let Some(value) = reading.value {
        if reading
            .unit
            .as_ref()
            .is_some_and(|unit| *unit != typing.unit)
        {
            return Err(ChannelValueError::UnitMismatch);
        }
        value
    } else {
        // The earlier members imply their unit. A channel in watts keeps
        // accepting the energy member when no power is sent, as it always did.
        let integer = match typing.unit.as_str() {
            "Wh" => reading.energy_watt_hours.or(reading.power_watts),
            "W" => reading.power_watts.or(reading.energy_watt_hours),
            _ if reading.power_watts.is_some() || reading.energy_watt_hours.is_some() => {
                return Err(ChannelValueError::UnitMismatch);
            }
            _ => None,
        };
        DecimalValue::integer(integer.ok_or(ChannelValueError::ValueMissing)?)
    };
    if typing.value_type == ChannelValueType::Boolean && !matches!(value.at_scale(0), Some(0 | 1)) {
        return Err(ChannelValueError::TypeMismatch);
    }
    value.at_scale(typing.scale).ok_or({
        if value.scale > typing.scale {
            ChannelValueError::TypeMismatch
        } else {
            ChannelValueError::OutOfRange
        }
    })
}
