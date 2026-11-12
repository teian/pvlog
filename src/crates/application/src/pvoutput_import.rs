//! Parsing of the status and output files `PVOutput` exports for a system.
//!
//! A status file holds one row per status of a day: local date and time, the
//! energy generated since midnight, the power at that moment and optionally
//! consumption, temperature and voltage. An output file holds one row per day.
//! Both come with a header row, as a download from the website, or without
//! one, in the field order of `PVOutput`'s `getstatus` (history) and `getoutput`
//! services, whose records may also be separated by semicolons.
//!
//! Every row is checked on its own: a row that cannot be read is rejected with
//! a reason, and the rest of the file is still read. Statuses become readings
//! at a UTC instant with the interval energy since the status before on the
//! same day; outputs become supplied daily summaries.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{LocalResult, NaiveDate, NaiveDateTime, NaiveTime, TimeZone as _};
use chrono_tz::Tz;
use serde::Serialize;
use thiserror::Error;

use crate::{MAXIMUM_WEATHER_CONDITION_CHARACTERS, SuppliedDailySummary};

/// Which kind of `PVOutput` export a file is.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PvOutputFileKind {
    /// Statuses through the day.
    Status,
    /// One output per day.
    Output,
}

/// One status of a status file as a reading.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PvOutputStatus {
    /// Position of the row in the file, counting from 1 and including a header.
    pub row: usize,
    /// UTC instant of the local date and time.
    pub observed_at_epoch_millis: i64,
    /// Local date of the status as `YYYY-MM-DD`.
    pub local_date: String,
    pub generation_power_watts: Option<i64>,
    /// Energy generated since the status before on the same day, or since
    /// midnight for the first status of a day.
    pub generation_energy_wh: Option<i64>,
    /// Energy generated since midnight, as the file states it.
    pub generation_day_energy_wh: Option<i64>,
    pub consumption_power_watts: Option<i64>,
    /// Energy consumed since the status before on the same day.
    pub consumption_energy_wh: Option<i64>,
    /// Energy consumed since midnight, as the file states it.
    pub consumption_day_energy_wh: Option<i64>,
    pub temperature_millidegrees_celsius: Option<i32>,
    pub voltage_millivolts: Option<u32>,
    /// True when an energy since midnight was lower than the one before it on
    /// the same day, so that no interval energy was derived from it.
    pub counter_reset: bool,
}

/// One output of an output file as a supplied daily summary of the system.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PvOutputDay {
    /// Position of the row in the file, counting from 1 and including a header.
    pub row: usize,
    /// The day, without device and with `complete` left to the importer.
    pub summary: SuppliedDailySummary,
}

/// A row that could not be read, with the reason and the column it concerns.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PvOutputRejection {
    /// Position of the row in the file, counting from 1 and including a header.
    pub row: usize,
    /// Machine-readable reason, such as `invalid_date` or `negative_value`.
    pub reason: &'static str,
    /// The column the reason concerns, when it concerns one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column: Option<&'static str>,
}

/// What was read from one export file.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PvOutputFile {
    pub kind: PvOutputFileKind,
    /// Statuses of a status file, oldest first.
    pub statuses: Vec<PvOutputStatus>,
    /// Days of an output file, oldest first.
    pub days: Vec<PvOutputDay>,
    /// Rows that were not read, in file order.
    pub rejected: Vec<PvOutputRejection>,
    /// Data rows of the file, without a header.
    pub rows: usize,
}

/// Why a file could not be read at all.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum PvOutputFileError {
    #[error("the file contains no rows")]
    Empty,
    #[error("the header names no {0} column")]
    MissingColumn(&'static str),
    #[error("the file is neither a PVOutput status nor an output export")]
    UnknownLayout,
}

/// The values a column can carry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Column {
    Date,
    Time,
    GenerationEnergy,
    GenerationPower,
    ConsumptionEnergy,
    ConsumptionPower,
    Temperature,
    Voltage,
    ExportEnergy,
    PeakPower,
    PeakTime,
    Condition,
    TemperatureMin,
    TemperatureMax,
    Import(&'static str),
}

