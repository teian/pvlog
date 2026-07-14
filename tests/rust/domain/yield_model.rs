use std::error::Error;

use pvlog_domain::{
    EstimateRange, GeographicPoint, IrradiancePoint, SurfaceOrientation, UtcTimestamp,
    WattsPerSquareMetre, plane_of_array_irradiance, solar_position,
};

#[test]
fn berlin_solstice_reference_positions_are_stable() -> Result<(), Box<dyn Error>> {
    let berlin = GeographicPoint {
        latitude_microdegrees: 52_520_000,
        longitude_microdegrees: 13_405_000,
    };
    let summer = solar_position(berlin, UtcTimestamp::from_epoch_millis(1_750_507_200_000)?);
    let winter = solar_position(berlin, UtcTimestamp::from_epoch_millis(1_766_318_400_000)?);
    assert_eq!(
        (summer.zenith_millidegrees, summer.azimuth_millidegrees),
        (30_731, 203_958)
    );
    assert_eq!(
        (winter.zenith_millidegrees, winter.azimuth_millidegrees),
        (76_910, 193_125)
    );
    Ok(())
}

#[test]
fn isotropic_transposition_is_stable_and_retains_uncertainty() -> Result<(), Box<dyn Error>> {
    let position = pvlog_domain::SolarPosition {
        zenith_millidegrees: 30_000,
        azimuth_millidegrees: 180_000,
    };
    let estimate = |central, lower, upper| EstimateRange {
        central: WattsPerSquareMetre::new(central),
        lower: Some(WattsPerSquareMetre::new(lower)),
        upper: Some(WattsPerSquareMetre::new(upper)),
    };
    let poa = plane_of_array_irradiance(
        IrradiancePoint {
            global_horizontal: Some(estimate(700, 650, 750)),
            direct_normal: Some(estimate(800, 750, 850)),
            diffuse_horizontal: Some(estimate(120, 100, 140)),
            plane_of_array: None,
        },
        position,
        SurfaceOrientation::new(180, 35)?,
    )?;
    assert_eq!(poa.central.value(), 919);
    assert_eq!(poa.lower.map(WattsPerSquareMetre::value), Some(850));
    assert_eq!(poa.upper.map(WattsPerSquareMetre::value), Some(988));
    Ok(())
}

#[test]
fn provider_plane_of_array_is_not_recomputed() -> Result<(), Box<dyn Error>> {
    let provider_value = EstimateRange::without_uncertainty(WattsPerSquareMetre::new(612));
    let actual = plane_of_array_irradiance(
        IrradiancePoint {
            global_horizontal: None,
            direct_normal: None,
            diffuse_horizontal: None,
            plane_of_array: Some(provider_value),
        },
        pvlog_domain::SolarPosition {
            zenith_millidegrees: 100_000,
            azimuth_millidegrees: 0,
        },
        SurfaceOrientation::new(180, 35)?,
    )?;
    assert_eq!(actual, provider_value);
    Ok(())
}
