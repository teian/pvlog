//! Deterministic version-1 PV yield model primitives.

use crate::{
    BasisPoints, EstimateRange, ForecastCompleteness, ForecastCompletenessReason,
    ForecastLossFactors, GeographicPoint, InverterId, IrradiancePoint, MilliDegreesCelsius,
    StringId, SystemId, UnsignedBasisPoints, UtcTimestamp, Watts, WattsPerSquareMetre,
};
use std::f64::consts::PI;
use thiserror::Error;

pub const YIELD_MODEL_V1_IDENTIFIER: &str = "pv-yield-v1";
pub const YIELD_MODEL_V1_REVISION: u16 = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SurfaceOrientation {
    pub azimuth_degrees: u16,
    pub tilt_degrees: u8,
}

impl SurfaceOrientation {
    /// Creates validated surface geometry.
    ///
    /// # Errors
    ///
    /// Returns [`YieldModelError::InvalidSurfaceOrientation`] for invalid azimuth or tilt.
    pub fn new(azimuth_degrees: u16, tilt_degrees: u8) -> Result<Self, YieldModelError> {
        if azimuth_degrees > 359 || tilt_degrees > 90 {
            Err(YieldModelError::InvalidSurfaceOrientation)
        } else {
            Ok(Self {
                azimuth_degrees,
                tilt_degrees,
            })
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SolarPosition {
    pub zenith_millidegrees: u32,
    pub azimuth_millidegrees: u32,
}

impl SolarPosition {
    #[must_use]
    pub const fn above_horizon(self) -> bool {
        self.zenith_millidegrees < 90_000
    }
}

/// Computes the NOAA fractional-year solar position, rounded to 0.001 degree.
#[must_use]
pub fn solar_position(location: GeographicPoint, at: UtcTimestamp) -> SolarPosition {
    let datetime = at.as_datetime();
    let day = f64::from(datetime.ordinal());
    let hour = f64::from(datetime.hour())
        + f64::from(datetime.minute()) / 60.0
        + f64::from(datetime.second()) / 3_600.0;
    let gamma = 2.0 * PI / 365.0 * (day - 1.0 + (hour - 12.0) / 24.0);
    let equation_of_time = 229.18
        * (0.000_075 + 0.001_868 * gamma.cos()
            - 0.032_077 * gamma.sin()
            - 0.014_615 * (2.0 * gamma).cos()
            - 0.040_849 * (2.0 * gamma).sin());
    let declination = 0.006_918 - 0.399_912 * gamma.cos() + 0.070_257 * gamma.sin()
        - 0.006_758 * (2.0 * gamma).cos()
        + 0.000_907 * (2.0 * gamma).sin()
        - 0.002_697 * (3.0 * gamma).cos()
        + 0.001_48 * (3.0 * gamma).sin();
    let longitude = f64::from(location.longitude_microdegrees) / 1_000_000.0;
    let solar_minutes = (hour * 60.0 + equation_of_time + 4.0 * longitude).rem_euclid(1_440.0);
    let hour_angle = (solar_minutes / 4.0 - 180.0).to_radians();
    let latitude = (f64::from(location.latitude_microdegrees) / 1_000_000.0).to_radians();
    let cos_zenith = (latitude.sin() * declination.sin()
        + latitude.cos() * declination.cos() * hour_angle.cos())
    .clamp(-1.0, 1.0);
    let zenith = cos_zenith.acos();
    let azimuth = hour_angle
        .sin()
        .atan2(hour_angle.cos() * latitude.sin() - declination.tan() * latitude.cos())
        .to_degrees()
        + 180.0;
    SolarPosition {
        zenith_millidegrees: rounded_millidegrees(zenith.to_degrees(), 180_000),
        azimuth_millidegrees: rounded_millidegrees(azimuth.rem_euclid(360.0), 359_999),
    }
}

/// Applies v1 isotropic-sky transposition with fixed 20% ground albedo.
///
/// # Errors
///
/// Returns [`YieldModelError::MissingTranspositionInput`] when provider plane-of-array data is
/// absent and GHI, DNI, or DHI is missing.
pub fn plane_of_array_irradiance(
    irradiance: IrradiancePoint,
    solar: SolarPosition,
    surface: SurfaceOrientation,
) -> Result<EstimateRange<WattsPerSquareMetre>, YieldModelError> {
    if let Some(value) = irradiance.plane_of_array {
        return Ok(value);
    }
    let global = irradiance
        .global_horizontal
        .ok_or(YieldModelError::MissingTranspositionInput)?;
    let direct = irradiance
        .direct_normal
        .ok_or(YieldModelError::MissingTranspositionInput)?;
    let diffuse = irradiance
        .diffuse_horizontal
        .ok_or(YieldModelError::MissingTranspositionInput)?;
    Ok(EstimateRange {
        central: transpose_value(
            global.central,
            direct.central,
            diffuse.central,
            solar,
            surface,
        ),
        lower: match (global.lower, direct.lower, diffuse.lower) {
            (Some(g), Some(d), Some(h)) => Some(transpose_value(g, d, h, solar, surface)),
            _ => None,
        },
        upper: match (global.upper, direct.upper, diffuse.upper) {
            (Some(g), Some(d), Some(h)) => Some(transpose_value(g, d, h, solar, surface)),
            _ => None,
        },
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StringDcInput {
    pub nameplate_power: Watts,
    pub plane_of_array: EstimateRange<WattsPerSquareMetre>,
    pub ambient_temperature: MilliDegreesCelsius,
    pub peak_power_temperature_coefficient_ppm_per_celsius: Option<i32>,
    pub losses: ForecastLossFactors,
    pub calibration: BasisPoints,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StringDcEstimate {
    pub module_temperature: EstimateRange<MilliDegreesCelsius>,
    pub power: EstimateRange<Watts>,
    pub was_physically_capped: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StringYieldContribution {
    pub string_id: StringId,
    pub nameplate_power: Watts,
    pub power: Option<EstimateRange<Watts>>,
    pub unavailable_reasons: Vec<ForecastCompletenessReason>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InverterYieldEstimate {
    pub inverter_id: InverterId,
    pub power: Option<EstimateRange<Watts>>,
    pub included_capacity: Watts,
    pub total_effective_capacity: Watts,
    pub completeness: ForecastCompleteness,
    pub clipped: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SystemYieldEstimate {
    pub system_id: SystemId,
    pub power: Option<EstimateRange<Watts>>,
    pub included_capacity: Watts,
    pub total_effective_capacity: Watts,
    pub completeness: ForecastCompleteness,
}

/// Aggregates string DC, applies inverter efficiency, and clips AC output.
///
/// # Errors
///
/// Returns an error for invalid capacity or arithmetic overflow.
pub fn aggregate_inverter_yield(
    inverter_id: InverterId,
    strings: &[StringYieldContribution],
    efficiency: UnsignedBasisPoints,
    maximum_ac_power: Watts,
) -> Result<InverterYieldEstimate, YieldModelError> {
    if maximum_ac_power.value() <= 0 {
        return Err(YieldModelError::InvalidInverterCapacity);
    }
    let mut total_capacity = 0_i64;
    let mut included_capacity = 0_i64;
    let mut power = EstimateAccumulator::default();
    let mut reasons = Vec::new();
    for string in strings {
        if string.nameplate_power.value() < 0 {
            return Err(YieldModelError::InvalidNameplatePower);
        }
        total_capacity = checked_add(total_capacity, string.nameplate_power.value())?;
        if let Some(estimate) = string.power {
            included_capacity = checked_add(included_capacity, string.nameplate_power.value())?;
            power.add(estimate)?;
        } else {
            reasons.extend(string.unavailable_reasons.iter().copied());
        }
    }
    if included_capacity == 0 {
        reasons.push(ForecastCompletenessReason::NoEffectiveEquipment);
        normalize_reasons(&mut reasons);
        return Ok(InverterYieldEstimate {
            inverter_id,
            power: None,
            included_capacity: Watts::new(0),
            total_effective_capacity: Watts::new(total_capacity),
            completeness: ForecastCompleteness::Unavailable { reasons },
            clipped: false,
        });
    }
    if included_capacity < total_capacity {
        reasons.push(ForecastCompletenessReason::PartialEffectiveCapacity);
    }
    normalize_reasons(&mut reasons);
    let convert = |value: i64| -> Result<Watts, YieldModelError> {
        Ok(Watts::new(
            checked_ratio(value, i64::from(efficiency.value()), 10_000)?
                .min(maximum_ac_power.value()),
        ))
    };
    let unclipped = checked_ratio(power.central, i64::from(efficiency.value()), 10_000)?;
    Ok(InverterYieldEstimate {
        inverter_id,
        power: Some(power.finish(convert)?),
        included_capacity: Watts::new(included_capacity),
        total_effective_capacity: Watts::new(total_capacity),
        completeness: completeness(reasons),
        clipped: unclipped > maximum_ac_power.value(),
    })
}

/// Aggregates inverter AC output while preserving included and excluded capacity.
///
/// # Errors
///
/// Returns an error on arithmetic overflow.
pub fn aggregate_system_yield(
    system_id: SystemId,
    inverters: &[InverterYieldEstimate],
) -> Result<SystemYieldEstimate, YieldModelError> {
    let mut total_capacity = 0_i64;
    let mut included_capacity = 0_i64;
    let mut power = EstimateAccumulator::default();
    let mut reasons = Vec::new();
    for inverter in inverters {
        total_capacity = checked_add(total_capacity, inverter.total_effective_capacity.value())?;
        included_capacity = checked_add(included_capacity, inverter.included_capacity.value())?;
        if let Some(estimate) = inverter.power {
            power.add(estimate)?;
        }
        match &inverter.completeness {
            ForecastCompleteness::Complete => {}
            ForecastCompleteness::Partial { reasons: item }
            | ForecastCompleteness::Unavailable { reasons: item } => reasons.extend(item),
        }
    }
    if included_capacity == 0 {
        reasons.push(ForecastCompletenessReason::NoEffectiveEquipment);
        normalize_reasons(&mut reasons);
        return Ok(SystemYieldEstimate {
            system_id,
            power: None,
            included_capacity: Watts::new(0),
            total_effective_capacity: Watts::new(total_capacity),
            completeness: ForecastCompleteness::Unavailable { reasons },
        });
    }
    if included_capacity < total_capacity {
        reasons.push(ForecastCompletenessReason::PartialEffectiveCapacity);
    }
    normalize_reasons(&mut reasons);
    Ok(SystemYieldEstimate {
        system_id,
        power: Some(power.finish(|value| Ok(Watts::new(value)))?),
        included_capacity: Watts::new(included_capacity),
        total_effective_capacity: Watts::new(total_capacity),
        completeness: completeness(reasons),
    })
}

struct EstimateAccumulator {
    central: i64,
    lower: Option<i64>,
    upper: Option<i64>,
}

impl Default for EstimateAccumulator {
    fn default() -> Self {
        Self {
            central: 0,
            lower: Some(0),
            upper: Some(0),
        }
    }
}

impl EstimateAccumulator {
    fn add(&mut self, estimate: EstimateRange<Watts>) -> Result<(), YieldModelError> {
        self.central = checked_add(self.central, estimate.central.value())?;
        self.lower = sum_optional(self.lower, estimate.lower)?;
        self.upper = sum_optional(self.upper, estimate.upper)?;
        Ok(())
    }

    fn finish(
        self,
        convert: impl Fn(i64) -> Result<Watts, YieldModelError>,
    ) -> Result<EstimateRange<Watts>, YieldModelError> {
        Ok(EstimateRange {
            central: convert(self.central)?,
            lower: self.lower.map(&convert).transpose()?,
            upper: self.upper.map(convert).transpose()?,
        })
    }
}

fn sum_optional(total: Option<i64>, value: Option<Watts>) -> Result<Option<i64>, YieldModelError> {
    match (total, value) {
        (Some(total), Some(value)) => Ok(Some(checked_add(total, value.value())?)),
        _ => Ok(None),
    }
}

fn checked_add(left: i64, right: i64) -> Result<i64, YieldModelError> {
    left.checked_add(right)
        .ok_or(YieldModelError::ArithmeticOverflow)
}

fn normalize_reasons(reasons: &mut Vec<ForecastCompletenessReason>) {
    reasons.sort_unstable();
    reasons.dedup();
}

fn completeness(reasons: Vec<ForecastCompletenessReason>) -> ForecastCompleteness {
    if reasons.is_empty() {
        ForecastCompleteness::Complete
    } else {
        ForecastCompleteness::Partial { reasons }
    }
}

/// Calculates v1 module temperature and string DC output with fixed-point loss application.
///
/// Module temperature uses ambient + 25 C at 800 W/m2. Loss factors are applied
/// multiplicatively, then calibration, and the result is capped at 125% of DC nameplate.
///
/// # Errors
///
/// Returns an error for non-positive nameplate capacity or arithmetic overflow.
pub fn calculate_string_dc(input: StringDcInput) -> Result<StringDcEstimate, YieldModelError> {
    if input.nameplate_power.value() <= 0 {
        return Err(YieldModelError::InvalidNameplatePower);
    }
    let module_temperature = EstimateRange {
        central: module_temperature(input.ambient_temperature, input.plane_of_array.central)?,
        lower: input
            .plane_of_array
            .lower
            .map(|value| module_temperature(input.ambient_temperature, value))
            .transpose()?,
        upper: input
            .plane_of_array
            .upper
            .map(|value| module_temperature(input.ambient_temperature, value))
            .transpose()?,
    };
    let maximum = checked_ratio(input.nameplate_power.value(), 12_500, 10_000)?;
    let calculate = |irradiance, temperature| {
        dc_value(
            input.nameplate_power,
            irradiance,
            temperature,
            input.peak_power_temperature_coefficient_ppm_per_celsius,
            input.losses,
            input.calibration,
            maximum,
        )
    };
    let central = calculate(input.plane_of_array.central, module_temperature.central)?;
    let lower = match (input.plane_of_array.lower, module_temperature.lower) {
        (Some(irradiance), Some(temperature)) => Some(calculate(irradiance, temperature)?),
        _ => None,
    };
    let upper = match (input.plane_of_array.upper, module_temperature.upper) {
        (Some(irradiance), Some(temperature)) => Some(calculate(irradiance, temperature)?),
        _ => None,
    };
    Ok(StringDcEstimate {
        was_physically_capped: central.value() == maximum
            || lower.is_some_and(|value| value.value() == maximum)
            || upper.is_some_and(|value| value.value() == maximum),
        module_temperature,
        power: EstimateRange {
            central,
            lower,
            upper,
        },
    })
}

fn module_temperature(
    ambient: MilliDegreesCelsius,
    irradiance: WattsPerSquareMetre,
) -> Result<MilliDegreesCelsius, YieldModelError> {
    let rise = i64::from(irradiance.value())
        .checked_mul(25_000)
        .ok_or(YieldModelError::ArithmeticOverflow)?
        / 800;
    let value = i64::from(ambient.value())
        .checked_add(rise)
        .ok_or(YieldModelError::ArithmeticOverflow)?;
    Ok(MilliDegreesCelsius::new(
        i32::try_from(value).map_err(|_| YieldModelError::ArithmeticOverflow)?,
    ))
}

#[allow(clippy::too_many_arguments)]
fn dc_value(
    nameplate: Watts,
    irradiance: WattsPerSquareMetre,
    temperature: MilliDegreesCelsius,
    coefficient_ppm: Option<i32>,
    losses: ForecastLossFactors,
    calibration: BasisPoints,
    maximum: i64,
) -> Result<Watts, YieldModelError> {
    let mut value = checked_ratio(nameplate.value(), i64::from(irradiance.value()), 1_000)?;
    let delta_millicelsius = i64::from(temperature.value()) - 25_000;
    let coefficient = i64::from(coefficient_ppm.unwrap_or(0));
    let temperature_factor = 1_000_000_i64
        .checked_add(
            coefficient
                .checked_mul(delta_millicelsius)
                .ok_or(YieldModelError::ArithmeticOverflow)?
                / 1_000,
        )
        .ok_or(YieldModelError::ArithmeticOverflow)?
        .max(0);
    value = checked_ratio(value, temperature_factor, 1_000_000)?;
    for loss in [
        losses.soiling,
        losses.shading,
        losses.mismatch,
        losses.wiring,
        losses.unavailability,
    ] {
        value = checked_ratio(value, 10_000 - i64::from(loss.value()), 10_000)?;
    }
    value = checked_ratio(value, 10_000 + i64::from(calibration.value()), 10_000)?;
    Ok(Watts::new(value.clamp(0, maximum)))
}

fn checked_ratio(value: i64, numerator: i64, denominator: i64) -> Result<i64, YieldModelError> {
    let product = i128::from(value)
        .checked_mul(i128::from(numerator))
        .ok_or(YieldModelError::ArithmeticOverflow)?;
    let rounded = if product >= 0 {
        product + i128::from(denominator / 2)
    } else {
        product - i128::from(denominator / 2)
    } / i128::from(denominator);
    i64::try_from(rounded).map_err(|_| YieldModelError::ArithmeticOverflow)
}

fn transpose_value(
    global: WattsPerSquareMetre,
    direct: WattsPerSquareMetre,
    diffuse: WattsPerSquareMetre,
    solar: SolarPosition,
    surface: SurfaceOrientation,
) -> WattsPerSquareMetre {
    if !solar.above_horizon() {
        return WattsPerSquareMetre::new(0);
    }
    let zenith = (f64::from(solar.zenith_millidegrees) / 1_000.0).to_radians();
    let solar_azimuth = (f64::from(solar.azimuth_millidegrees) / 1_000.0).to_radians();
    let tilt = f64::from(surface.tilt_degrees).to_radians();
    let surface_azimuth = f64::from(surface.azimuth_degrees).to_radians();
    let incidence = (zenith.cos() * tilt.cos()
        + zenith.sin() * tilt.sin() * (solar_azimuth - surface_azimuth).cos())
    .max(0.0);
    let value = f64::from(direct.value()) * incidence
        + f64::from(diffuse.value()) * (1.0 + tilt.cos()) / 2.0
        + f64::from(global.value()) * 0.2 * (1.0 - tilt.cos()) / 2.0;
    WattsPerSquareMetre::new(rounded_nonnegative_u32(value))
}

fn rounded_millidegrees(value: f64, maximum: u32) -> u32 {
    rounded_nonnegative_u32(value * 1_000.0).min(maximum)
}

fn rounded_nonnegative_u32(value: f64) -> u32 {
    if !value.is_finite() || value <= 0.0 {
        0
    } else if value >= f64::from(u32::MAX) {
        u32::MAX
    } else {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        {
            value.round() as u32
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum YieldModelError {
    #[error("surface azimuth or tilt is outside the supported range")]
    InvalidSurfaceOrientation,
    #[error("plane-of-array conversion requires GHI, DNI, and DHI")]
    MissingTranspositionInput,
    #[error("string DC nameplate power must be positive")]
    InvalidNameplatePower,
    #[error("yield model fixed-point arithmetic overflowed")]
    ArithmeticOverflow,
    #[error("inverter maximum AC capacity must be positive")]
    InvalidInverterCapacity,
}
