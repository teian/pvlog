//! Connection health and uploader configuration of data sources.

use pvlog_application::{
    ConnectionState, SOURCE_PROFILES, connection_state, render_configuration, source_profile,
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
