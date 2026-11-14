//! `PVOutput` status and output exports are read row by row into readings and
//! daily summaries, with a reason for every row that cannot be read.

use std::{collections::BTreeMap, error::Error};

use chrono::{TimeZone as _, Utc};
use chrono_tz::Tz;
use pvlog_application::{
    PvOutputFile, PvOutputFileError, PvOutputFileKind, PvOutputRejection, PvOutputStatus,
    parse_pvoutput_export,
};

const BERLIN: Tz = chrono_tz::Europe::Berlin;

fn fixture(name: &str) -> Result<String, Box<dyn Error>> {
    Ok(std::fs::read_to_string(format!(
        "{}/../../../tests/fixtures/pvoutput/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))?)
}

fn utc(year: i32, month: u32, day: u32, hour: u32, minute: u32) -> i64 {
    Utc.with_ymd_and_hms(year, month, day, hour, minute, 0)
        .single()
        .map_or_default(|instant| instant.timestamp_millis())
}

fn rejection(row: usize, reason: &'static str, column: &'static str) -> PvOutputRejection {
    PvOutputRejection {
        row,
        reason,
        column: Some(column),
    }
}

fn status_at(file: &PvOutputFile, at: i64) -> Result<&PvOutputStatus, Box<dyn Error>> {
    file.statuses
        .iter()
        .find(|status| status.observed_at_epoch_millis == at)
        .ok_or_else(|| format!("no status at {at}").into())
}

#[test]
fn a_status_export_becomes_readings_with_interval_energy() -> Result<(), Box<dyn Error>> {
    let file = parse_pvoutput_export(&fixture("status-export.csv")?, BERLIN)?;
    assert_eq!(file.kind, PvOutputFileKind::Status);
    assert_eq!(file.rows, 16);

    // Rows are ordered by time; the energy since midnight becomes the energy
    // of each interval, and a missing value leaves the reference as it was.
    let day: Vec<_> = file
        .statuses
        .iter()
        .filter(|status| status.local_date == "2027-01-15")
        .map(|status| {
            (
                status.row,
                status.generation_energy_wh,
                status.consumption_energy_wh,
                status.counter_reset,
            )
        })
        .collect();
    assert_eq!(
        day,
        vec![
            (3, Some(10), Some(250), false),
            (2, Some(15), Some(70), false),
            (4, None, None, false),
            (5, Some(35), Some(90), false),
            // The energy since midnight went back: no interval energy.
            (6, None, Some(40), true),
            (7, Some(20), None, false),
        ]
    );

    // Local time in the system's timezone becomes UTC; the header's kWh and
    // decimal degrees and volts become PVLog's integer units.
    let first = status_at(&file, utc(2027, 1, 15, 7, 0))?;
    assert_eq!(first.generation_power_watts, Some(120));
    assert_eq!(first.generation_day_energy_wh, Some(10));
    assert_eq!(first.consumption_power_watts, Some(800));
    assert_eq!(first.temperature_millidegrees_celsius, Some(2_500));
    assert_eq!(first.voltage_millivolts, Some(231_200));
    assert_eq!(
        status_at(&file, utc(2027, 1, 15, 7, 5))?.voltage_millivolts,
        Some(231_400)
    );
    // A unit in the value wins over the header's.
    let next_day = status_at(&file, utc(2027, 1, 16, 7, 0))?;
    assert_eq!(next_day.generation_energy_wh, Some(5));
    assert_eq!(next_day.temperature_millidegrees_celsius, Some(-500));
    // When the clocks go back, a time that occurs twice is read as the first
    // and then the second occurrence.
    assert_eq!(status_at(&file, utc(2027, 10, 31, 0, 30))?.row, 16);
    let repeated = status_at(&file, utc(2027, 10, 31, 1, 30))?;
    assert_eq!(repeated.row, 17);
    assert_eq!(repeated.generation_energy_wh, Some(1));

    assert_eq!(
        file.rejected,
        vec![
            rejection(8, "invalid_date", "date"),
            rejection(9, "invalid_time", "time"),
            rejection(10, "invalid_date", "date"),
            // 02:30 does not exist when the clocks go forward.
            rejection(11, "nonexistent_local_time", "time"),
            rejection(12, "duplicate_time", "time"),
            rejection(13, "negative_value", "energyGenerated"),
            rejection(14, "unknown_unit", "powerGenerated"),
        ]
    );
    assert_eq!(file.statuses.len() + file.rejected.len(), file.rows);
    Ok(())
}

#[test]
fn a_status_service_response_is_read_without_a_header() -> Result<(), Box<dyn Error>> {
    let file = parse_pvoutput_export(&fixture("status-service.txt")?, BERLIN)?;
    assert_eq!(file.kind, PvOutputFileKind::Status);
    assert_eq!(file.rows, 3);
    assert_eq!(
        file.rejected,
        [] as [pvlog_application::PvOutputRejection; 0]
    );
    assert_eq!(
        file.statuses
            .iter()
            .map(|status| (
                status.observed_at_epoch_millis,
                status.generation_energy_wh,
                status.generation_power_watts,
                status.consumption_energy_wh,
                status.temperature_millidegrees_celsius,
            ))
            .collect::<Vec<_>>(),
        vec![
            (
                utc(2027, 1, 15, 7, 0),
                Some(5),
                Some(100),
                None,
                Some(2_900)
            ),
            (utc(2027, 1, 15, 7, 5), Some(15), Some(200), None, None),
            (
                utc(2027, 1, 15, 7, 10),
                Some(20),
                Some(300),
                None,
                Some(3_100)
            ),
        ]
    );
    Ok(())
}

#[test]
fn an_output_export_becomes_daily_summaries() -> Result<(), Box<dyn Error>> {
    let file = parse_pvoutput_export(&fixture("output-export.csv")?, BERLIN)?;
    assert_eq!(file.kind, PvOutputFileKind::Output);
    assert_eq!(file.rows, 7);
    assert_eq!(
        file.days
            .iter()
            .map(|day| (day.row, day.summary.date.as_str()))
            .collect::<Vec<_>>(),
        vec![(3, "2027-01-14"), (2, "2027-01-15")]
    );
    let first = &file.days[0].summary;
    assert_eq!(first.device_id, None);
    assert_eq!(first.generation_energy_wh, Some(5_200));
    assert_eq!(first.export_energy_wh, Some(3_100));
    assert_eq!(first.consumption_energy_wh, Some(9_800));
    assert_eq!(first.peak_power_watts, Some(2_400));
    assert_eq!(first.peak_time.as_deref(), Some("12:35"));
    assert_eq!(first.weather_condition.as_deref(), Some("Partly Cloudy"));
    assert_eq!(first.temperature_min_millidegrees_celsius, Some(-1_500));
    assert_eq!(first.temperature_max_millidegrees_celsius, Some(4_000));
    assert_eq!(
        first.import_energy_wh_by_band,
        BTreeMap::from([("offPeak".to_owned(), 3_400), ("peak".to_owned(), 1_200)])
    );
    assert!(file.days[1].summary.import_energy_wh_by_band.is_empty());

    assert_eq!(
        file.rejected,
        vec![
            rejection(4, "invalid_date", "date"),
            rejection(5, "too_long", "condition"),
            rejection(6, "duplicate_date", "date"),
            rejection(7, "invalid_value", "maxTemperature"),
            rejection(8, "invalid_time", "peakTime"),
        ]
    );
    Ok(())
}

#[test]
fn an_output_service_response_is_read_in_field_order() -> Result<(), Box<dyn Error>> {
    let service = parse_pvoutput_export(&fixture("output-service.csv")?, BERLIN)?;
    let export = parse_pvoutput_export(&fixture("output-export.csv")?, BERLIN)?;
    assert_eq!(service.kind, PvOutputFileKind::Output);
    assert_eq!(
        service.rejected,
        [] as [pvlog_application::PvOutputRejection; 0]
    );
    // Both layouts of the same days give the same summaries.
    assert_eq!(
        service
            .days
            .iter()
            .map(|day| &day.summary)
            .collect::<Vec<_>>(),
        export
            .days
            .iter()
            .map(|day| &day.summary)
            .collect::<Vec<_>>()
    );
    Ok(())
}

#[test]
fn a_file_that_is_no_export_is_refused() {
    assert_eq!(
        parse_pvoutput_export(" \n\n", BERLIN),
        Err(PvOutputFileError::Empty)
    );
    assert_eq!(
        parse_pvoutput_export("hello,world\n1,2", BERLIN),
        Err(PvOutputFileError::UnknownLayout)
    );
    assert_eq!(
        parse_pvoutput_export("Time,Power\n08:00,100", BERLIN),
        Err(PvOutputFileError::MissingColumn("date"))
    );
}
