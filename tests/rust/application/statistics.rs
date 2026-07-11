use pvlog_application::{
    StatisticsBucket, StatisticsError, StatisticsPeriod, calculate_statistics,
};
use std::error::Error;

#[test]
fn combines_energy_environment_battery_finance_and_coverage() -> Result<(), Box<dyn Error>> {
    let buckets = [
        StatisticsBucket {
            generation_wh: Some(5_000),
            consumption_wh: Some(3_000),
            grid_import_wh: Some(500),
            grid_export_wh: Some(2_500),
            peak_generation_watts: Some(4_200),
            minimum_temperature_milli_celsius: Some(10_000),
            maximum_temperature_milli_celsius: Some(25_000),
            battery_charge_wh: Some(1_000),
            battery_discharge_wh: Some(800),
            minimum_battery_basis_points: Some(2_000),
            maximum_battery_basis_points: Some(9_000),
            revenue_minor_units: Some(50),
            cost_minor_units: Some(20),
            covered_millis: 80,
            expected_millis: 100,
        },
        StatisticsBucket {
            generation_wh: Some(7_000),
            consumption_wh: Some(4_000),
            grid_import_wh: Some(1_000),
            grid_export_wh: Some(4_000),
            peak_generation_watts: Some(5_000),
            minimum_temperature_milli_celsius: Some(8_000),
            maximum_temperature_milli_celsius: Some(28_000),
            battery_charge_wh: Some(500),
            battery_discharge_wh: Some(600),
            minimum_battery_basis_points: Some(1_500),
            maximum_battery_basis_points: Some(9_500),
            revenue_minor_units: Some(70),
            cost_minor_units: Some(30),
            covered_millis: 100,
            expected_millis: 100,
        },
    ];

    let result = calculate_statistics(StatisticsPeriod::Monthly, &buckets, Some(6_000))?;

    assert_eq!(result.generation_wh, Some(12_000));
    assert_eq!(result.self_consumption_wh, Some(5_500));
    assert_eq!(result.efficiency_wh_per_kw, Some(2_000));
    assert_eq!(result.peak_generation_watts, Some(5_000));
    assert_eq!(result.minimum_temperature_milli_celsius, Some(8_000));
    assert_eq!(result.maximum_battery_basis_points, Some(9_500));
    assert_eq!(result.net_financial_minor_units, Some(70));
    assert_eq!(result.coverage_basis_points, 9_000);
    Ok(())
}

#[test]
fn unavailable_fields_remain_unavailable_instead_of_becoming_zero() -> Result<(), Box<dyn Error>> {
    let result = calculate_statistics(
        StatisticsPeriod::Lifetime,
        &[StatisticsBucket {
            generation_wh: Some(1_000),
            covered_millis: 1,
            expected_millis: 2,
            ..StatisticsBucket::default()
        }],
        None,
    )?;

    assert_eq!(result.generation_wh, Some(1_000));
    assert_eq!(result.consumption_wh, None);
    assert_eq!(result.efficiency_wh_per_kw, None);
    assert_eq!(result.net_financial_minor_units, None);
    assert_eq!(result.coverage_basis_points, 5_000);
    Ok(())
}

#[test]
fn invalid_coverage_and_capacity_are_rejected() {
    let bucket = StatisticsBucket {
        covered_millis: 2,
        expected_millis: 1,
        ..StatisticsBucket::default()
    };
    assert_eq!(
        calculate_statistics(StatisticsPeriod::Daily, &[bucket], Some(1)),
        Err(StatisticsError::InvalidCoverage)
    );

    let valid = StatisticsBucket {
        covered_millis: 1,
        expected_millis: 1,
        ..StatisticsBucket::default()
    };
    assert_eq!(
        calculate_statistics(StatisticsPeriod::Yearly, &[valid], Some(0)),
        Err(StatisticsError::InvalidCapacity)
    );
}
