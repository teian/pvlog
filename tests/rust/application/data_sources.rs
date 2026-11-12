//! Connection health and uploader configuration of data sources.

use pvlog_application::{
    ChannelAggregation, ChannelReading, ChannelValueType, ConnectionState, DeviceChannelTarget,
    SOURCE_PROFILES, channel_typing_for, connection_state, device_channel, render_configuration,
    source_profile, status_code_label,
};

const NOW: i64 = 1_800_000_000_000;
const INTERVAL_SECONDS: u32 = 300;
const THREE_INTERVALS_MILLIS: i64 = 900_000;

#[test]
fn a_source_that_never_delivered_is_not_connected() {
    assert_eq!(
        connection_state(true, None, INTERVAL_SECONDS, NOW),
        ConnectionState::NeverConnected
    );
}

#[test]
fn a_source_is_stale_after_more_than_three_status_intervals() {
    let received = |age: i64| connection_state(true, Some(NOW - age), INTERVAL_SECONDS, NOW);
    assert_eq!(received(0), ConnectionState::Receiving);
    assert_eq!(received(THREE_INTERVALS_MILLIS), ConnectionState::Receiving);
    assert_eq!(received(THREE_INTERVALS_MILLIS + 1), ConnectionState::Stale);
}

#[test]
fn staleness_follows_the_status_interval_of_the_system() {
    // One hour between readings: twenty minutes of silence is normal.
    assert_eq!(
        connection_state(true, Some(NOW - 1_200_000), 3_600, NOW),
        ConnectionState::Receiving
    );
}

#[test]
fn a_source_without_a_usable_credential_is_disconnected() {
    assert_eq!(
        connection_state(false, Some(NOW), INTERVAL_SECONDS, NOW),
        ConnectionState::Disconnected,
        "fresh readings do not hide a revoked credential"
    );
    assert_eq!(
        connection_state(false, None, INTERVAL_SECONDS, NOW),
        ConnectionState::Disconnected
    );
}

#[test]
fn the_profile_catalog_offers_smalog_first() {
    let ids: Vec<&str> = SOURCE_PROFILES.iter().map(|profile| profile.id).collect();
    assert_eq!(ids, ["smalog", "sbfspot", "generic", "import"]);
    assert!(source_profile("solar-wizard").is_none());
    assert!(
        source_profile("import").is_some_and(|profile| !profile.issues_credential),
        "an import needs no upload credential"
    );
}

#[test]
fn every_profile_renders_the_snippets_it_announces() {
    for profile in SOURCE_PROFILES {
        let kinds: Vec<&str> = render_configuration(profile, "https://pv.example", "sys", "key")
            .iter()
            .map(|snippet| snippet.id)
            .collect();
        assert_eq!(kinds, profile.snippet_kinds, "{}", profile.id);
    }
}

#[test]
fn the_smalog_file_refers_to_the_key_and_the_environment_carries_it() {
    let Some(profile) = source_profile("smalog") else {
        panic!("smalog profile missing");
    };
    let snippets =
        render_configuration(profile, "https://pv.example", "0190-system", "pvlog_secret");
    assert_eq!(
        snippets[0].content,
        "[pvlog]\nenabled = true\nurl = \"https://pv.example\"\n\
         system_id = \"0190-system\"\napi_key = \"${PVLOG_API_KEY}\"\n"
    );
    assert_eq!(snippets[1].content, "PVLOG_API_KEY=pvlog_secret\n");
    let lines_with_key: Vec<&str> = snippets
        .iter()
        .flat_map(|snippet| snippet.content.lines())
        .filter(|line| line.contains("pvlog_secret"))
        .collect();
    assert_eq!(
        lines_with_key,
        ["PVLOG_API_KEY=pvlog_secret"],
        "the cleartext key appears on the environment line only"
    );
}

