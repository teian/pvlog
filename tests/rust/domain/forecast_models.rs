use std::{error::Error, str::FromStr as _};

use pvlog_domain::{
    AccountId, BasisPoints, CalculationSettings, CatalogEntryId, CatalogRevision,
    DimensionsMillimetres, EffectivePeriod, EquipmentTemplateReference, EquipmentValueProvenance,
    ForecastCompleteness, ForecastCompletenessReason, ForecastLossFactors, ForecastSettings,
    ForecastSettingsId, IanaTimezone, Inverter, InverterId, ModelVersion, NetCalculationMode,
    NormalizedWeatherPoint, NormalizedWeatherRun, PowerCalculationMode, PvString, PvSystem,
    SolarCellTechnology, SolarModuleSpecification, SolarModuleSpecificationSnapshot,
    SpatialCoverage, StringId, SystemId, SystemLifecycle, SystemPrivacy, TemperatureRange,
    TimeRange, UnsignedBasisPoints, UtcTimestamp, Watts, WattsPerSquareMetre, WeatherDataKind,
    WeatherDataProvenance, WeatherDataRunId,
};
use time::macros::date;
use url::Url;

#[test]
fn settings_snapshots_and_weather_runs_serialize_deterministically() -> Result<(), Box<dyn Error>> {
    assert_eq!(UnsignedBasisPoints::new(10_000)?.value(), 10_000);
    assert!(UnsignedBasisPoints::new(10_001).is_err());

    let system = complete_system()?;
    let string = &system.inverters[0].strings[0];
    let snapshot = string
        .forecast_input_snapshot()
        .ok_or("complete string has no forecast snapshot")?;
    assert_eq!(
        snapshot
            .module_specification_snapshot
            .as_ref()
            .and_then(|module| module.template.as_ref())
            .map(|template| template.value_provenance),
        Some(EquipmentValueProvenance::CatalogCustomized)
    );
    assert_eq!(snapshot.digest()?, snapshot.digest()?);
    assert_eq!(
        serde_json::to_vec(&snapshot)?,
        serde_json::to_vec(&snapshot)?
    );

    let mut changed = snapshot.clone();
    changed.module_model = Some("Field-edited module".to_owned());
    assert_ne!(snapshot.digest()?, changed.digest()?);

    let start = UtcTimestamp::from_epoch_millis(1_735_689_600_000)?;
    let end = UtcTimestamp::from_epoch_millis(1_735_693_200_000)?;
    let weather = NormalizedWeatherRun {
        id: WeatherDataRunId::new(),
        kind: WeatherDataKind::Forecast,
        issued_at: Some(start),
        valid_range: TimeRange::new(start, end)?,
        resolution_seconds: 3_600,
        spatial_coverage: SpatialCoverage::Point(pvlog_domain::GeographicPoint {
            latitude_microdegrees: 52_520_000,
            longitude_microdegrees: 13_405_000,
        }),
        provenance: WeatherDataProvenance {
            provider_id: pvlog_domain::ProviderId::new(),
            adapter: "fixture".to_owned(),
            source_url: Url::parse("https://weather.invalid/fixture")?,
            license_identifier: "fixture-license".to_owned(),
            attribution: "Deterministic fixture".to_owned(),
            fetched_at: start,
        },
        points: vec![NormalizedWeatherPoint {
            interval: TimeRange::new(start, end)?,
            irradiance: pvlog_domain::IrradiancePoint {
                global_horizontal: Some(pvlog_domain::EstimateRange::without_uncertainty(
                    WattsPerSquareMetre::new(500),
                )),
                direct_normal: None,
                diffuse_horizontal: None,
                plane_of_array: None,
            },
            ambient_temperature: None,
            wind_speed: None,
            cloud_cover: Some(UnsignedBasisPoints::new(2_500)?),
        }],
    };
    assert_eq!(serde_json::to_vec(&weather)?, serde_json::to_vec(&weather)?);
    Ok(())
}

