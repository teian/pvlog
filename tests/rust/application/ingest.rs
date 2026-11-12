//! Checking an ingestion request and turning device values into channels and
//! system values.

use pvlog_application::{
    DecimalValue, DerivedGeneration, IngestRequest, IngestRequestProblem, LifetimeCounters,
    check_ingest_request, derive_generation, device_channel, device_channel_readings,
    device_totals, source_profile, unknown_ingest_member, validate_ingest_request,
};
use serde_json::{Value, json};

fn request(body: Value) -> IngestRequest {
    serde_json::from_value(body).unwrap_or_else(|error| panic!("the body is read: {error}"))
}

/// A device that reports every value the ingestion API has a member for.
fn full_device() -> Value {
    json!({
        "id": "2000123456",
        "ac": [
            { "phase": 1, "powerW": 800, "currentMa": 3478, "voltageMv": 230_100 },
            { "phase": 2, "powerW": 790, "currentMa": 3434, "voltageMv": 229_900 },
            { "phase": 3, "powerW": 810, "currentMa": 3521, "voltageMv": 230_400 },
        ],
        "gridFrequencyMhz": 49_980, "gridImportPowerW": 0, "gridExportPowerW": 1_900,
        "energyTodayWh": 4_200, "energyLifetimeWh": 10_000_000,
        "operatingTimeS": 86_400, "feedInTimeS": 80_000,
        "statusCode": 307, "gridRelayStatusCode": 51,
        "temperatureMillicelsius": 41_500, "signalPermille": 730,
        "trackers": [
            { "number": 1, "powerW": 1_300, "currentMa": 3_400, "voltageMv": 384_120 },
            { "number": 255, "powerW": 1_150, "currentMa": 3_050, "voltageMv": 377_000 },
        ],
        "battery": { "stateOfChargePermille": 815, "voltageMv": 51_200,
                     "currentMa": -4_300, "temperatureMillicelsius": 24_000 },
        "extra": { "fan.speed": { "value": 1450.5, "unit": "rpm" } },
    })
}

fn body(measurement: &Value) -> Value {
    json!({ "schemaVersion": 1, "measurements": [measurement] })
}

/// The reasons a request is rejected for, as `list[index].member: reason`.
fn rejections(body: Value) -> Vec<String> {
    validate_ingest_request(&request(body))
        .into_iter()
        .map(|rejection| {
            format!(
                "{}[{}].{}: {}",
                rejection.category, rejection.index, rejection.member, rejection.reason
            )
        })
        .collect()
}

#[test]
fn every_member_of_schema_version_one_is_known() {
    let complete = json!({
        "schemaVersion": 1,
        "source": { "name": "smalog", "version": "0.3.0" },
        "devices": [{ "id": "2000123456", "kind": "inverter", "familyId": "9305",
                      "configuredName": "Dach", "reportedName": "SN: 2000123456",
                      "model": "SB 3000TL-21", "firmware": "3.10.18.R", "transport": "bluetooth" }],
        "measurements": [{
            "measuredAt": 1_800_000_000,
            "system": {
                "generationPowerW": 1, "generationEnergyWh": 1, "generationLifetimeWh": 1,
                "consumptionPowerW": 1, "consumptionEnergyWh": 1, "consumptionLifetimeWh": 1,
                "gridImportPowerW": 1, "gridExportPowerW": 0, "gridImportEnergyWh": 1,
                "gridExportEnergyWh": 1, "batteryStateOfChargePermille": 1, "batteryPowerW": 1,
                "batteryEnergyWh": 1, "temperatureMillicelsius": 1, "voltageMv": 1,
                "message": "ok",
            },
            "devices": [full_device()],
        }],
        "energySamples": [{ "deviceId": "2000123456", "measuredAt": 1_800_000_300,
                            "energyLifetimeWh": 10_000_080, "powerW": 960 }],
        "dailySummaries": [{ "deviceId": "2000123456", "date": "2027-01-14", "energyWh": 5_200,
                             "energyLifetimeWh": 9_995_800, "complete": true,
                             "exportEnergyWh": 1, "consumptionEnergyWh": 1, "peakPowerW": 1,
                             "peakTime": "12:35", "weatherCondition": "Fine",
                             "temperatureMinMillicelsius": 1, "temperatureMaxMillicelsius": 2,
                             "importEnergyWhByBand": { "peak": 1 } }],
        "events": [{ "deviceId": "2000123456", "eventId": "8812", "occurredAt": 1_800_000_000,
                     "code": 1, "type": "Event", "category": "Info", "group": "Parameter",
                     "message": "m", "oldValue": "a", "newValue": "b", "userGroup": "Installer" }],
    });
    assert_eq!(unknown_ingest_member(&complete), None);
    let parsed = request(complete);
    assert_eq!(check_ingest_request(&parsed), Ok(()));
    assert_eq!(validate_ingest_request(&parsed), Vec::new());
}

