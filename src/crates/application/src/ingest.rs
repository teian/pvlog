//! The request a logger sends to the ingestion API, how it is checked, and how
//! the values of devices become measurement channels and system values.
//!
//! Every value is an integer in the unit its member name states, times are
//! epoch seconds, and an absent member means that the value is missing. A
//! request is checked completely before anything of it is stored.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    ChannelReading, DecimalValue, MAXIMUM_DEVICE_ID_CHARACTERS, MAXIMUM_MESSAGE_CHARACTERS,
    MAXIMUM_STATE_OF_CHARGE_PERMILLE, ReportedDevice, SuppliedDailySummary,
    validate_reported_devices,
};

/// Version of the request format this server accepts.
pub const INGEST_SCHEMA_VERSION: u32 = 1;
/// Most measurements one request may carry.
pub const MAXIMUM_INGEST_MEASUREMENTS: usize = 5_000;
/// Most energy samples one request may carry.
pub const MAXIMUM_INGEST_ENERGY_SAMPLES: usize = 20_000;
/// Most daily summaries one request may carry.
pub const MAXIMUM_INGEST_DAILY_SUMMARIES: usize = 5_000;
/// Most events one request may carry.
pub const MAXIMUM_INGEST_EVENTS: usize = 5_000;
/// Most devices one request, and one measurement, may describe.
pub const MAXIMUM_INGEST_DEVICES: usize = 64;
/// Most extra values one device may carry in one measurement.
pub const MAXIMUM_INGEST_EXTRA_VALUES: usize = 32;
/// Largest request body as it is sent, compressed or not, in bytes.
pub const MAXIMUM_INGEST_BODY_BYTES: usize = 1024 * 1024;
/// Largest request body after decompression, in bytes.
pub const MAXIMUM_INGEST_DECODED_BYTES: usize = 32 * 1024 * 1024;
/// Content encodings a request body may use.
pub const INGEST_CONTENT_ENCODINGS: [&str; 3] = ["identity", "gzip", "zstd"];

/// Latest time a request may name: the start of the year 2100, in epoch seconds.
const MAXIMUM_EPOCH_SECONDS: i64 = 4_102_444_800;
const MAXIMUM_EVENT_ID_CHARACTERS: usize = 128;
const MAXIMUM_EVENT_TEXT_CHARACTERS: usize = 500;
const MAXIMUM_EXTRA_KEY_CHARACTERS: usize = 48;
const MAXIMUM_UNIT_CHARACTERS: usize = 16;
const MAXIMUM_SOURCE_TEXT_CHARACTERS: usize = 64;

/// What the ingestion API accepts, for a client that adapts to the server.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IngestCapabilities {
    pub schema_versions: Vec<u32>,
    pub content_encodings: Vec<&'static str>,
    pub limits: IngestLimits,
}

/// Limits of one ingestion request.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IngestLimits {
    pub measurements: usize,
    pub energy_samples: usize,
    pub daily_summaries: usize,
    pub events: usize,
    pub devices: usize,
    pub extra_values_per_device: usize,
    /// Largest body as sent, in bytes.
    pub body_bytes: usize,
    /// Largest body after decompression, in bytes.
    pub decoded_body_bytes: usize,
}

/// The capabilities of this server.
#[must_use]
pub fn ingest_capabilities() -> IngestCapabilities {
    IngestCapabilities {
        schema_versions: vec![INGEST_SCHEMA_VERSION],
        content_encodings: INGEST_CONTENT_ENCODINGS.to_vec(),
        limits: IngestLimits {
            measurements: MAXIMUM_INGEST_MEASUREMENTS,
            energy_samples: MAXIMUM_INGEST_ENERGY_SAMPLES,
            daily_summaries: MAXIMUM_INGEST_DAILY_SUMMARIES,
            events: MAXIMUM_INGEST_EVENTS,
            devices: MAXIMUM_INGEST_DEVICES,
            extra_values_per_device: MAXIMUM_INGEST_EXTRA_VALUES,
            body_bytes: MAXIMUM_INGEST_BODY_BYTES,
            decoded_body_bytes: MAXIMUM_INGEST_DECODED_BYTES,
        },
    }
}

/// Everything a logger sends in one request.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct IngestRequest {
    pub schema_version: u32,
    pub source: Option<IngestSource>,
    #[serde(default)]
    pub devices: Vec<IngestDevice>,
    #[serde(default)]
    pub measurements: Vec<IngestMeasurement>,
    #[serde(default)]
    pub energy_samples: Vec<IngestEnergySample>,
    #[serde(default)]
    pub daily_summaries: Vec<IngestDailySummary>,
    #[serde(default)]
    pub events: Vec<IngestEvent>,
}