impl Column {
    /// Name of the column in rejections.
    fn name(self) -> &'static str {
        match self {
            Self::Date => "date",
            Self::Time => "time",
            Self::GenerationEnergy => "energyGenerated",
            Self::GenerationPower => "powerGenerated",
            Self::ConsumptionEnergy => "energyUsed",
            Self::ConsumptionPower => "powerUsed",
            Self::Temperature => "temperature",
            Self::Voltage => "voltage",
            Self::ExportEnergy => "energyExported",
            Self::PeakPower => "peakPower",
            Self::PeakTime => "peakTime",
            Self::Condition => "condition",
            Self::TemperatureMin => "minTemperature",
            Self::TemperatureMax => "maxTemperature",
            Self::Import("peak") => "peakEnergyImport",
            Self::Import("offPeak") => "offPeakEnergyImport",
            Self::Import("shoulder") => "shoulderEnergyImport",
            Self::Import(_) => "highShoulderEnergyImport",
        }
    }
}

/// Field order of a `getstatus` history record; `None` is a field `PVLog` does
/// not keep (efficiency, average power, normalised output).
const STATUS_FIELDS: [Option<Column>; 11] = [
    Some(Column::Date),
    Some(Column::Time),
    Some(Column::GenerationEnergy),
    None,
    Some(Column::GenerationPower),
    None,
    None,
    Some(Column::ConsumptionEnergy),
    Some(Column::ConsumptionPower),
    Some(Column::Temperature),
    Some(Column::Voltage),
];

/// Field order of a `getoutput` record; efficiency is not kept.
const OUTPUT_FIELDS: [Option<Column>; 14] = [
    Some(Column::Date),
    Some(Column::GenerationEnergy),
    None,
    Some(Column::ExportEnergy),
    Some(Column::ConsumptionEnergy),
    Some(Column::PeakPower),
    Some(Column::PeakTime),
    Some(Column::Condition),
    Some(Column::TemperatureMin),
    Some(Column::TemperatureMax),
    Some(Column::Import("peak")),
    Some(Column::Import("offPeak")),
    Some(Column::Import("shoulder")),
    Some(Column::Import("highShoulder")),
];

/// The column a header names, by the names `PVOutput`'s services and website
/// use, with the unit the header states for its values, such as `kwh` in
/// "Energy (kWh)". A header that names none of them is a column `PVLog` does
/// not keep.
fn header_column(kind: PvOutputFileKind, header: &str) -> Option<(Column, &'static str)> {
    let name: String = header
        .chars()
        .filter_map(|character| {
            if character.is_ascii_alphanumeric() {
                Some(character.to_ascii_lowercase())
            } else if character.is_whitespace() || character == '-' || character == '_' {
                Some(' ')
            } else {
                None
            }
        })
        .collect();
    let name = name.split_whitespace().collect::<Vec<_>>().join(" ");
    // A unit in the header, such as "Power (kW)", is the unit of its values.
    let (name, unit) = ["wh", "kwh", "mwh", "w", "kw", "c", "v"]
        .iter()
        .find_map(|unit| Some((name.strip_suffix(&format!(" {unit}"))?, *unit)))
        .unwrap_or((&name, ""));
    let column = match name {
        "date" | "output date" => Column::Date,
        "time" => Column::Time,
        "temperature" | "temp" => Column::Temperature,
        "voltage" => Column::Voltage,
        _ => match kind {
            PvOutputFileKind::Status => match name {
                "energy generation" | "energy generated" | "energy" | "generated"
                | "generation" => Column::GenerationEnergy,
                "instantaneous power" | "power" | "power generation" | "power generated" => {
                    Column::GenerationPower
                }
                "energy consumption" | "energy used" | "energy consumed" | "consumption"
                | "used" => Column::ConsumptionEnergy,
                "power consumption" | "power used" | "power consumed" => Column::ConsumptionPower,
                _ => return None,
            },
            PvOutputFileKind::Output => match name {
                "energy generated" | "energy generation" | "generated" | "generation" => {
                    Column::GenerationEnergy
                }
                "energy exported" | "exported" | "export" => Column::ExportEnergy,
                "energy used" | "energy consumption" | "energy consumed" | "consumption"
                | "consumed" | "used" => Column::ConsumptionEnergy,
                "peak power" => Column::PeakPower,
                "peak time" => Column::PeakTime,
                "condition" | "conditions" | "weather" => Column::Condition,
                "min temperature" | "min temp" | "minimum temperature" => Column::TemperatureMin,
                "max temperature" | "max temp" | "maximum temperature" => Column::TemperatureMax,
                "peak energy import" | "peak import" | "import peak" => Column::Import("peak"),
                "off peak energy import" | "off peak import" | "import off peak" => {
                    Column::Import("offPeak")
                }
                "shoulder energy import" | "shoulder import" | "import shoulder" => {
                    Column::Import("shoulder")
                }
                "high shoulder energy import" | "high shoulder import" | "import high shoulder" => {
                    Column::Import("highShoulder")
                }
                _ => return None,
            },
        },
    };
    Some((column, unit))
}

