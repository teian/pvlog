//! Deterministic version-1 PV yield model primitives.

use crate::{EstimateRange, GeographicPoint, IrradiancePoint, UtcTimestamp, WattsPerSquareMetre};
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
}