/// The software that sends the request.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct IngestSource {
    pub name: Option<String>,
    pub version: Option<String>,
}

/// Identity of a device the logger reads.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct IngestDevice {
    pub id: String,
    pub kind: Option<String>,
    pub family_id: Option<String>,
    pub configured_name: Option<String>,
    pub reported_name: Option<String>,
    pub model: Option<String>,
    pub firmware: Option<String>,
    pub transport: Option<String>,
}

impl From<&IngestDevice> for ReportedDevice {
    fn from(device: &IngestDevice) -> Self {
        Self {
            id: device.id.clone(),
            kind: device.kind.clone(),
            family_id: device.family_id.clone(),
            configured_name: device.configured_name.clone(),
            reported_name: device.reported_name.clone(),
            model: device.model.clone(),
            firmware: device.firmware.clone(),
            transport: device.transport.clone(),
        }
    }
}

/// What was measured at one time: values of the system, of devices, or both.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct IngestMeasurement {
    /// Epoch seconds.
    pub measured_at: i64,
    pub system: Option<IngestSystemValues>,
    #[serde(default)]
    pub devices: Vec<IngestDeviceValues>,
}

/// Values of the whole system. A value stated here wins over the one derived
/// from the devices of the same measurement.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct IngestSystemValues {
    pub generation_power_w: Option<i64>,
    pub generation_energy_wh: Option<i64>,
    pub generation_lifetime_wh: Option<i64>,
    pub consumption_power_w: Option<i64>,
    pub consumption_energy_wh: Option<i64>,
    pub consumption_lifetime_wh: Option<i64>,
    pub grid_import_power_w: Option<i64>,
    pub grid_export_power_w: Option<i64>,
    pub grid_import_energy_wh: Option<i64>,
    pub grid_export_energy_wh: Option<i64>,
    pub battery_state_of_charge_permille: Option<i64>,
    /// Positive while the battery charges.
    pub battery_power_w: Option<i64>,
    pub battery_energy_wh: Option<i64>,
    pub temperature_millicelsius: Option<i64>,
    pub voltage_mv: Option<i64>,
    pub message: Option<String>,
}

/// Values of one device at one time.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct IngestDeviceValues {
    pub id: String,
    #[serde(default)]
    pub ac: Vec<IngestPhase>,
    pub grid_frequency_mhz: Option<i64>,
    pub grid_import_power_w: Option<i64>,
    pub grid_export_power_w: Option<i64>,
    pub energy_today_wh: Option<i64>,
    pub energy_lifetime_wh: Option<i64>,
    pub operating_time_s: Option<i64>,
    pub feed_in_time_s: Option<i64>,
    pub status_code: Option<i64>,
    pub grid_relay_status_code: Option<i64>,
    pub temperature_millicelsius: Option<i64>,
    pub signal_permille: Option<i64>,
    #[serde(default)]
    pub trackers: Vec<IngestTracker>,
    pub battery: Option<IngestBattery>,
    /// Values without a member of their own, by a name the logger chooses.
    #[serde(default)]
    pub extra: BTreeMap<String, IngestExtraValue>,
}

/// One AC phase of a device.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct IngestPhase {
    /// 1 to 3.
    pub phase: i64,
    pub power_w: Option<i64>,
    pub current_ma: Option<i64>,
    pub voltage_mv: Option<i64>,
}

/// One DC input of a device.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct IngestTracker {
    /// 1 to 255, as the device numbers it.
    pub number: i64,
    pub power_w: Option<i64>,
    pub current_ma: Option<i64>,
    pub voltage_mv: Option<i64>,
}

/// The battery of a device.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct IngestBattery {
    pub state_of_charge_permille: Option<i64>,
    pub voltage_mv: Option<i64>,
    pub current_ma: Option<i64>,
    pub temperature_millicelsius: Option<i64>,
}

/// A value with its unit that becomes a measurement channel of its device.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct IngestExtraValue {
    pub value: serde_json::Number,
    pub unit: Option<String>,
}

/// An energy sample from the archive of a device.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct IngestEnergySample {
    pub device_id: String,
    /// Epoch seconds.
    pub measured_at: i64,
    pub energy_lifetime_wh: Option<i64>,
    pub power_w: Option<i64>,
}