/// The key scheme as the design documents it: measurement, unit, target kind
/// and aggregation. Every value is a whole number in the logger's unit.
const DOCUMENTED_SMALOG_KEYS: &[(&str, &str, &str, ChannelAggregation)] = {
    use ChannelAggregation::{CounterDelta, Last, Max, Mean};
    &[
        ("ac.l{phase}.power", "W", "inverter", Mean),
        ("ac.l{phase}.current", "mA", "inverter", Mean),
        ("ac.l{phase}.voltage", "mV", "inverter", Mean),
        ("grid.frequency", "mHz", "inverter", Mean),
        ("grid.import_power", "W", "inverter", Mean),
        ("grid.export_power", "W", "inverter", Mean),
        ("energy.today", "Wh", "inverter", Last),
        ("energy.lifetime", "Wh", "inverter", CounterDelta),
        ("operating_time", "s", "inverter", Last),
        ("feed_in_time", "s", "inverter", Last),
        ("status", "code", "inverter", Last),
        ("grid_relay", "code", "inverter", Last),
        ("temperature", "m°C", "inverter", Max),
        ("signal", "‰", "inverter", Mean),
        ("mppt.{tracker}.power", "W", "string", Mean),
        ("mppt.{tracker}.current", "mA", "string", Mean),
        ("mppt.{tracker}.voltage", "mV", "string", Mean),
        ("battery.soc", "‰", "inverter", Last),
        ("battery.voltage", "mV", "inverter", Mean),
        ("battery.current", "mA", "inverter", Mean),
        ("battery.temperature", "m°C", "inverter", Mean),
    ]
};

/// Concrete keys a pattern stands for: every phase, and the first, a sparse
/// and the last tracker number.
fn concrete_keys(pattern: &str) -> Vec<String> {
    let key = pattern.replace("{serial}", "2000123456");
    if key.contains("{phase}") {
        ["1", "2", "3"]
            .iter()
            .map(|phase| key.replace("{phase}", phase))
            .collect()
    } else if key.contains("{tracker}") {
        ["1", "5", "255"]
            .iter()
            .map(|tracker| key.replace("{tracker}", tracker))
            .collect()
    } else {
        vec![key]
    }
}

#[test]
fn the_smalog_profile_defines_exactly_the_documented_keys() {
    let Some(profile) = source_profile("smalog") else {
        panic!("smalog profile missing");
    };
    let mut defined: Vec<&str> = profile.channel_keys.iter().map(|key| key.key).collect();
    let mut documented: Vec<String> = DOCUMENTED_SMALOG_KEYS
        .iter()
        .map(|(measurement, ..)| format!("inv.{{serial}}.{measurement}"))
        .collect();
    defined.sort_unstable();
    documented.sort_unstable();
    assert_eq!(defined, documented);
}

#[test]
fn every_smalog_key_registers_with_its_documented_definition() {
    let Some(profile) = source_profile("smalog") else {
        panic!("smalog profile missing");
    };
    // What a source claims about a key of the scheme does not matter.
    let misleading = ChannelReading {
        power_watts: Some(1),
        ..ChannelReading::default()
    };
    for (measurement, unit, target_kind, aggregation) in DOCUMENTED_SMALOG_KEYS {
        let pattern = format!("inv.{{serial}}.{measurement}");
        let Some(definition) = profile.channel_keys.iter().find(|key| key.key == pattern) else {
            panic!("{pattern} is not part of the profile");
        };
        assert_eq!(definition.unit, *unit, "{pattern}");
        assert_eq!(definition.scale, 0, "{pattern}");
        assert_eq!(definition.target_kind, *target_kind, "{pattern}");
        assert_eq!(definition.aggregation, *aggregation, "{pattern}");
        let counter = *measurement == "energy.lifetime";
        assert_eq!(
            definition.value_type,
            if counter {
                ChannelValueType::Counter
            } else {
                ChannelValueType::Integer
            },
            "{pattern}"
        );
        for key in concrete_keys(&pattern) {
            let typing = channel_typing_for(&key, &misleading);
            assert_eq!(typing.unit, *unit, "{key}");
            assert_eq!(typing.scale, 0, "{key}");
            assert_eq!(typing.aggregation, *aggregation, "{key}");
            assert_eq!(typing.value_type, definition.value_type, "{key}");
            let Some(device) = device_channel(&key) else {
                panic!("{key} belongs to the key scheme");
            };
            assert_eq!(device.definition, definition, "{key}");
            assert_eq!(
                matches!(device.target, DeviceChannelTarget::Tracker { .. }),
                *target_kind == "string",
                "{key}"
            );
        }
    }
}