#[test]
fn an_unknown_member_is_found_wherever_it_is() {
    let cases = [
        (
            json!({ "schemaVersion": 1, "sourceName": "x" }),
            "sourceName",
        ),
        (json!({ "source": { "build": 7 } }), "source.build"),
        (
            json!({ "devices": [{ "id": "1" }, { "id": "2", "serial": "2" }] }),
            "devices[1].serial",
        ),
        (
            json!({ "measurements": [{ "measuredAt": 1, "system": { "powerW": 1 } }] }),
            "measurements[0].system.powerW",
        ),
        (
            json!({ "measurements": [{}, { "devices": [{ "id": "1", "trackers": [{ "no": 1 }] }] }] }),
            "measurements[1].devices[0].trackers[0].no",
        ),
        (
            json!({ "measurements": [{ "devices": [{ "battery": { "soc": 1 } }] }] }),
            "measurements[0].devices[0].battery.soc",
        ),
        (
            json!({ "measurements": [{ "devices": [{ "extra": { "fan": { "value": 1, "note": "x" } } }] }] }),
            "measurements[0].devices[0].extra.fan.note",
        ),
        (
            json!({ "energySamples": [{ "energyWh": 1 }] }),
            "energySamples[0].energyWh",
        ),
        (
            json!({ "dailySummaries": [{ "generationEnergyWh": 1 }] }),
            "dailySummaries[0].generationEnergyWh",
        ),
        (
            json!({ "events": [{ "occurredAtEpochMillis": 1 }] }),
            "events[0].occurredAtEpochMillis",
        ),
    ];
    for (body, path) in cases {
        assert_eq!(unknown_ingest_member(&body).as_deref(), Some(path));
    }
    // Names a logger chooses itself are not members of the schema.
    assert_eq!(
        unknown_ingest_member(&json!({
            "measurements": [{ "devices": [{ "extra": { "anything.goes": { "value": 1 } } }] }],
            "dailySummaries": [{ "importEnergyWhByBand": { "night": 1 } }],
        })),
        None
    );
}