/// The summary of one local day, of a device or of the system.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct IngestDailySummary {
    pub device_id: Option<String>,
    /// Local calendar date in the system's timezone, as `YYYY-MM-DD`.
    pub date: String,
    pub energy_wh: Option<i64>,
    pub energy_lifetime_wh: Option<i64>,
    #[serde(default)]
    pub complete: bool,
    pub export_energy_wh: Option<i64>,
    pub consumption_energy_wh: Option<i64>,
    pub peak_power_w: Option<i64>,
    pub peak_time: Option<String>,
    pub weather_condition: Option<String>,
    pub temperature_min_millicelsius: Option<i64>,
    pub temperature_max_millicelsius: Option<i64>,
    #[serde(default)]
    pub import_energy_wh_by_band: BTreeMap<String, i64>,
}

impl IngestDailySummary {
    /// The summary in the form daily summaries are stored from.
    #[must_use]
    pub fn supplied(&self) -> SuppliedDailySummary {
        SuppliedDailySummary {
            date: self.date.clone(),
            device_id: self.device_id.clone(),
            generation_energy_wh: self.energy_wh,
            export_energy_wh: self.export_energy_wh,
            consumption_energy_wh: self.consumption_energy_wh,
            generation_lifetime_wh: self.energy_lifetime_wh,
            complete: self.complete,
            peak_power_watts: self.peak_power_w,
            peak_time: self.peak_time.clone(),
            weather_condition: self.weather_condition.clone(),
            temperature_min_millidegrees_celsius: self.temperature_min_millicelsius,
            temperature_max_millidegrees_celsius: self.temperature_max_millicelsius,
            import_energy_wh_by_band: self.import_energy_wh_by_band.clone(),
        }
    }
}

/// An event a device reported.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct IngestEvent {
    pub device_id: String,
    /// The device's own identifier of the event; a number is read as its digits.
    #[serde(default, deserialize_with = "text_or_integer")]
    pub event_id: Option<String>,
    /// Epoch seconds.
    pub occurred_at: i64,
    pub code: Option<i64>,
    #[serde(rename = "type")]
    pub event_type: Option<String>,
    pub category: Option<String>,
    pub group: Option<String>,
    pub message: Option<String>,
    pub old_value: Option<String>,
    pub new_value: Option<String>,
    pub user_group: Option<String>,
}

/// Reads an identifier that a logger may send as text or as an integer.
fn text_or_integer<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    match Option::<Value>::deserialize(deserializer)? {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) => Ok(Some(text)),
        Some(Value::Number(number)) if number.is_i64() || number.is_u64() => {
            Ok(Some(number.to_string()))
        }
        Some(_) => Err(serde::de::Error::custom("expected text or an integer")),
    }
}

