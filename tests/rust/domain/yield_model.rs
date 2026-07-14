use std::error::Error;

use pvlog_domain::{
    ActualEnergySample, BasisPoints, CalculationBasis, EstimateRange, ForecastLossFactors,
    GeographicPoint, IrradiancePoint, MilliDegreesCelsius, PerformanceComparison, PerformanceKind,
    StringDcInput, StringId, StringYieldContribution, SurfaceOrientation, SystemId,
    UnsignedBasisPoints, UtcTimestamp, Watts, WattsPerSquareMetre, WeatherDataKind,
    YieldModelError, aggregate_inverter_yield, aggregate_system_yield, calculate_string_dc,
    compare_actual_to_modeled, integrate_interval_energy, plane_of_array_irradiance,
    solar_position,
};

#[test]
fn performance_requires_coverage_positive_expectation_and_exact_scope() -> Result<(), Box<dyn Error>>
{
    let scope = pvlog_domain::YieldScope::System(SystemId::new());
    let actual = ActualEnergySample {
        scope,
        energy: Some(pvlog_domain::WattHours::new(900)),
        coverage: UnsignedBasisPoints::new(9_500)?,
        quality: UnsignedBasisPoints::new(9_800)?,
    };
    assert_eq!(
        compare_actual_to_modeled(
            PerformanceKind::GenerationPerformance,
            CalculationBasis::Expected,
            scope,
            actual,
            Some(pvlog_domain::WattHours::new(1_000)),
            UnsignedBasisPoints::new(9_000)?,
            UnsignedBasisPoints::new(9_000)?,
        )?,
        PerformanceComparison::Available {
            ratio_basis_points: 9_000,
            actual_energy: pvlog_domain::WattHours::new(900),
            modeled_energy: pvlog_domain::WattHours::new(1_000),
            coverage: UnsignedBasisPoints::new(9_500)?,
        }
    );
    let low_coverage = ActualEnergySample {
        coverage: UnsignedBasisPoints::new(5_000)?,
        ..actual
    };
    assert!(matches!(
        compare_actual_to_modeled(
            PerformanceKind::ForecastRealization,
            CalculationBasis::Forecast,
            scope,
            low_coverage,
            Some(pvlog_domain::WattHours::new(1_000)),
            UnsignedBasisPoints::new(9_000)?,
            UnsignedBasisPoints::new(9_000)?,
        )?,
        PerformanceComparison::Unavailable {
            reason: pvlog_domain::ForecastCompletenessReason::InsufficientActualCoverage
        }
    ));
    assert_eq!(
        compare_actual_to_modeled(
            PerformanceKind::GenerationPerformance,
            CalculationBasis::Expected,
            pvlog_domain::YieldScope::String(StringId::new()),
            actual,
            Some(pvlog_domain::WattHours::new(1_000)),
            UnsignedBasisPoints::new(9_000)?,
            UnsignedBasisPoints::new(9_000)?,
        ),
        Err(YieldModelError::UnsupportedActualAllocation)
    );
    Ok(())
}

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
fn interval_energy_keeps_forecast_and_historical_weather_paths_distinct()
-> Result<(), Box<dyn Error>> {
    let interval = pvlog_domain::TimeRange::new(
        UtcTimestamp::from_epoch_millis(0)?,
        UtcTimestamp::from_epoch_millis(900_000)?,
    )?;
    let power = EstimateRange {
        central: Watts::new(4_000),
        lower: Some(Watts::new(3_600)),
        upper: Some(Watts::new(4_400)),
    };
    let forecast = integrate_interval_energy(
        CalculationBasis::Forecast,
        WeatherDataKind::Forecast,
        power,
        interval,
    )?;
    assert_eq!(forecast.central.value(), 1_000);
    assert_eq!(
        forecast.lower.map(pvlog_domain::WattHours::value),
        Some(900)
    );
    assert_eq!(
        integrate_interval_energy(
            CalculationBasis::Expected,
            WeatherDataKind::Observed,
            power,
            interval,
        )?,
        forecast
    );
    assert_eq!(
        integrate_interval_energy(
            CalculationBasis::Expected,
            WeatherDataKind::Forecast,
            power,
            interval,
        ),
        Err(YieldModelError::IncompatibleWeatherClassification)
    );
    Ok(())
}

#[test]
fn inverter_and_system_aggregation_clip_and_preserve_partial_capacity() -> Result<(), Box<dyn Error>>
{
    let inverter = aggregate_inverter_yield(
        pvlog_domain::InverterId::new(),
        &[
            StringYieldContribution {
                string_id: StringId::new(),
                nameplate_power: Watts::new(6_000),
                power: Some(EstimateRange {
                    central: Watts::new(5_500),
                    lower: Some(Watts::new(5_000)),
                    upper: Some(Watts::new(6_000)),
                }),
                unavailable_reasons: vec![],
            },
            StringYieldContribution {
                string_id: StringId::new(),
                nameplate_power: Watts::new(2_000),
                power: None,
                unavailable_reasons: vec![pvlog_domain::ForecastCompletenessReason::MissingTilt],
            },
        ],
        UnsignedBasisPoints::new(9_700)?,
        Watts::new(5_000),
    )?;
    assert_eq!(
        inverter.power.as_ref().map(|value| value.central.value()),
        Some(5_000)
    );
    assert_eq!(inverter.included_capacity.value(), 6_000);
    assert_eq!(inverter.total_effective_capacity.value(), 8_000);
    assert!(inverter.clipped);

    let system = aggregate_system_yield(SystemId::new(), &[inverter])?;
    assert_eq!(
        system.power.as_ref().map(|value| value.central.value()),
        Some(5_000)
    );
    assert_eq!(system.included_capacity.value(), 6_000);
    assert_eq!(system.total_effective_capacity.value(), 8_000);
    Ok(())
}

#[test]
fn string_dc_applies_temperature_losses_calibration_and_physical_cap() -> Result<(), Box<dyn Error>>
{
    let losses = ForecastLossFactors {
        soiling: UnsignedBasisPoints::new(200)?,
        shading: UnsignedBasisPoints::new(100)?,
        mismatch: UnsignedBasisPoints::new(100)?,
        wiring: UnsignedBasisPoints::new(100)?,
        unavailability: UnsignedBasisPoints::new(50)?,
    };
    let estimate = calculate_string_dc(StringDcInput {
        nameplate_power: Watts::new(8_000),
        plane_of_array: EstimateRange {
            central: WattsPerSquareMetre::new(1_000),
            lower: Some(WattsPerSquareMetre::new(900)),
            upper: Some(WattsPerSquareMetre::new(1_100)),
        },
        ambient_temperature: MilliDegreesCelsius::new(20_000),
        peak_power_temperature_coefficient_ppm_per_celsius: Some(-3_500),
        losses,
        calibration: BasisPoints::new(250)?,
    })?;
    assert_eq!(estimate.module_temperature.central.value(), 51_250);
    assert_eq!(estimate.power.central.value(), 7_046);
    assert!(!estimate.was_physically_capped);

    let capped = calculate_string_dc(StringDcInput {
        nameplate_power: Watts::new(8_000),
        plane_of_array: EstimateRange::without_uncertainty(WattsPerSquareMetre::new(2_000)),
        ambient_temperature: MilliDegreesCelsius::new(-20_000),
        peak_power_temperature_coefficient_ppm_per_celsius: Some(-4_000),
        losses: ForecastLossFactors::default(),
        calibration: BasisPoints::new(0)?,
    })?;
    assert_eq!(capped.power.central.value(), 10_000);
    assert!(capped.was_physically_capped);
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
