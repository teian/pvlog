//! Deterministic per-string yield model.
//!
//! Irradiance on the plane of the array is turned into AC energy through a
//! fixed chain: the array's rated capacity scaled by irradiance against
//! standard test conditions, reduced by a temperature derate, then by the
//! configured optical and electrical losses, then by the operator's
//! calibration. Every step is integer arithmetic so a rebuild of the same
//! inputs reproduces the same numbers exactly.

/// Irradiance at standard test conditions, in watts per square metre.
const STANDARD_IRRADIANCE: i64 = 1_000;
/// Cell temperature the module rating is quoted at, in thousandths of a degree.
const RATING_TEMPERATURE_MILLI_C: i64 = 25_000;
/// Power lost per degree above the rating temperature, in basis points.
const TEMPERATURE_COEFFICIENT_BASIS_POINTS: i64 = 38;
/// Cell temperature rises above ambient roughly in proportion to irradiance.
const CELL_RISE_MILLI_C_PER_WM2: i64 = 30;

/// Loss chain for one string, each value in basis points.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LossChain {
    pub soiling: u16,
    pub shading: u16,
    pub mismatch: u16,
    pub wiring: u16,
    pub unavailability: u16,
    /// Signed operator correction applied last; positive raises the estimate.
    pub calibration: i32,
}

/// Weather inputs for one interval.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IntervalWeather {
    /// Plane-of-array irradiance in watts per square metre.
    pub plane_of_array_wm2: i64,
    pub ambient_temperature_milli_c: Option<i64>,
}

/// Modeled power for one string over one interval.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ModeledInterval {
    pub power_watts: i64,
    pub energy_wh: i64,
}

/// Models one interval for a string of the given rated capacity.
///
/// `rated_power_watts` is the array's nameplate output at standard conditions.
/// Losses compound rather than add, because each stage acts on what the
/// previous stage left.
#[must_use]
pub fn model_interval(
    rated_power_watts: i64,
    losses: LossChain,
    weather: IntervalWeather,
    interval_millis: i64,
) -> ModeledInterval {
    if rated_power_watts <= 0 || weather.plane_of_array_wm2 <= 0 || interval_millis <= 0 {
        return ModeledInterval::default();
    }
    let irradiance =
        rated_power_watts.saturating_mul(weather.plane_of_array_wm2) / STANDARD_IRRADIANCE;
    let derated = apply_temperature(irradiance, weather);
    let after_losses = [
        losses.soiling,
        losses.shading,
        losses.mismatch,
        losses.wiring,
        losses.unavailability,
    ]
    .into_iter()
    .fold(derated, |power, loss| {
        power.saturating_mul(10_000 - i64::from(loss.min(10_000))) / 10_000
    });
    let calibrated = after_losses
        .saturating_mul(10_000 + i64::from(losses.calibration))
        .max(0)
        / 10_000;
    ModeledInterval {
        power_watts: calibrated,
        energy_wh: calibrated.saturating_mul(interval_millis) / 3_600_000,
    }
}

/// Reduces output for cell temperatures above the module's rating point.
///
/// Without an ambient reading the derate is skipped rather than guessed, so a
/// missing measurement never silently lowers a forecast.
fn apply_temperature(power_watts: i64, weather: IntervalWeather) -> i64 {
    let Some(ambient) = weather.ambient_temperature_milli_c else {
        return power_watts;
    };
    let cell = ambient + weather.plane_of_array_wm2 * CELL_RISE_MILLI_C_PER_WM2 / 100;
    let excess = cell - RATING_TEMPERATURE_MILLI_C;
    if excess <= 0 {
        return power_watts;
    }
    let derate = excess * TEMPERATURE_COEFFICIENT_BASIS_POINTS / 1_000;
    power_watts.saturating_mul((10_000 - derate).max(0)) / 10_000
}