/// The members an object may have and what each of them holds.
enum Shape {
    /// A number, text or boolean.
    Value,
    Object(&'static [(&'static str, &'static Shape)]),
    List(&'static Shape),
    /// An object whose member names the client chooses.
    Map(&'static Shape),
}

const VALUE: &Shape = &Shape::Value;
const ELECTRICAL: &[(&str, &Shape)] = &[
    ("powerW", VALUE),
    ("currentMa", VALUE),
    ("voltageMv", VALUE),
];
const PHASE: &Shape = &Shape::Object(&[
    ("phase", VALUE),
    ELECTRICAL[0],
    ELECTRICAL[1],
    ELECTRICAL[2],
]);
const TRACKER: &Shape = &Shape::Object(&[
    ("number", VALUE),
    ELECTRICAL[0],
    ELECTRICAL[1],
    ELECTRICAL[2],
]);
const DEVICE_VALUES: &Shape = &Shape::Object(&[
    ("id", VALUE),
    ("ac", &Shape::List(PHASE)),
    ("gridFrequencyMhz", VALUE),
    ("gridImportPowerW", VALUE),
    ("gridExportPowerW", VALUE),
    ("energyTodayWh", VALUE),
    ("energyLifetimeWh", VALUE),
    ("operatingTimeS", VALUE),
    ("feedInTimeS", VALUE),
    ("statusCode", VALUE),
    ("gridRelayStatusCode", VALUE),
    ("temperatureMillicelsius", VALUE),
    ("signalPermille", VALUE),
    ("trackers", &Shape::List(TRACKER)),
    (
        "battery",
        &Shape::Object(&[
            ("stateOfChargePermille", VALUE),
            ("voltageMv", VALUE),
            ("currentMa", VALUE),
            ("temperatureMillicelsius", VALUE),
        ]),
    ),
    (
        "extra",
        &Shape::Map(&Shape::Object(&[("value", VALUE), ("unit", VALUE)])),
    ),
]);
const REQUEST: &Shape = &Shape::Object(&[
    ("schemaVersion", VALUE),
    (
        "source",
        &Shape::Object(&[("name", VALUE), ("version", VALUE)]),
    ),
    (
        "devices",
        &Shape::List(&Shape::Object(&[
            ("id", VALUE),
            ("kind", VALUE),
            ("familyId", VALUE),
            ("configuredName", VALUE),
            ("reportedName", VALUE),
            ("model", VALUE),
            ("firmware", VALUE),
            ("transport", VALUE),
        ])),
    ),
    (
        "measurements",
        &Shape::List(&Shape::Object(&[
            ("measuredAt", VALUE),
            (
                "system",
                &Shape::Object(&[
                    ("generationPowerW", VALUE),
                    ("generationEnergyWh", VALUE),
                    ("generationLifetimeWh", VALUE),
                    ("consumptionPowerW", VALUE),
                    ("consumptionEnergyWh", VALUE),
                    ("consumptionLifetimeWh", VALUE),
                    ("gridImportPowerW", VALUE),
                    ("gridExportPowerW", VALUE),
                    ("gridImportEnergyWh", VALUE),
                    ("gridExportEnergyWh", VALUE),
                    ("batteryStateOfChargePermille", VALUE),
                    ("batteryPowerW", VALUE),
                    ("batteryEnergyWh", VALUE),
                    ("temperatureMillicelsius", VALUE),
                    ("voltageMv", VALUE),
                    ("message", VALUE),
                ]),
            ),
            ("devices", &Shape::List(DEVICE_VALUES)),
        ])),
    ),
    (
        "energySamples",
        &Shape::List(&Shape::Object(&[
            ("deviceId", VALUE),
            ("measuredAt", VALUE),
            ("energyLifetimeWh", VALUE),
            ("powerW", VALUE),
        ])),
    ),
    (
        "dailySummaries",
        &Shape::List(&Shape::Object(&[
            ("deviceId", VALUE),
            ("date", VALUE),
            ("energyWh", VALUE),
            ("energyLifetimeWh", VALUE),
            ("complete", VALUE),
            ("exportEnergyWh", VALUE),
            ("consumptionEnergyWh", VALUE),
            ("peakPowerW", VALUE),
            ("peakTime", VALUE),
            ("weatherCondition", VALUE),
            ("temperatureMinMillicelsius", VALUE),
            ("temperatureMaxMillicelsius", VALUE),
            ("importEnergyWhByBand", &Shape::Map(VALUE)),
        ])),
    ),
    (
        "events",
        &Shape::List(&Shape::Object(&[
            ("deviceId", VALUE),
            ("eventId", VALUE),
            ("occurredAt", VALUE),
            ("code", VALUE),
            ("type", VALUE),
            ("category", VALUE),
            ("group", VALUE),
            ("message", VALUE),
            ("oldValue", VALUE),
            ("newValue", VALUE),
            ("userGroup", VALUE),
        ])),
    ),
]);

/// Finds the first member of a request body that schema version 1 does not
/// define, as its path, for example `measurements[3].devices[0].powerW`.
///
/// An unknown member is refused rather than ignored: a logger that sends a
/// value the server would drop must learn so on the first request.
#[must_use]
pub fn unknown_ingest_member(body: &Value) -> Option<String> {
    fn walk(value: &Value, shape: &Shape, path: &str) -> Option<String> {
        match (shape, value) {
            (Shape::Object(members), Value::Object(object)) => {
                object.iter().find_map(|(name, value)| {
                    let path = if path.is_empty() {
                        name.clone()
                    } else {
                        format!("{path}.{name}")
                    };
                    match members.iter().find(|(known, _)| known == name) {
                        Some((_, shape)) => walk(value, shape, &path),
                        None => Some(path),
                    }
                })
            }
            (Shape::List(item), Value::Array(items)) => items
                .iter()
                .enumerate()
                .find_map(|(index, value)| walk(value, item, &format!("{path}[{index}]"))),
            (Shape::Map(item), Value::Object(object)) => object
                .iter()
                .find_map(|(name, value)| walk(value, item, &format!("{path}.{name}"))),
            // A value of the wrong kind is reported when the body is read.
            _ => None,
        }
    }
    walk(body, REQUEST, "")
}

/// Why a whole request is refused before its items are looked at.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum IngestRequestProblem {
    /// The request states a schema version this server does not accept.
    UnsupportedSchemaVersion(u32),
    /// A list is longer than the published limit allows.
    LimitExceeded { member: &'static str, limit: usize },
}

/// One item of a request that cannot be stored, by its list and position.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IngestRejection {
    /// The list the item is in: `devices`, `measurements`, `energySamples`,
    /// `dailySummaries` or `events`.
    pub category: &'static str,
    /// Position of the item in that list, counted from zero.
    pub index: usize,
    /// Path of the member within the item; empty when the item as a whole is meant.
    pub member: String,
    /// Stable code of what is wrong with it.
    pub reason: &'static str,
}

/// Checks the size of a request against the published limits.
///
/// # Errors
///
/// Returns the schema version when it is not accepted, or the first list that
/// is longer than its limit.
pub fn check_ingest_request(request: &IngestRequest) -> Result<(), IngestRequestProblem> {
    if request.schema_version != INGEST_SCHEMA_VERSION {
        return Err(IngestRequestProblem::UnsupportedSchemaVersion(
            request.schema_version,
        ));
    }
    for (length, member, limit) in [
        (request.devices.len(), "devices", MAXIMUM_INGEST_DEVICES),
        (
            request.measurements.len(),
            "measurements",
            MAXIMUM_INGEST_MEASUREMENTS,
        ),
        (
            request.energy_samples.len(),
            "energySamples",
            MAXIMUM_INGEST_ENERGY_SAMPLES,
        ),
        (
            request.daily_summaries.len(),
            "dailySummaries",
            MAXIMUM_INGEST_DAILY_SUMMARIES,
        ),
        (request.events.len(), "events", MAXIMUM_INGEST_EVENTS),
    ] {
        if length > limit {
            return Err(IngestRequestProblem::LimitExceeded { member, limit });
        }
    }
    Ok(())
}

/// Whether text can name a device in a channel key.
fn is_device_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAXIMUM_DEVICE_ID_CHARACTERS
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

/// Whether a time lies between the epoch and the year 2100.
fn is_time(seconds: i64) -> bool {
    (0..=MAXIMUM_EPOCH_SECONDS).contains(&seconds)
}

/// Collects what is wrong with the items of one list.
struct Findings<'a> {
    rejections: &'a mut Vec<IngestRejection>,
    category: &'static str,
    index: usize,
}

impl Findings<'_> {
    fn reject(&mut self, member: impl Into<String>, reason: &'static str) {
        self.rejections.push(IngestRejection {
            category: self.category,
            index: self.index,
            member: member.into(),
            reason,
        });
    }