/// Splits one line into its fields; a field in double quotes may contain
/// commas and doubled quotes.
fn fields(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut characters = line.chars().peekable();
    while let Some(character) = characters.next() {
        match character {
            '"' if quoted && characters.peek() == Some(&'"') => {
                field.push('"');
                characters.next();
            }
            '"' => quoted = !quoted,
            ',' if !quoted => fields.push(std::mem::take(&mut field)),
            _ => field.push(character),
        }
    }
    fields.push(field);
    fields
        .into_iter()
        .map(|field| field.trim().to_owned())
        .collect()
}

/// Splits a file into records with their position. Records are lines; the
/// records of a service response may also be separated by semicolons.
fn records(text: &str) -> Vec<(usize, Vec<String>)> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    let joined = lines.len() == 1 && lines.first().is_some_and(|line| line.contains(';'));
    let records: Vec<&str> = if joined {
        lines
            .iter()
            .flat_map(|line| line.split(';'))
            .map(str::trim)
            .filter(|record| !record.is_empty())
            .collect()
    } else {
        lines
    };
    records
        .into_iter()
        .enumerate()
        .map(|(index, record)| (index + 1, fields(record)))
        .collect()
}

fn is_time(text: &str) -> bool {
    parse_time(text).is_some()
}

fn parse_time(text: &str) -> Option<NaiveTime> {
    NaiveTime::parse_from_str(text, "%H:%M")
        .or_else(|_| NaiveTime::parse_from_str(text, "%H:%M:%S"))
        .ok()
}

fn parse_date(text: &str) -> Option<NaiveDate> {
    let date = if text.len() == 8 && text.bytes().all(|byte| byte.is_ascii_digit()) {
        NaiveDate::parse_from_str(text, "%Y%m%d").ok()?
    } else if text.len() == 10 {
        NaiveDate::parse_from_str(text, "%Y-%m-%d").ok()?
    } else {
        return None;
    };
    Some(date)
}

/// What a numeric column holds and the unit `PVLog` stores it in.
#[derive(Clone, Copy)]
enum Quantity {
    /// Watt-hours; `kWh` and `MWh` are converted.
    Energy,
    /// Watts; `kW` is converted.
    Power,
    /// Thousandths of a degree Celsius.
    Temperature,
    /// Millivolts.
    Voltage,
}

/// Reads a number with an optional unit and returns it in `PVLog`'s unit,
/// rounded to the nearest integer. A number without a unit is in
/// `default_unit`, the unit its header names, or else in `PVOutput`'s unit.
/// An empty field, `NaN` or `-` is missing.
fn quantity(text: &str, kind: Quantity, default_unit: &str) -> Result<Option<i64>, &'static str> {
    let text = text.trim();
    if text.is_empty() || text == "-" || text.eq_ignore_ascii_case("nan") {
        return Ok(None);
    }
    let split = text
        .find(|character: char| {
            !(character.is_ascii_digit() || character == '.' || character == '-')
        })
        .unwrap_or(text.len());
    let (number, unit) = text.split_at(split);
    let unit = unit.trim().trim_start_matches('°').to_ascii_lowercase();
    let unit = if unit.is_empty() {
        default_unit.to_owned()
    } else {
        unit
    };
    let factor: f64 = match (kind, unit.as_str()) {
        (Quantity::Energy, "" | "wh") | (Quantity::Power, "" | "w") => 1.0,
        (Quantity::Energy, "kwh")
        | (Quantity::Power, "kw")
        | (Quantity::Temperature, "" | "c")
        | (Quantity::Voltage, "" | "v") => 1_000.0,
        (Quantity::Energy, "mwh") => 1_000_000.0,
        _ => return Err("unknown_unit"),
    };
    let value: f64 = number.parse().map_err(|_| "invalid_number")?;
    if !value.is_finite() {
        return Err("invalid_number");
    }
    let scaled = (value * factor).round();
    if matches!(kind, Quantity::Energy | Quantity::Power) && scaled < 0.0 {
        return Err("negative_value");
    }
    if scaled.abs() > 9.0e15 {
        return Err("out_of_range");
    }
    #[allow(clippy::cast_possible_truncation)]
    Ok(Some(scaled as i64))
}