#[test]
fn the_schema_version_and_the_limits_are_checked_first() {
    assert_eq!(
        check_ingest_request(&request(json!({ "schemaVersion": 2 }))),
        Err(IngestRequestProblem::UnsupportedSchemaVersion(2))
    );
    let samples: Vec<Value> = (0..20_001)
        .map(|second| json!({ "deviceId": "1", "measuredAt": second, "powerW": 1 }))
        .collect();
    assert_eq!(
        check_ingest_request(&request(
            json!({ "schemaVersion": 1, "energySamples": samples })
        )),
        Err(IngestRequestProblem::LimitExceeded {
            member: "energySamples",
            limit: 20_000
        })
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn every_item_that_cannot_be_stored_is_named() {
    let device = |members: Value| {
        let mut device = json!({ "id": "2000123456" });
        if let (Some(device), Some(members)) = (device.as_object_mut(), members.as_object()) {
            device.extend(members.clone());
        }
        body(&json!({ "measuredAt": 1, "devices": [device] }))
    };
    let system = |members: Value| body(&json!({ "measuredAt": 1, "system": members }));
    let cases = [
        (
            body(&json!({ "measuredAt": -1, "system": {} })),
            "measurements[0].measuredAt: out_of_range",
        ),
        (body(&json!({ "measuredAt": 1 })), "measurements[0].: empty"),
        (
            body(&json!({ "measuredAt": 1, "devices": [{ "id": "roof inverter" }] })),
            "measurements[0].devices[0].id: invalid_device_id",
        ),
        (
            body(&json!({ "measuredAt": 1, "devices": [{ "id": "1" }, { "id": "1" }] })),
            "measurements[0].devices[1].id: duplicate",
        ),
        (
            device(json!({ "ac": [{ "phase": 4 }] })),
            "measurements[0].devices[0].ac[0].phase: out_of_range",
        ),
        (
            device(json!({ "ac": [{ "phase": 1 }, { "phase": 1 }] })),
            "measurements[0].devices[0].ac[1].phase: duplicate",
        ),
        (
            device(json!({ "trackers": [{ "number": 0 }] })),
            "measurements[0].devices[0].trackers[0].number: out_of_range",
        ),
        (
            device(json!({ "trackers": [{ "number": 256 }] })),
            "measurements[0].devices[0].trackers[0].number: out_of_range",
        ),
        (
            device(json!({ "trackers": [{ "number": 5 }, { "number": 5 }] })),
            "measurements[0].devices[0].trackers[1].number: duplicate",
        ),
        (
            device(json!({ "energyLifetimeWh": -1 })),
            "measurements[0].devices[0].energyLifetimeWh: negative",
        ),
        (
            device(json!({ "signalPermille": 1_001 })),
            "measurements[0].devices[0].signalPermille: out_of_range",
        ),
        (
            device(json!({ "battery": { "stateOfChargePermille": 1_001 } })),
            "measurements[0].devices[0].battery.stateOfChargePermille: out_of_range",
        ),
        (
            device(json!({ "extra": { "Fan Speed": { "value": 1 } } })),
            "measurements[0].devices[0].extra.Fan Speed: invalid_key",
        ),
        (
            device(json!({ "extra": { "tiny": { "value": 1e-12 } } })),
            "measurements[0].devices[0].extra.tiny.value: not_a_plain_decimal",
        ),
        (
            device(json!({ "extra": { "fan": { "value": 1, "unit": "" } } })),
            "measurements[0].devices[0].extra.fan.unit: invalid_unit",
        ),
        (
            system(json!({ "batteryStateOfChargePermille": 1_001 })),
            "measurements[0].system.batteryStateOfChargePermille: out_of_range",
        ),
        (
            system(json!({ "gridImportPowerW": 5, "gridExportPowerW": 5 })),
            "measurements[0].system.gridExportPowerW: contradicts_import",
        ),
        (
            system(json!({ "generationLifetimeWh": -1 })),
            "measurements[0].system.generationLifetimeWh: negative",
        ),
        (
            system(json!({ "message": "a message that is far too long to keep" })),
            "measurements[0].system.message: too_long",
        ),
        (
            json!({ "schemaVersion": 1, "devices": [{ "id": "1" }, { "id": "1" }] }),
            "devices[1].: duplicate",
        ),
        (
            json!({ "schemaVersion": 1, "devices": [{ "id": "1", "firmware": " " }] }),
            "devices[0].: invalid_device",
        ),
        (
            json!({ "schemaVersion": 1, "energySamples": [{ "deviceId": "1", "measuredAt": 1 }] }),
            "energySamples[0].: empty",
        ),
        (
            json!({ "schemaVersion": 1, "dailySummaries": [{ "date": "2027-02-30" }] }),
            "dailySummaries[0].date: invalid",
        ),
        (
            json!({ "schemaVersion": 1, "dailySummaries": [{ "date": "2027-01-14", "energyWh": -1 }] }),
            "dailySummaries[0].energyWh: invalid",
        ),
        (
            json!({ "schemaVersion": 1, "events": [{ "deviceId": " ", "occurredAt": 1 }] }),
            "events[0].deviceId: invalid_text",
        ),
    ];
    for (body, expected) in cases {
        assert_eq!(rejections(body), [expected]);
    }
    // The same time, device and date may appear once per request.
    assert_eq!(
        rejections(json!({
            "schemaVersion": 1,
            "measurements": [{ "measuredAt": 1, "system": {} }, { "measuredAt": 1, "system": {} }],
            "energySamples": [
                { "deviceId": "1", "measuredAt": 1, "powerW": 1 },
                { "deviceId": "2", "measuredAt": 1, "powerW": 1 },
                { "deviceId": "1", "measuredAt": 1, "powerW": 2 },
            ],
            "dailySummaries": [
                { "date": "2027-01-14" },
                { "deviceId": "1", "date": "2027-01-14" },
                { "date": "2027-01-14" },
            ],
        })),
        [
            "measurements[1].measuredAt: duplicate",
            "energySamples[2].measuredAt: duplicate",
            "dailySummaries[2].date: duplicate",
        ]
    );
}

#[test]
fn device_values_are_stored_under_the_keys_of_the_profile() {
    let parsed = request(body(
        &json!({ "measuredAt": 1, "devices": [full_device()] }),
    ));
    let readings = device_channel_readings(&parsed.measurements[0].devices[0]);
    let value = |key: &str| {
        readings
            .iter()
            .find(|(candidate, _)| candidate == &format!("inv.2000123456.{key}"))
            .and_then(|(_, reading)| reading.value)
    };
    assert_eq!(value("ac.l2.current"), Some(DecimalValue::integer(3_434)));
    assert_eq!(
        value("mppt.255.voltage"),
        Some(DecimalValue::integer(377_000))
    );
    assert_eq!(
        value("battery.current"),
        Some(DecimalValue::integer(-4_300))
    );
    assert_eq!(value("grid_relay"), Some(DecimalValue::integer(51)));
    assert_eq!(value("x.fan.speed"), DecimalValue::parse("1450.5"));

    // Every key of the smalog profile is fed by a member, and every member
    // other than an extra value lands on a key of the profile.
    let Some(profile) = source_profile("smalog") else {
        panic!("smalog profile missing");
    };
    let mut fed: Vec<&str> = readings
        .iter()
        .filter_map(|(key, _)| Some(device_channel(key)?.definition.key))
        .collect();
    fed.sort_unstable();
    fed.dedup();
    let mut defined: Vec<&str> = profile.channel_keys.iter().map(|key| key.key).collect();
    defined.sort_unstable();
    assert_eq!(fed, defined);
    assert_eq!(
        readings
            .iter()
            .filter(|(key, _)| device_channel(key).is_none())
            .map(|(key, _)| key.as_str())
            .collect::<Vec<_>>(),
        ["inv.2000123456.x.fan.speed"]
    );

    // A value the device did not report produces no entry.
    let sparse = request(body(&json!({
        "measuredAt": 1,
        "devices": [{ "id": "7", "ac": [{ "phase": 2, "voltageMv": 230_000 }] }],
    })));
    assert_eq!(
        device_channel_readings(&sparse.measurements[0].devices[0])
            .into_iter()
            .map(|(key, _)| key)
            .collect::<Vec<_>>(),
        ["inv.7.ac.l2.voltage"]
    );
}

#[test]
fn device_totals_leave_missing_what_no_device_reported() {
    let parsed = request(body(&json!({
        "measuredAt": 1,
        "devices": [
            full_device(),
            { "id": "2", "ac": [{ "phase": 1, "powerW": 600 }, { "phase": 2, "currentMa": 1 }] },
            { "id": "3", "energyLifetimeWh": 5 },
        ],
    })));
    let totals = device_totals(&parsed.measurements[0].devices);
    assert_eq!(totals.generation_power_w, Some(800 + 790 + 810 + 600));
    assert_eq!(totals.grid_import_power_w, Some(0));
    assert_eq!(totals.grid_export_power_w, Some(1_900));
    assert_eq!(totals.battery_state_basis_points, Some(8_150));

    let silent = request(body(
        &json!({ "measuredAt": 1, "devices": [{ "id": "3" }] }),
    ));
    let totals = device_totals(&silent.measurements[0].devices);
    assert_eq!(totals.generation_power_w, None);
    assert_eq!(totals.grid_import_power_w, None);
    assert_eq!(totals.battery_state_basis_points, None);
}

fn counters(values: &[(&str, i64)]) -> LifetimeCounters {
    values
        .iter()
        .map(|(device, value)| ((*device).to_owned(), *value))
        .collect()
}

#[test]
fn interval_energy_needs_the_same_devices_and_growing_counters() {
    let before = counters(&[("roof", 10_000), ("garage", 2_000)]);
    let derived = |current: &[(&str, i64)], previous: Option<&LifetimeCounters>| {
        derive_generation(&counters(current), previous)
    };
    // Nothing to derive from, and nothing before.
    assert_eq!(derived(&[], Some(&before)), DerivedGeneration::default());
    assert_eq!(
        derived(&[("roof", 10_000), ("garage", 2_000)], None),
        DerivedGeneration {
            lifetime_wh: Some(12_000),
            energy_wh: None,
            counter_reset: false
        }
    );
    // Both counters grew.
    assert_eq!(
        derived(&[("roof", 10_080), ("garage", 2_040)], Some(&before)),
        DerivedGeneration {
            lifetime_wh: Some(12_120),
            energy_wh: Some(120),
            counter_reset: false
        }
    );
    // A silent device: neither a sum that would look like a reset nor energy.
    assert_eq!(
        derived(&[("roof", 10_080)], Some(&before)),
        DerivedGeneration::default()
    );
    // A new device: a new sum to continue from, but no energy across the change.
    assert_eq!(
        derived(
            &[("roof", 10_080), ("garage", 2_040), ("shed", 7)],
            Some(&before)
        ),
        DerivedGeneration {
            lifetime_wh: Some(12_127),
            energy_wh: None,
            counter_reset: false
        }
    );
    // A replaced inverter starts at zero: no energy across the reset.
    assert_eq!(
        derived(&[("roof", 10_080), ("garage", 3)], Some(&before)),
        DerivedGeneration {
            lifetime_wh: Some(10_083),
            energy_wh: None,
            counter_reset: true
        }
    );
}