    /// Refuses a value below zero.
    fn not_negative(&mut self, member: impl Into<String>, value: Option<i64>) {
        if value.is_some_and(|value| value < 0) {
            self.reject(member, "negative");
        }
    }

    /// Refuses a value outside `0..=maximum`.
    fn within(&mut self, member: impl Into<String>, value: Option<i64>, maximum: i64) {
        if value.is_some_and(|value| !(0..=maximum).contains(&value)) {
            self.reject(member, "out_of_range");
        }
    }

    /// Refuses text that is blank, longer than `maximum` or not printable.
    fn text(&mut self, member: impl Into<String>, text: Option<&String>, maximum: usize) {
        if text.is_some_and(|text| {
            text.trim().is_empty()
                || text.chars().count() > maximum
                || text.chars().any(char::is_control)
        }) {
            self.reject(member, "invalid_text");
        }
    }
}

/// Checks every item of a request and returns each one that cannot be stored.
///
/// The request is refused as a whole when the result is not empty, so that a
/// logger never has to work out which part of a request was kept.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn validate_ingest_request(request: &IngestRequest) -> Vec<IngestRejection> {
    let mut rejections = Vec::new();
    if let Some(source) = &request.source {
        let mut findings = Findings {
            rejections: &mut rejections,
            category: "source",
            index: 0,
        };
        findings.text("name", source.name.as_ref(), MAXIMUM_SOURCE_TEXT_CHARACTERS);
        findings.text(
            "version",
            source.version.as_ref(),
            MAXIMUM_SOURCE_TEXT_CHARACTERS,
        );
    }
    for (index, device) in request.devices.iter().enumerate() {
        let repeated = request.devices[..index]
            .iter()
            .any(|other| other.id == device.id);
        if repeated || validate_reported_devices(&[ReportedDevice::from(device)]).is_err() {
            rejections.push(IngestRejection {
                category: "devices",
                index,
                member: String::new(),
                reason: if repeated {
                    "duplicate"
                } else {
                    "invalid_device"
                },
            });
        }
    }
    let mut times = BTreeSet::new();
    for (index, measurement) in request.measurements.iter().enumerate() {
        let mut findings = Findings {
            rejections: &mut rejections,
            category: "measurements",
            index,
        };
        if !is_time(measurement.measured_at) {
            findings.reject("measuredAt", "out_of_range");
        } else if !times.insert(measurement.measured_at) {
            findings.reject("measuredAt", "duplicate");
        }
        if measurement.system.is_none() && measurement.devices.is_empty() {
            findings.reject("", "empty");
        }
        if measurement.devices.len() > MAXIMUM_INGEST_DEVICES {
            findings.reject("devices", "too_many");
        }
        if let Some(system) = &measurement.system {
            validate_system(&mut findings, system);
        }
        for (position, device) in measurement.devices.iter().enumerate() {
            let path = format!("devices[{position}]");
            if !is_device_id(&device.id) {
                findings.reject(format!("{path}.id"), "invalid_device_id");
            } else if measurement.devices[..position]
                .iter()
                .any(|other| other.id == device.id)
            {
                findings.reject(format!("{path}.id"), "duplicate");
            }
            validate_device_values(&mut findings, &path, device);
        }
    }
    let mut samples = BTreeSet::new();
    for (index, sample) in request.energy_samples.iter().enumerate() {
        let mut findings = Findings {
            rejections: &mut rejections,
            category: "energySamples",
            index,
        };
        if !is_device_id(&sample.device_id) {
            findings.reject("deviceId", "invalid_device_id");
        }
        if !is_time(sample.measured_at) {
            findings.reject("measuredAt", "out_of_range");
        } else if !samples.insert((sample.device_id.as_str(), sample.measured_at)) {
            findings.reject("measuredAt", "duplicate");
        }
        if sample.energy_lifetime_wh.is_none() && sample.power_w.is_none() {
            findings.reject("", "empty");
        }
        findings.not_negative("energyLifetimeWh", sample.energy_lifetime_wh);
    }
    let mut days = BTreeSet::new();
    for (index, summary) in request.daily_summaries.iter().enumerate() {
        let mut findings = Findings {
            rejections: &mut rejections,
            category: "dailySummaries",
            index,
        };
        if let Err(member) = summary.supplied().validate() {
            findings.reject(summary_member(member), "invalid");
        } else if !days.insert((summary.device_id.as_deref(), summary.date.as_str())) {
            findings.reject("date", "duplicate");
        }
    }
    for (index, event) in request.events.iter().enumerate() {
        let mut findings = Findings {
            rejections: &mut rejections,
            category: "events",
            index,
        };
        findings.text(
            "deviceId",
            Some(&event.device_id),
            MAXIMUM_DEVICE_ID_CHARACTERS,
        );
        findings.text(
            "eventId",
            event.event_id.as_ref(),
            MAXIMUM_EVENT_ID_CHARACTERS,
        );
        if !is_time(event.occurred_at) {
            findings.reject("occurredAt", "out_of_range");
        }
        for (member, text) in [
            ("type", &event.event_type),
            ("category", &event.category),
            ("group", &event.group),
            ("message", &event.message),
            ("oldValue", &event.old_value),
            ("newValue", &event.new_value),
            ("userGroup", &event.user_group),
        ] {
            findings.text(member, text.as_ref(), MAXIMUM_EVENT_TEXT_CHARACTERS);
        }
    }
    rejections
}

