//! Moved out of the crate: this repository keeps Rust tests in tests/.

use pvlog::yield_model::{IntervalWeather, LossChain, ModeledInterval, model_interval};

const HOUR: i64 = 3_600_000;

fn clear_sky(irradiance: i64) -> IntervalWeather {
    IntervalWeather {
        plane_of_array_wm2: irradiance,
        ambient_temperature_milli_c: None,
    }
}

#[test]
fn standard_conditions_without_losses_produce_the_rated_output() {
    let modeled = model_interval(5_000, LossChain::default(), clear_sky(1_000), HOUR);
    assert_eq!(
        modeled,
        ModeledInterval {
            power_watts: 5_000,
            energy_wh: 5_000,
        }
    );
}

#[test]
fn losses_compound_rather_than_add() {
    // Two ten-percent losses leave 81 percent, not 80.
    let losses = LossChain {
        soiling: 1_000,
        shading: 1_000,
        ..LossChain::default()
    };
    let modeled = model_interval(10_000, losses, clear_sky(1_000), HOUR);
    assert_eq!(modeled.power_watts, 8_100);
}

#[test]
fn a_warm_cell_reduces_output_and_a_missing_reading_does_not() {
    let warm = IntervalWeather {
        plane_of_array_wm2: 1_000,
        ambient_temperature_milli_c: Some(30_000),
    };
    let warm_output = model_interval(5_000, LossChain::default(), warm, HOUR).power_watts;
    let unknown_output =
        model_interval(5_000, LossChain::default(), clear_sky(1_000), HOUR).power_watts;
    assert!(
        warm_output < unknown_output,
        "a hot cell must produce less than the rating point"
    );
    assert_eq!(
        unknown_output, 5_000,
        "a missing ambient reading must not derate"
    );
}

#[test]
fn darkness_and_invalid_input_produce_nothing() {
    assert_eq!(
        model_interval(5_000, LossChain::default(), clear_sky(0), HOUR),
        ModeledInterval::default()
    );
    assert_eq!(
        model_interval(0, LossChain::default(), clear_sky(1_000), HOUR),
        ModeledInterval::default()
    );
}

#[test]
fn calibration_moves_the_estimate_in_both_directions() {
    let raise = LossChain {
        calibration: 500,
        ..LossChain::default()
    };
    let lower = LossChain {
        calibration: -500,
        ..LossChain::default()
    };
    assert_eq!(
        model_interval(10_000, raise, clear_sky(1_000), HOUR).power_watts,
        10_500
    );
    assert_eq!(
        model_interval(10_000, lower, clear_sky(1_000), HOUR).power_watts,
        9_500
    );
}