/// Where a column is found in a row and the unit its header names.
#[derive(Clone, Copy)]
struct Placement {
    index: usize,
    column: Column,
    unit: &'static str,
}

/// A data row with its fields assigned to columns.
struct Row<'a> {
    number: usize,
    values: &'a [String],
    columns: &'a [Placement],
}

impl Row<'_> {
    fn placement(&self, column: Column) -> Option<Placement> {
        self.columns
            .iter()
            .find(|placement| placement.column == column)
            .copied()
    }

    fn text(&self, column: Column) -> Option<&str> {
        self.placement(column)
            .and_then(|placement| self.values.get(placement.index))
            .map(String::as_str)
            .filter(|text| !text.is_empty())
    }

    fn reject(&self, reason: &'static str, column: Option<Column>) -> PvOutputRejection {
        PvOutputRejection {
            row: self.number,
            reason,
            column: column.map(Column::name),
        }
    }

    fn quantity(&self, column: Column, kind: Quantity) -> Result<Option<i64>, PvOutputRejection> {
        let unit = self
            .placement(column)
            .map_or("", |placement| placement.unit);
        self.text(column).map_or(Ok(None), |text| {
            quantity(text, kind, unit).map_err(|reason| self.reject(reason, Some(column)))
        })
    }

    fn date(&self) -> Result<NaiveDate, PvOutputRejection> {
        self.text(Column::Date)
            .and_then(parse_date)
            .ok_or_else(|| self.reject("invalid_date", Some(Column::Date)))
    }
}

/// Reads a `PVOutput` status or output export.
///
/// Local dates and times of statuses are read in `timezone`. A file whose
/// layout is not recognized is refused as a whole; every other problem
/// rejects only its row.
///
/// # Errors
///
/// Returns an error for a file without rows, a header without a date (or, in
/// a status file, a time) column, and a file that is neither export.
pub fn parse_pvoutput_export(text: &str, timezone: Tz) -> Result<PvOutputFile, PvOutputFileError> {
    let records = records(text);
    let (_, first) = records.first().ok_or(PvOutputFileError::Empty)?;
    let header = first.iter().any(|field| {
        let field = field.to_ascii_lowercase();
        field.contains("date") || field == "time"
    });
    let (kind, columns, data) = if header {
        let kind = if first
            .iter()
            .any(|field| field.trim().eq_ignore_ascii_case("time"))
        {
            PvOutputFileKind::Status
        } else {
            PvOutputFileKind::Output
        };
        let columns: Vec<Placement> = first
            .iter()
            .enumerate()
            .filter_map(|(index, name)| {
                let (column, unit) = header_column(kind, name)?;
                Some(Placement {
                    index,
                    column,
                    unit,
                })
            })
            .collect();
        for required in [Column::Date, Column::Time] {
            let needed = required == Column::Date || kind == PvOutputFileKind::Status;
            if needed && !columns.iter().any(|placement| placement.column == required) {
                return Err(PvOutputFileError::MissingColumn(required.name()));
            }
        }
        (kind, columns, &records[1..])
    } else {
        let kind = match first.get(1) {
            Some(second) if is_time(second) => PvOutputFileKind::Status,
            Some(_) if first.first().is_some_and(|date| parse_date(date).is_some()) => {
                PvOutputFileKind::Output
            }
            _ => return Err(PvOutputFileError::UnknownLayout),
        };
        let order: &[Option<Column>] = match kind {
            PvOutputFileKind::Status => &STATUS_FIELDS,
            PvOutputFileKind::Output => &OUTPUT_FIELDS,
        };
        let columns = order
            .iter()
            .enumerate()
            .filter_map(|(index, column)| {
                Some(Placement {
                    index,
                    column: (*column)?,
                    unit: "",
                })
            })
            .collect();
        (kind, columns, &records[..])
    };
    let mut file = PvOutputFile {
        kind,
        statuses: Vec::new(),
        days: Vec::new(),
        rejected: Vec::new(),
        rows: data.len(),
    };
    for (number, values) in data {
        let row = Row {
            number: *number,
            values,
            columns: &columns,
        };
        let outcome = match kind {
            PvOutputFileKind::Status => {
                status(&row, timezone, &file.statuses).map(|status| file.statuses.push(status))
            }
            PvOutputFileKind::Output => day(&row).map(|day| file.days.push(day)),
        };
        if let Err(rejection) = outcome {
            file.rejected.push(rejection);
        }
    }
    if kind == PvOutputFileKind::Status {
        file.statuses = with_interval_energy(file.statuses, &mut file.rejected);
    } else {
        file.days = distinct_days(file.days, &mut file.rejected);
    }
    file.rejected.sort_by_key(|rejection| rejection.row);
    Ok(file)
}

