#![allow(dead_code)]

#[path = "../../../src/crates/pvlog/src/reporting.rs"]
mod reporting;

use pvlog_domain::SystemId;

#[test]
fn seasonal_report_groups_daily_energy_and_ignores_missing_values() {
    let response = reporting::seasonal_response(
        SystemId::new(),
        &[
            ("2026-01-10".to_owned(), Some(1_000)),
            ("2026-02-10".to_owned(), Some(3_000)),
            ("2026-04-10".to_owned(), Some(5_000)),
            ("2026-07-10".to_owned(), None),
        ],
    );

    assert_eq!(response.seasons[0].generation_energy_wh, 4_000);
    assert_eq!(response.seasons[0].measured_days, 2);
    assert_eq!(response.seasons[0].average_daily_energy_wh, 2_000);
    assert_eq!(response.seasons[1].generation_energy_wh, 5_000);
    assert_eq!(response.seasons[2].measured_days, 0);
}