/// Names a member of a daily summary as the ingestion API spells it.
fn summary_member(member: &'static str) -> &'static str {
    match member {
        "generationEnergyWh" => "energyWh",
        "generationLifetimeWh" => "energyLifetimeWh",
        "peakPowerWatts" => "peakPowerW",
        "temperatureMaxMillidegreesCelsius" => "temperatureMaxMillicelsius",
        other => other,
    }
}

fn validate_system(findings: &mut Findings<'_>, system: &IngestSystemValues) {
    for (member, value) in [
        ("system.generationLifetimeWh", system.generation_lifetime_wh),
        (
            "system.consumptionLifetimeWh",
            system.consumption_lifetime_wh,
        ),
        ("system.gridImportPowerW", system.grid_import_power_w),
        ("system.gridExportPowerW", system.grid_export_power_w),
        ("system.gridImportEnergyWh", system.grid_import_energy_wh),
        ("system.gridExportEnergyWh", system.grid_export_energy_wh),
        ("system.batteryEnergyWh", system.battery_energy_wh),
        ("system.voltageMv", system.voltage_mv),
    ] {
        findings.not_negative(member, value);
    }
    findings.within(
        "system.batteryStateOfChargePermille",
        system.battery_state_of_charge_permille,
        MAXIMUM_STATE_OF_CHARGE_PERMILLE,
    );
    // A grid connection carries power in one direction at a time.
    if system.grid_import_power_w.is_some_and(|watts| watts > 0)
        && system.grid_export_power_w.is_some_and(|watts| watts > 0)
    {
        findings.reject("system.gridExportPowerW", "contradicts_import");
    }
    for (member, value) in [
        (
            "system.temperatureMillicelsius",
            system.temperature_millicelsius,
        ),
        ("system.voltageMv", system.voltage_mv),
    ] {
        if value.is_some_and(|value| i32::try_from(value).is_err()) {
            findings.reject(member, "out_of_range");
        }
    }
    if system
        .message
        .as_ref()
        .is_some_and(|message| message.chars().count() > MAXIMUM_MESSAGE_CHARACTERS)
    {
        findings.reject("system.message", "too_long");
    }
}

fn validate_device_values(findings: &mut Findings<'_>, path: &str, device: &IngestDeviceValues) {
    for (position, phase) in device.ac.iter().enumerate() {
        if !(1..=3).contains(&phase.phase) {
            findings.reject(format!("{path}.ac[{position}].phase"), "out_of_range");
        } else if device.ac[..position]
            .iter()
            .any(|other| other.phase == phase.phase)
        {
            findings.reject(format!("{path}.ac[{position}].phase"), "duplicate");
        }
    }
    for (position, tracker) in device.trackers.iter().enumerate() {
        if !(1..=255).contains(&tracker.number) {
            findings.reject(
                format!("{path}.trackers[{position}].number"),
                "out_of_range",
            );
        } else if device.trackers[..position]
            .iter()
            .any(|other| other.number == tracker.number)
        {
            findings.reject(format!("{path}.trackers[{position}].number"), "duplicate");
        }
    }
    for (member, value) in [
        ("gridFrequencyMhz", device.grid_frequency_mhz),
        ("gridImportPowerW", device.grid_import_power_w),
        ("gridExportPowerW", device.grid_export_power_w),
        ("energyTodayWh", device.energy_today_wh),
        ("energyLifetimeWh", device.energy_lifetime_wh),
        ("operatingTimeS", device.operating_time_s),
        ("feedInTimeS", device.feed_in_time_s),
    ] {
        findings.not_negative(format!("{path}.{member}"), value);
    }
    findings.within(
        format!("{path}.signalPermille"),
        device.signal_permille,
        MAXIMUM_STATE_OF_CHARGE_PERMILLE,
    );
    if let Some(battery) = &device.battery {
        findings.within(
            format!("{path}.battery.stateOfChargePermille"),
            battery.state_of_charge_permille,
            MAXIMUM_STATE_OF_CHARGE_PERMILLE,
        );
    }
    if device.extra.len() > MAXIMUM_INGEST_EXTRA_VALUES {
        findings.reject(format!("{path}.extra"), "too_many");
    }
    for (key, extra) in &device.extra {
        let member = format!("{path}.extra.{key}");
        let key_valid = !key.is_empty()
            && key.len() <= MAXIMUM_EXTRA_KEY_CHARACTERS
            && key.bytes().all(|byte| {
                byte.is_ascii_lowercase()
                    || byte.is_ascii_digit()
                    || matches!(byte, b'_' | b'-' | b'.')
            });
        if !key_valid {
            findings.reject(member, "invalid_key");
        } else if DecimalValue::parse(&extra.value.to_string()).is_none() {
            findings.reject(format!("{member}.value"), "not_a_plain_decimal");
        } else if extra.unit.as_ref().is_some_and(|unit| {
            unit.is_empty()
                || unit.chars().count() > MAXIMUM_UNIT_CHARACTERS
                || unit.chars().any(char::is_control)
        }) {
            findings.reject(format!("{member}.unit"), "invalid_unit");
        }
    }
}

/// The measurement channels the values of a device are stored in, by the key
/// of the device key scheme, each with its value.
///
/// A value with a member of its own takes the key and the unit the scheme
/// defines. An extra value takes the key `inv.<serial>.x.<name>` and brings
/// its own unit. An absent value produces no entry.
#[must_use]
pub fn device_channel_readings(device: &IngestDeviceValues) -> Vec<(String, ChannelReading)> {
    let serial = &device.id;
    let mut readings = Vec::new();
    let mut integer = |measurement: String, value: Option<i64>| {
        if let Some(value) = value {
            readings.push((
                format!("inv.{serial}.{measurement}"),
                ChannelReading {
                    value: Some(DecimalValue::integer(value)),
                    ..ChannelReading::default()
                },
            ));
        }
    };
    for phase in &device.ac {
        let prefix = format!("ac.l{}", phase.phase);
        integer(format!("{prefix}.power"), phase.power_w);
        integer(format!("{prefix}.current"), phase.current_ma);
        integer(format!("{prefix}.voltage"), phase.voltage_mv);
    }
    for (measurement, value) in [
        ("grid.frequency", device.grid_frequency_mhz),
        ("grid.import_power", device.grid_import_power_w),
        ("grid.export_power", device.grid_export_power_w),
        ("energy.today", device.energy_today_wh),
        ("energy.lifetime", device.energy_lifetime_wh),
        ("operating_time", device.operating_time_s),
        ("feed_in_time", device.feed_in_time_s),
        ("status", device.status_code),
        ("grid_relay", device.grid_relay_status_code),
        ("temperature", device.temperature_millicelsius),
        ("signal", device.signal_permille),
    ] {
        integer(measurement.to_owned(), value);
    }
    for tracker in &device.trackers {
        let prefix = format!("mppt.{}", tracker.number);
        integer(format!("{prefix}.power"), tracker.power_w);
        integer(format!("{prefix}.current"), tracker.current_ma);
        integer(format!("{prefix}.voltage"), tracker.voltage_mv);
    }
    if let Some(battery) = &device.battery {
        integer("battery.soc".to_owned(), battery.state_of_charge_permille);
        integer("battery.voltage".to_owned(), battery.voltage_mv);
        integer("battery.current".to_owned(), battery.current_ma);
        integer(
            "battery.temperature".to_owned(),
            battery.temperature_millicelsius,
        );
    }
    for (key, extra) in &device.extra {
        readings.push((
            format!("inv.{serial}.x.{key}"),
            ChannelReading {
                value: DecimalValue::parse(&extra.value.to_string()),
                unit: extra.unit.clone(),
                ..ChannelReading::default()
            },
        ));
    }
    readings
}

/// System values that the devices of one measurement add up to.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DeviceTotals {
    /// Sum of the AC power of every phase that reported one.
    pub generation_power_w: Option<i64>,
    pub grid_import_power_w: Option<i64>,
    pub grid_export_power_w: Option<i64>,
    /// Mean state of charge of the batteries that reported one, in basis points.
    pub battery_state_basis_points: Option<i32>,
}

