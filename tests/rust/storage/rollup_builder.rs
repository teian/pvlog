#![allow(clippy::expect_used)]

use pvlog_storage::{RollupGranularity, RollupSample, RollupWindow, build_rollups};

#[test]
fn every_rollup_granularity_computes_complete_statistics() {
    for granularity in [
        RollupGranularity::FifteenMinutes,
        RollupGranularity::Hourly,
        RollupGranularity::Daily,
        RollupGranularity::Monthly,
        RollupGranularity::Yearly,
    ] {
        let window = RollupWindow {
            granularity,
            start_epoch_millis: 0,
            end_epoch_millis: 1_000,
            local_label: format!("{granularity:?}"),
        };
        let rollups = build_rollups(
            granularity,
            &[
                RollupSample {
                    timestamp_epoch_millis: 100,
                    value: 4,
                    covered_millis: 400,
                },
                RollupSample {
                    timestamp_epoch_millis: 700,
                    value: -2,
                    covered_millis: 350,
                },
            ],
            &[window],
        )
        .expect("valid rollup");
        let rollup = &rollups[0];
        assert_eq!((rollup.sum, rollup.min, rollup.max), (2, -2, 4));
        assert_eq!((rollup.count, rollup.first, rollup.last), (2, 4, -2));
        assert_eq!(
            (rollup.covered_millis, rollup.coverage_basis_points),
            (750, 7_500)
        );
    }
}

#[test]
fn timezone_resolved_daily_windows_handle_spring_and_fall_dst() {
    const HOUR: i64 = 3_600_000;
    let spring = RollupWindow {
        granularity: RollupGranularity::Daily,
        start_epoch_millis: 0,
        end_epoch_millis: 23 * HOUR,
        local_label: "2026-03-29 Europe/Berlin".into(),
    };
    let fall = RollupWindow {
        granularity: RollupGranularity::Daily,
        start_epoch_millis: 30 * HOUR,
        end_epoch_millis: 55 * HOUR,
        local_label: "2026-10-25 Europe/Berlin".into(),
    };
    let spring_result = build_rollups(
        RollupGranularity::Daily,
        &[RollupSample {
            timestamp_epoch_millis: HOUR,
            value: 1,
            covered_millis: u64::try_from(23 * HOUR).expect("positive"),
        }],
        &[spring],
    )
    .expect("23-hour local day");
    let fall_result = build_rollups(
        RollupGranularity::Daily,
        &[RollupSample {
            timestamp_epoch_millis: 31 * HOUR,
            value: 1,
            covered_millis: u64::try_from(25 * HOUR).expect("positive"),
        }],
        &[fall],
    )
    .expect("25-hour local day");
    assert_eq!(spring_result[0].coverage_basis_points, 10_000);
    assert_eq!(fall_result[0].coverage_basis_points, 10_000);
}
