//! A new system's timezone comes from the country its location lies in.

use pvlog_application::timezone_for_country;

const MUNICH: (i32, i32) = (48_137_000, 11_575_000);
const SAN_FRANCISCO: (i32, i32) = (37_775_000, -122_419_000);
const PHOENIX: (i32, i32) = (33_448_000, -112_074_000);
const NEW_YORK: (i32, i32) = (40_713_000, -74_006_000);
const PERTH: (i32, i32) = (-31_950_000, 115_860_000);

#[test]
fn a_country_with_one_timezone_needs_no_position() {
    assert_eq!(
        timezone_for_country("AT", None, None).as_deref(),
        Some("Europe/Vienna")
    );
    assert_eq!(
        timezone_for_country("at", None, Some("America/New_York")).as_deref(),
        Some("Europe/Vienna"),
        "a client elsewhere does not override the country"
    );
}

#[test]
fn an_exclave_zone_does_not_rename_the_mainland() {
    // Busingen's reference position is closer to Munich than Berlin's.
    assert_eq!(
        timezone_for_country("DE", Some(MUNICH), None).as_deref(),
        Some("Europe/Berlin")
    );
}

#[test]
fn the_nearest_timezone_decides_in_a_country_with_several() {
    assert_eq!(
        timezone_for_country("US", Some(SAN_FRANCISCO), Some("Europe/Berlin")).as_deref(),
        Some("America/Los_Angeles")
    );
    assert_eq!(
        timezone_for_country("US", Some(PHOENIX), None).as_deref(),
        Some("America/Phoenix"),
        "a zone without daylight saving keeps its own name"
    );
    assert_eq!(
        timezone_for_country("AU", Some(PERTH), None).as_deref(),
        Some("Australia/Perth")
    );
}

#[test]
fn the_clients_timezone_names_the_result_only_when_it_keeps_the_same_clock() {
    assert_eq!(
        timezone_for_country("US", Some(SAN_FRANCISCO), Some("America/Chicago")).as_deref(),
        Some("America/Los_Angeles"),
        "a client in another zone of the country does not move the plant"
    );
    assert_eq!(
        timezone_for_country("US", Some(NEW_YORK), Some("America/Detroit")).as_deref(),
        Some("America/Detroit"),
        "an equivalent client zone is kept as the owner knows it"
    );
    assert_eq!(
        timezone_for_country("US", None, Some("America/Chicago")).as_deref(),
        Some("America/Chicago")
    );
}

#[test]
fn without_a_position_the_most_populous_timezone_is_assumed() {
    assert_eq!(
        timezone_for_country("US", None, None).as_deref(),
        Some("America/New_York")
    );
}

#[test]
fn an_unknown_country_yields_nothing() {
    assert_eq!(timezone_for_country("ZZ", Some(MUNICH), None), None);
}
