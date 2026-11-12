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
}

/// Resolves a key of the device key scheme shared by the source profiles.
///
/// Keys are `inv.<serial>.<measurement>`. The serial consists of letters,
/// digits, `-` and `_`; trackers are numbered 1 to 255 and phases 1 to 3.
/// Returns `None` for any other key.
#[must_use]
pub fn device_channel(key: &str) -> Option<DeviceChannel<'_>> {
    use ChannelAggregation::{CounterDelta, Last, Max, Mean};
    let rest = key.strip_prefix("inv.")?;
    let (serial, measurement) = rest.split_once('.')?;
    if serial.is_empty()
        || !serial
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    {
        return None;
    }
    let electrical = |quantity: &str| match quantity {
        "power" => Some(("W", Mean)),
        "current" => Some(("mA", Mean)),
        "voltage" => Some(("mV", Mean)),
        _ => None,
    };
    let inverter = DeviceChannelTarget::Inverter { serial };
    let mut tracker_power = false;
    let (target, value_type, (unit, aggregation)) = if let Some(rest) =
        measurement.strip_prefix("mppt.")
    {
        let (number, quantity) = rest.split_once('.')?;
        let tracker = number.parse::<u8>().ok().filter(|number| *number > 0)?;
        // "01" would be a second key for tracker 1.
        if number != tracker.to_string() {
            return None;
        }
        tracker_power = quantity == "power";
        (
            DeviceChannelTarget::Tracker { serial, tracker },
            ChannelValueType::Integer,
            electrical(quantity)?,
        )
    } else if let Some(rest) = measurement.strip_prefix("ac.l") {
        let (phase, quantity) = rest.split_once('.')?;
        if !matches!(phase, "1" | "2" | "3") {
            return None;
        }
        (inverter, ChannelValueType::Integer, electrical(quantity)?)
    } else {
        let (value_type, definition) = match measurement {
            "grid.frequency" => (ChannelValueType::Integer, ("mHz", Mean)),
            "grid.import_power" | "grid.export_power" => (ChannelValueType::Integer, ("W", Mean)),
            "energy.today" => (ChannelValueType::Integer, ("Wh", Last)),
            "energy.lifetime" => (ChannelValueType::Counter, ("Wh", CounterDelta)),
            "operating_time" | "feed_in_time" => (ChannelValueType::Integer, ("s", Last)),
            "status" | "grid_relay" => (ChannelValueType::Integer, ("code", Last)),
            "temperature" => (ChannelValueType::Integer, ("m°C", Max)),
            "signal" => (ChannelValueType::Integer, ("‰", Mean)),
            "battery.soc" => (ChannelValueType::Integer, ("‰", Last)),
            "battery.voltage" => (ChannelValueType::Integer, ("mV", Mean)),
            "battery.current" => (ChannelValueType::Integer, ("mA", Mean)),
            "battery.temperature" => (ChannelValueType::Integer, ("m°C", Mean)),
            _ => return None,
        };
        (inverter, value_type, definition)
    };
    Some(DeviceChannel {
        target,
        tracker_power,
        typing: ChannelTyping {
            value_type,
            unit: unit.to_owned(),
            scale: 0,
            aggregation,
        },
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