/// Reads one status row.
fn status(
    row: &Row<'_>,
    timezone: Tz,
    earlier: &[PvOutputStatus],
) -> Result<PvOutputStatus, PvOutputRejection> {
    let date = row.date()?;
    let time = row
        .text(Column::Time)
        .and_then(parse_time)
        .ok_or_else(|| row.reject("invalid_time", Some(Column::Time)))?;
    let local = NaiveDateTime::new(date, time);
    let instant = match timezone.from_local_datetime(&local) {
        LocalResult::Single(instant) => instant,
        // A time that occurs twice when the clocks go back is the first
        // occurrence, unless an earlier row already took that one.
        LocalResult::Ambiguous(first, second) => {
            let taken = earlier
                .iter()
                .any(|status| status.observed_at_epoch_millis == first.timestamp_millis());
            if taken { second } else { first }
        }
        LocalResult::None => {
            return Err(row.reject("nonexistent_local_time", Some(Column::Time)));
        }
    };
    let temperature = row
        .quantity(Column::Temperature, Quantity::Temperature)?
        .map(|value| {
            i32::try_from(value).map_err(|_| row.reject("out_of_range", Some(Column::Temperature)))
        })
        .transpose()?;
    let voltage = row
        .quantity(Column::Voltage, Quantity::Voltage)?
        .map(|value| {
            u32::try_from(value).map_err(|_| row.reject("out_of_range", Some(Column::Voltage)))
        })
        .transpose()?;
    Ok(PvOutputStatus {
        row: row.number,
        observed_at_epoch_millis: instant.timestamp_millis(),
        local_date: date.format("%Y-%m-%d").to_string(),
        generation_power_watts: row.quantity(Column::GenerationPower, Quantity::Power)?,
        generation_day_energy_wh: row.quantity(Column::GenerationEnergy, Quantity::Energy)?,
        consumption_power_watts: row.quantity(Column::ConsumptionPower, Quantity::Power)?,
        consumption_day_energy_wh: row.quantity(Column::ConsumptionEnergy, Quantity::Energy)?,
        temperature_millidegrees_celsius: temperature,
        voltage_millivolts: voltage,
        ..PvOutputStatus::default()
    })
}