/// Adds up the values of the devices of one measurement.
///
/// A device that reported nothing for a value contributes nothing to it, and a
/// value no device reported stays missing instead of becoming zero.
#[must_use]
pub fn device_totals(devices: &[IngestDeviceValues]) -> DeviceTotals {
    let sum = |values: &mut dyn Iterator<Item = i64>| {
        values.fold(None, |total: Option<i64>, value| {
            Some(total.unwrap_or_default().saturating_add(value))
        })
    };
    let states: Vec<i64> = devices
        .iter()
        .filter_map(|device| device.battery?.state_of_charge_permille)
        .collect();
    DeviceTotals {
        generation_power_w: sum(&mut devices
            .iter()
            .flat_map(|device| device.ac.iter().filter_map(|phase| phase.power_w))),
        grid_import_power_w: sum(&mut devices
            .iter()
            .filter_map(|device| device.grid_import_power_w)),
        grid_export_power_w: sum(&mut devices
            .iter()
            .filter_map(|device| device.grid_export_power_w)),
        battery_state_basis_points: i64::try_from(states.len())
            .ok()
            .filter(|count| *count > 0)
            // Per mille to basis points is a factor of ten.
            .and_then(|count| i32::try_from(states.iter().sum::<i64>() * 10 / count).ok()),
    }
}