#[test]
fn the_sbfspot_profile_shares_the_keys_sbfspot_records() {
    let (Some(smalog), Some(sbfspot)) = (source_profile("smalog"), source_profile("sbfspot"))
    else {
        panic!("profiles missing");
    };
    let only_smalog: Vec<&str> = smalog
        .channel_keys
        .iter()
        .filter(|key| !sbfspot.channel_keys.contains(key))
        .map(|key| key.key)
        .collect();
    assert_eq!(
        only_smalog,
        [
            "inv.{serial}.grid.import_power",
            "inv.{serial}.grid.export_power",
            "inv.{serial}.battery.soc",
            "inv.{serial}.battery.voltage",
            "inv.{serial}.battery.current",
            "inv.{serial}.battery.temperature",
        ]
    );
    assert!(
        sbfspot
            .channel_keys
            .iter()
            .all(|key| smalog.channel_keys.contains(key)),
        "a system that moves from SBFspot to smalog keeps its channels"
    );
    for profile in ["generic", "import"] {
        assert!(
            source_profile(profile).is_some_and(|profile| profile.channel_keys.is_empty()),
            "{profile}"
        );
    }
}

#[test]
fn status_codes_are_labelled_from_the_profile() {
    let Some(profile) = source_profile("smalog") else {
        panic!("smalog profile missing");
    };
    let labelled: Vec<&str> = profile
        .channel_keys
        .iter()
        .filter(|key| !key.status_labels.is_empty())
        .map(|key| key.key)
        .collect();
    assert_eq!(labelled, ["inv.{serial}.status", "inv.{serial}.grid_relay"]);

    let status = |code| status_code_label("inv.2000123456.status", code);
    assert_eq!(status(307), Some("Ok"));
    assert_eq!(status(455), Some("Warning"));
    assert_eq!(status(35), Some("Fault"));
    assert_eq!(status(303), Some("Off"));
    assert_eq!(status(16_777_213), Some("Information not available"));
    assert_eq!(status(51), None, "a relay state is no device status");
    assert_eq!(status(9_999), None, "an unknown code stays a number");

    let relay = |code| status_code_label("inv.2000123456.grid_relay", code);
    assert_eq!(relay(51), Some("Closed"));
    assert_eq!(relay(311), Some("Open"));
    assert_eq!(status_code_label("inv.2000123456.temperature", 307), None);
    assert_eq!(status_code_label("meter.status", 307), None);
}

#[test]
fn the_sbfspot_environment_and_the_generic_request_are_complete() {
    let (Some(sbfspot), Some(generic)) = (source_profile("sbfspot"), source_profile("generic"))
    else {
        panic!("profiles missing");
    };
    let environment = &render_configuration(sbfspot, "https://pv.example", "sys-1", "k-1")[0];
    for line in [
        "PVLOG_URL=https://pv.example",
        "PVLOG_SYSTEM_ID=sys-1",
        "PVLOG_API_KEY=k-1",
    ] {
        assert!(environment.content.contains(line), "{line}");
    }
    let curl = &render_configuration(generic, "https://pv.example", "sys-1", "k-1")[0];
    assert!(
        curl.content
            .contains("https://pv.example/api/v1/systems/sys-1/observations")
    );
    assert!(curl.content.contains("Authorization: Bearer k-1"));
    assert!(curl.content.contains("Idempotency-Key"));
}