#[test]
fn capacity_aggregation_selects_effective_versions_and_boundaries() -> Result<(), Box<dyn Error>> {
    let mut system = complete_system()?;
    let old_inverter = system.inverters[0].clone();
    let inverter_id = old_inverter.id;
    system.inverters[0].period =
        EffectivePeriod::new(date!(2025 - 01 - 01), Some(date!(2026 - 01 - 01)))?;
    let old_period = system.inverters[0].period;
    system.inverters[0].strings[0].period = old_period;
    let settings = system.inverters[0].strings[0]
        .forecast_settings
        .as_mut()
        .ok_or("complete string has no settings")?;
    settings.period = old_period;

    let mut new_inverter = old_inverter;
    new_inverter.id = inverter_id;
    new_inverter.period = EffectivePeriod::new(date!(2026 - 01 - 01), None)?;
    new_inverter.strings[0].inverter_id = inverter_id;
    new_inverter.strings[0].period = new_inverter.period;
    new_inverter.strings[0].rated_power = Watts::new(9_000);
    new_inverter.strings[0].module_peak_power = Some(Watts::new(450));
    let new_settings = new_inverter.strings[0]
        .forecast_settings
        .as_mut()
        .ok_or("complete string has no settings")?;
    new_settings.id = ForecastSettingsId::new();
    new_settings.period = new_inverter.period;
    system.inverters.push(new_inverter);

    let before = system.effective_capacity_snapshot(date!(2025 - 06 - 01))?;
    assert_eq!(before.total_peak_power, Watts::new(8_100));
    assert_eq!(before.forecast_ready_peak_power, Watts::new(8_100));
    assert_eq!(
        before.next_configuration_boundary,
        Some(date!(2026 - 01 - 01))
    );
    assert_eq!(before.completeness, ForecastCompleteness::Complete);

    let after = system.effective_capacity_snapshot(date!(2026 - 06 - 01))?;
    assert_eq!(after.total_peak_power, Watts::new(9_000));
    assert_eq!(after.inverters.len(), 1);
    assert_eq!(after.inverters[0].inverter_id, inverter_id);
    Ok(())
}

#[test]
fn incomplete_inputs_do_not_invalidate_telemetry_configuration() -> Result<(), Box<dyn Error>> {
    let mut system = complete_system()?;
    system.latitude_microdegrees = None;
    system.inverters[0].strings[0].module_specification_snapshot = None;
    system.inverters[0].strings[0].forecast_settings = None;

    system.validate_hierarchy()?;
    let capacity = system.effective_capacity_snapshot(date!(2025 - 06 - 01))?;
    assert_eq!(capacity.total_peak_power, Watts::new(8_100));
    assert_eq!(capacity.forecast_ready_peak_power, Watts::new(0));
    let ForecastCompleteness::Unavailable { reasons } = capacity.completeness else {
        return Err("incomplete forecast inputs were not reported unavailable".into());
    };
    assert!(reasons.contains(&ForecastCompletenessReason::MissingSystemLocation));
    assert!(reasons.contains(&ForecastCompletenessReason::MissingModuleSpecification));
    assert!(reasons.contains(&ForecastCompletenessReason::MissingForecastSettings));
    Ok(())
}

#[test]
fn aggregation_rejects_overlapping_versions_and_overflow() -> Result<(), Box<dyn Error>> {
    let mut overlapping = complete_system()?;
    overlapping.inverters.push(overlapping.inverters[0].clone());
    assert!(
        overlapping
            .effective_capacity_snapshot(date!(2025 - 06 - 01))
            .is_err()
    );

    let mut overflowing = complete_system()?;
    let mut second = overflowing.inverters[0].strings[0].clone();
    second.id = StringId::new();
    second.rated_power = Watts::new(1);
    overflowing.inverters[0].strings[0].rated_power = Watts::new(i64::MAX);
    overflowing.inverters[0].strings.push(second);
    assert!(
        overflowing
            .effective_capacity_snapshot(date!(2025 - 06 - 01))
            .is_err()
    );
    Ok(())
}

