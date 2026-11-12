//! What a source reports about its devices next to their measurements: who
//! they are, and the state of a battery attached to them.

use std::collections::BTreeMap;

use crate::channel_values::{ChannelReading, DecimalValue, device_channel};

/// Most devices one reading may describe.
pub const MAXIMUM_REPORTED_DEVICES: usize = 64;

/// Longest device identifier, counted in characters.
pub const MAXIMUM_DEVICE_ID_CHARACTERS: usize = 64;

/// Longest name, model, firmware or other identity text, counted in characters.
pub const MAXIMUM_DEVICE_TEXT_CHARACTERS: usize = 120;

/// Largest state of charge in the unit of the device key scheme, per mille.
pub const MAXIMUM_STATE_OF_CHARGE_PERMILLE: i64 = 1_000;

/// The identity a source reports for one of its devices.
///
/// A member that is `None` was not reported; what is already known stays.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ReportedDevice {
    /// The device as the source names it; for an inverter its serial number.
    pub id: String,
    /// Kind of device, `inverter` when not stated.
    pub kind: Option<String>,
    /// Device family identifier, such as SMA's `SUSy` ID.
    pub family_id: Option<String>,
    /// Name the operator gave the device in the logger.
    pub configured_name: Option<String>,
    /// Name the device reports for itself.
    pub reported_name: Option<String>,
    pub model: Option<String>,
    pub firmware: Option<String>,
    /// How the logger reaches the device, such as `bluetooth` or `speedwire`.
    pub transport: Option<String>,
}

/// Checks the device identities of one reading.
///
/// # Errors
///
/// Returns when there are too many devices, when an identifier is not one a
/// channel key could carry, when a device is listed twice, or when a text is
/// empty, too long or not printable.
pub fn validate_reported_devices(devices: &[ReportedDevice]) -> Result<(), InvalidReportedDevice> {
    if devices.len() > MAXIMUM_REPORTED_DEVICES {
        return Err(InvalidReportedDevice);
    }
    for (index, device) in devices.iter().enumerate() {
        let id_valid = !device.id.is_empty()
            && device.id.len() <= MAXIMUM_DEVICE_ID_CHARACTERS
            && device
                .id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_');
        let texts = [
            &device.kind,
            &device.family_id,
            &device.configured_name,
            &device.reported_name,
            &device.model,
            &device.firmware,
            &device.transport,
        ];
        let texts_valid = texts.into_iter().flatten().all(|text| {
            !text.trim().is_empty()
                && text.chars().count() <= MAXIMUM_DEVICE_TEXT_CHARACTERS
                && !text.chars().any(char::is_control)
        });
        let repeated = devices[..index].iter().any(|other| other.id == device.id);
        if !id_valid || !texts_valid || repeated {
            return Err(InvalidReportedDevice);
        }
    }
    Ok(())
}

/// A reading describes a device in a way that cannot be stored.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InvalidReportedDevice;

/// State of charge a per-device channel reading carries, in per mille.
///
/// Returns `None` for any other key and for a reading without a value.
fn device_state_of_charge(key: &str, reading: &ChannelReading) -> Option<DecimalValue> {
    let device = device_channel(key)?;
    (device.definition.key == "inv.{serial}.battery.soc")
        .then_some(reading.value)
        .flatten()
}

/// Whether every per-device state of charge of a reading lies between empty and full.
#[must_use]
pub fn device_states_of_charge_in_range(readings: &BTreeMap<String, ChannelReading>) -> bool {
    readings
        .iter()
        .filter_map(|(key, reading)| device_state_of_charge(key, reading))
        .all(|value| {
            value
                .at_scale(0)
                .is_some_and(|permille| (0..=MAXIMUM_STATE_OF_CHARGE_PERMILLE).contains(&permille))
        })
}

/// The system's battery state of charge in basis points, taken from the
/// devices that reported one.
///
/// Several batteries count equally, because their capacities are not known.
/// Returns `None` when no device reported a state of charge.
#[must_use]
pub fn system_state_of_charge_basis_points(
    readings: &BTreeMap<String, ChannelReading>,
) -> Option<i32> {
    let permille: Vec<i64> = readings
        .iter()
        .filter_map(|(key, reading)| device_state_of_charge(key, reading)?.at_scale(0))
        .collect();
    let count = i64::try_from(permille.len())
        .ok()
        .filter(|count| *count > 0)?;
    // Per mille to basis points is a factor of ten; the mean is rounded down.
    i32::try_from(permille.iter().sum::<i64>().checked_mul(10)? / count).ok()
}