/// The lifetime counters of the devices of one measurement, by device.
pub type LifetimeCounters = BTreeMap<String, i64>;

/// What the lifetime counters of devices say about the system.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DerivedGeneration {
    /// Sum of the counters. Missing while a device that reported before is silent.
    pub lifetime_wh: Option<i64>,
    /// Energy generated since the previous measurement.
    pub energy_wh: Option<i64>,
    /// True when a counter is lower than it was before.
    pub counter_reset: bool,
}

/// Derives the system's lifetime counter and interval energy from the
/// lifetime counters of its devices.
///
/// `previous` holds the counters of the measurement before this one. Energy
/// is the sum of what each counter grew by, and is derived only when the same
/// devices reported both times and no counter went back: across a silent or a
/// new device and across a reset the energy of the interval is unknown, not
/// zero. The lifetime sum is missing while a device that reported before is
/// silent, because a sum over fewer devices would look like a counter reset.
#[must_use]
pub fn derive_generation(
    current: &LifetimeCounters,
    previous: Option<&LifetimeCounters>,
) -> DerivedGeneration {
    if current.is_empty() {
        return DerivedGeneration::default();
    }
    let total = current.values().sum::<i64>();
    let Some(previous) = previous else {
        return DerivedGeneration {
            lifetime_wh: Some(total),
            ..DerivedGeneration::default()
        };
    };
    if !previous.keys().all(|device| current.contains_key(device)) {
        return DerivedGeneration::default();
    }
    let same_devices = previous.len() == current.len();
    let counter_reset = previous
        .iter()
        .any(|(device, before)| current.get(device).is_some_and(|now| now < before));
    DerivedGeneration {
        lifetime_wh: Some(total),
        energy_wh: (same_devices && !counter_reset).then(|| total - previous.values().sum::<i64>()),
        counter_reset,
    }
}