fn complete_system() -> Result<PvSystem, Box<dyn Error>> {
    let system_id = SystemId::new();
    let inverter_id = InverterId::new();
    let period = EffectivePeriod::new(date!(2025 - 01 - 01), None)?;
    Ok(PvSystem {
        id: system_id,
        account_id: AccountId::new(),
        name: "Forecast-ready roof".to_owned(),
        description: None,
        timezone: IanaTimezone::from_str("Europe/Berlin")?,
        commissioning_date: date!(2025 - 01 - 01),
        country_code: Some("DE".to_owned()),
        latitude_microdegrees: Some(52_520_000),
        longitude_microdegrees: Some(13_405_000),
        status_interval_seconds: 300,
        lifecycle: SystemLifecycle::Active,
        privacy: SystemPrivacy::default(),
        calculation: CalculationSettings {
            power: PowerCalculationMode::MeasuredOnly,
            net: NetCalculationMode::Measured,
            derive_interval_energy: false,
            currency: None,
        },
        inverters: vec![Inverter {
            id: inverter_id,
            system_id,
            name: "Roof inverter".to_owned(),
            manufacturer: Some("Inverter manufacturer".to_owned()),
            model: Some("Inverter model".to_owned()),
            serial_reference: None,
            rated_power: Some(Watts::new(8_000)),
            period,
            strings: vec![PvString {
                id: StringId::new(),
                inverter_id,
                name: "South roof".to_owned(),
                panel_count: 18,
                panel_manufacturer: Some("Module manufacturer".to_owned()),
                panel_model: Some("Installed module".to_owned()),
                module_peak_power: Some(Watts::new(450)),
                rated_power: Watts::new(8_100),
                module_specification_snapshot: Some(module_snapshot()),
                orientation_degrees: Some(180),
                tilt_degrees: Some(35),
                period,
                forecast_settings: Some(ForecastSettings {
                    id: ForecastSettingsId::new(),
                    period,
                    model_version: ModelVersion {
                        identifier: "pv-yield".to_owned(),
                        revision: 1,
                    },
                    losses: ForecastLossFactors {
                        soiling: UnsignedBasisPoints::new(200)?,
                        shading: UnsignedBasisPoints::new(300)?,
                        mismatch: UnsignedBasisPoints::new(100)?,
                        wiring: UnsignedBasisPoints::new(100)?,
                        unavailability: UnsignedBasisPoints::new(50)?,
                    },
                    calibration: BasisPoints::new(0)?,
                }),
            }],
        }],
    })
}

fn module_snapshot() -> SolarModuleSpecificationSnapshot {
    SolarModuleSpecificationSnapshot {
        manufacturer: "Module manufacturer".to_owned(),
        model: "Installed module".to_owned(),
        specification: SolarModuleSpecification {
            cell_technology: SolarCellTechnology::NTypeMonocrystalline,
            cell_description: Some("144 half cells".to_owned()),
            bifacial: false,
            bifaciality_factor_basis_points: None,
            bifaciality_tolerance_basis_points: None,
            peak_power_watts: 450,
            open_circuit_voltage_millivolts: 39_300,
            maximum_power_voltage_millivolts: 33_100,
            short_circuit_current_milliamperes: 14_480,
            maximum_power_current_milliamperes: 13_600,
            efficiency_basis_points: 2_200,
            short_circuit_current_temperature_coefficient_ppm_per_celsius: 450,
            open_circuit_voltage_temperature_coefficient_ppm_per_celsius: -2_500,
            peak_power_temperature_coefficient_ppm_per_celsius: -2_900,
            maximum_system_voltage_millivolts: 1_500_000,
            operating_temperature: TemperatureRange {
                minimum_milli_celsius: -40_000,
                maximum_milli_celsius: 85_000,
            },
            maximum_series_fuse_milliamperes: 25_000,
            maximum_front_static_load_pascals: 5_400,
            maximum_rear_static_load_pascals: 2_400,
            dimensions_millimetres: DimensionsMillimetres {
                length: 1_762,
                width: 1_134,
                height: 30,
            },
            weight_grams: 22_000,
        },
        template: Some(EquipmentTemplateReference {
            entry_id: CatalogEntryId("module-template".to_owned()),
            revision: CatalogRevision("2026.1".to_owned()),
            value_provenance: EquipmentValueProvenance::CatalogCustomized,
        }),
    }
}
