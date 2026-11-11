//! Moved out of the crate: this repository keeps Rust tests in tests/.

use pvlog::sharing::public_location;
use pvlog_api::PublicSystem;

/// Everything an anonymous visitor is allowed to see, and nothing else.
///
/// Pinning the field set means a later addition to the response has to pass
/// through this test, rather than quietly widening what a public link
/// exposes.
#[test]
fn the_public_view_carries_only_the_agreed_fields() {
    let view = PublicSystem {
        name: "Testanlage".to_owned(),
        location: None,
        commissioning_date: None,
        capacity_watts: Some(19_520),
        timezone: "Europe/Berlin".to_owned(),
        generation_today_wh: Some(609),
        generation_total_wh: Some(27_898_674),
        last_seen_epoch_millis: Some(1_788_022_485_000),
    };
    let Ok(encoded) = serde_json::to_value(&view) else {
        panic!("the public view must serialize");
    };
    let Some(object) = encoded.as_object() else {
        panic!("the public view must serialize to an object");
    };
    let mut fields: Vec<&str> = object.keys().map(String::as_str).collect();
    fields.sort_unstable();
    assert_eq!(
        fields,
        [
            "capacityWatts",
            "commissioningDate",
            "generationTodayWh",
            "generationTotalWh",
            "lastSeenEpochMillis",
            "location",
            "name",
            "timezone",
        ]
    );
}

#[test]
fn a_hidden_placement_reveals_nothing() {
    assert_eq!(
        public_location(
            "hidden",
            Some("Wohnhaus Dach".to_owned()),
            Some("DE".to_owned())
        ),
        None
    );
}

#[test]
fn a_country_placement_reveals_only_the_country() {
    assert_eq!(
        public_location(
            "country",
            Some("Wohnhaus Dach".to_owned()),
            Some("DE".to_owned())
        ),
        Some("DE".to_owned())
    );
}

#[test]
fn a_blank_description_is_omitted_rather_than_sent_empty() {
    assert_eq!(
        public_location("locality", Some("   ".to_owned()), None),
        None
    );
}