/// Orders the statuses by time, rejects a second status for the same instant
/// and derives each status's interval energy from the energy since midnight.
fn with_interval_energy(
    mut statuses: Vec<PvOutputStatus>,
    rejected: &mut Vec<PvOutputRejection>,
) -> Vec<PvOutputStatus> {
    statuses.sort_by_key(|status| (status.observed_at_epoch_millis, status.row));
    let mut seen = BTreeSet::new();
    statuses.retain(|status| {
        let first = seen.insert(status.observed_at_epoch_millis);
        if !first {
            rejected.push(PvOutputRejection {
                row: status.row,
                reason: "duplicate_time",
                column: Some(Column::Time.name()),
            });
        }
        first
    });
    // The energy since midnight of the status before on the same day.
    let mut before: Option<(String, Option<i64>, Option<i64>)> = None;
    for status in &mut statuses {
        let (generation_before, consumption_before) = match &before {
            Some((date, generation, consumption)) if *date == status.local_date => {
                (*generation, *consumption)
            }
            // The day's energy starts from zero at midnight.
            _ => (Some(0), Some(0)),
        };
        let mut reset = false;
        let mut interval = |now: Option<i64>, previous: Option<i64>| match (now, previous) {
            (Some(now), Some(previous)) if now >= previous => Some(now - previous),
            (Some(_), Some(_)) => {
                reset = true;
                None
            }
            _ => None,
        };
        status.generation_energy_wh = interval(status.generation_day_energy_wh, generation_before);
        status.consumption_energy_wh =
            interval(status.consumption_day_energy_wh, consumption_before);
        status.counter_reset = reset;
        // A missing value leaves the last known one as the reference.
        before = Some((
            status.local_date.clone(),
            status.generation_day_energy_wh.or(generation_before),
            status.consumption_day_energy_wh.or(consumption_before),
        ));
    }
    statuses
}

/// Reads one output row and checks it as a supplied daily summary.
fn day(row: &Row<'_>) -> Result<PvOutputDay, PvOutputRejection> {
    let date = row.date()?;
    let peak_time = row
        .text(Column::PeakTime)
        .map(|text| {
            parse_time(text)
                .map(|time| time.format("%H:%M").to_string())
                .ok_or_else(|| row.reject("invalid_time", Some(Column::PeakTime)))
        })
        .transpose()?;
    let condition = row.text(Column::Condition).map(ToOwned::to_owned);
    if condition
        .as_ref()
        .is_some_and(|text| text.chars().count() > MAXIMUM_WEATHER_CONDITION_CHARACTERS)
    {
        return Err(row.reject("too_long", Some(Column::Condition)));
    }
    let mut import_energy_wh_by_band = BTreeMap::new();
    for band in ["peak", "offPeak", "shoulder", "highShoulder"] {
        if let Some(value) = row.quantity(Column::Import(band), Quantity::Energy)? {
            import_energy_wh_by_band.insert(band.to_owned(), value);
        }
    }
    let summary = SuppliedDailySummary {
        date: date.format("%Y-%m-%d").to_string(),
        device_id: None,
        generation_energy_wh: row.quantity(Column::GenerationEnergy, Quantity::Energy)?,
        export_energy_wh: row.quantity(Column::ExportEnergy, Quantity::Energy)?,
        consumption_energy_wh: row.quantity(Column::ConsumptionEnergy, Quantity::Energy)?,
        generation_lifetime_wh: None,
        complete: false,
        peak_power_watts: row.quantity(Column::PeakPower, Quantity::Power)?,
        peak_time,
        weather_condition: condition,
        temperature_min_millidegrees_celsius: row
            .quantity(Column::TemperatureMin, Quantity::Temperature)?,
        temperature_max_millidegrees_celsius: row
            .quantity(Column::TemperatureMax, Quantity::Temperature)?,
        import_energy_wh_by_band,
    };
    // The same checks as a daily summary sent through the API.
    summary.validate().map_err(|field| {
        row.reject(
            "invalid_value",
            Some(match field {
                "temperatureMaxMillidegreesCelsius" => Column::TemperatureMax,
                "peakTime" => Column::PeakTime,
                "weatherCondition" => Column::Condition,
                _ => Column::Date,
            }),
        )
    })?;
    Ok(PvOutputDay {
        row: row.number,
        summary,
    })
}

/// Orders the days by date and rejects a second output for the same day.
fn distinct_days(
    mut days: Vec<PvOutputDay>,
    rejected: &mut Vec<PvOutputRejection>,
) -> Vec<PvOutputDay> {
    days.sort_by(|left, right| {
        (left.summary.date.as_str(), left.row).cmp(&(right.summary.date.as_str(), right.row))
    });
    let mut seen = BTreeSet::new();
    days.retain(|day| {
        let first = seen.insert(day.summary.date.clone());
        if !first {
            rejected.push(PvOutputRejection {
                row: day.row,
                reason: "duplicate_date",
                column: Some(Column::Date.name()),
            });
        }
        first
    });
    days
}
