//! Reads an `SBFspot` `SQLite` database: the archive in `DayData` drives the
//! upload, and the optional tables add what each inverter measured, who it
//! is, what the site consumed and which events the inverters reported.

use serde::Serialize;
use sqlx::{
    Row, SqlitePool,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions, SqliteRow},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    str::FromStr,
    time::Duration,
};
use thiserror::Error;

/// Seconds around an archive time within which a spot value belongs to it.
const SPOT_WINDOW_SECONDS: i64 = 150;

/// SMA's code for a status the device did not report.
const STATUS_NOT_AVAILABLE: i64 = 16_777_213;

/// One reading of the system with the values of its inverters.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Reading {
    #[serde(skip)]
    pub timestamp: i64,
    pub observed_at_epoch_millis: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generation_power_watts: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generation_energy_wh: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub consumption_power_watts: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub consumption_energy_wh: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub voltage_millivolts: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature_millidegrees_celsius: Option<i32>,
    pub source_reference: String,
    /// Values per inverter under the device key scheme `inv.<serial>.<measurement>`,
    /// in the integer unit the scheme defines for the key.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub channels: BTreeMap<String, ChannelValue>,
    /// Identity of the inverters, sent with the first reading of a page.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub devices: Vec<DeviceIdentity>,
}

/// The value of one measurement channel.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct ChannelValue {
    pub value: i64,
}

/// What `SBFspot` knows about an inverter.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceIdentity {
    /// Serial number of the inverter.
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reported_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub firmware: Option<String>,
}

/// An event an inverter reported.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceEvent {
    #[serde(skip)]
    pub position: EventPosition,
    pub device_id: String,
    pub event_id: String,
    pub occurred_at_epoch_millis: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<i64>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub event_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_group: Option<String>,
}

/// Where an event stands in the order events are uploaded in.
#[derive(
    Clone, Copy, Debug, Default, serde::Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize,
)]
#[serde(rename_all = "camelCase")]
pub struct EventPosition {
    pub timestamp: i64,
    pub serial: i64,
    pub entry_id: i64,
}

/// A group of values an `SBFspot` database may or may not hold.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceGroup {
    /// Per-inverter phase, tracker, status and temperature values (`SpotData`).
    SpotValues,
    /// Site consumption (`Consumption`).
    Consumption,
    /// Name, type and software version of the inverters (`Inverters`).
    InverterIdentity,
    /// Events of the inverters (`EventData`).
    Events,
}

#[derive(Clone, Debug)]
pub struct SbfspotSource {
    pool: SqlitePool,
    has_consumption: bool,
    has_inverters: bool,
    has_events: bool,
    /// Columns of `SpotData`; empty when the table does not exist.
    spot_columns: BTreeSet<String>,
}

impl SbfspotSource {
    /// Opens a live `SBFspot` database in read-only mode and validates its schema.
    ///
    /// # Errors
    /// Returns an error if `SQLite` cannot be opened or the required `DayData` table is missing.
    pub async fn open(path: &Path) -> Result<Self, SbfspotError> {
        if !path.is_file() {
            return Err(SbfspotError::DatabaseMissing(path.to_path_buf()));
        }
        let path_text = path
            .to_str()
            .ok_or_else(|| SbfspotError::InvalidPath(path.to_path_buf()))?;
        let options = SqliteConnectOptions::from_str(path_text)
            .map_err(SbfspotError::Database)?
            .read_only(true)
            .create_if_missing(false)
            .busy_timeout(Duration::from_secs(10));
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .map_err(SbfspotError::Database)?;
        if !table_exists(&pool, "DayData").await? {
            return Err(SbfspotError::MissingDayData);
        }
        let spot_columns = sqlx::query("SELECT name FROM pragma_table_info('SpotData')")
            .fetch_all(&pool)
            .await
            .map_err(SbfspotError::Database)?
            .iter()
            .map(|row| row.try_get::<String, _>("name"))
            .collect::<Result<_, _>>()?;
        Ok(Self {
            has_consumption: table_exists(&pool, "Consumption").await?,
            has_inverters: table_exists(&pool, "Inverters").await?,
            has_events: table_exists(&pool, "EventData").await?,
            spot_columns,
            pool,
        })
    }

    /// The groups of values this database does not hold. What exists is
    /// delivered all the same.
    #[must_use]
    pub fn unavailable_groups(&self) -> Vec<SourceGroup> {
        [
            (self.spot_columns.is_empty(), SourceGroup::SpotValues),
            (!self.has_consumption, SourceGroup::Consumption),
            (!self.has_inverters, SourceGroup::InverterIdentity),
            (!self.has_events, SourceGroup::Events),
        ]
        .into_iter()
        .filter_map(|(absent, group)| absent.then_some(group))
        .collect()
    }

    /// Reads one ordered page of archive times with the values of every
    /// inverter and converts the lifetime counters to interval energy.
    ///
    /// Energy is the sum of what each inverter's counter grew by. It is left
    /// out when the inverters of a time are not the inverters of the time
    /// before, because a replaced or silent inverter makes the difference of
    /// the sums meaningless.
    ///
    /// # Errors
    /// Returns an error for malformed values, overflow, or `SQLite` failures.
    pub async fn read_after(
        &self,
        timestamp: i64,
        limit: usize,
    ) -> Result<Vec<Reading>, SbfspotError> {
        let limit = i64::try_from(limit).map_err(|_| SbfspotError::LimitOverflow)?;
        let times: Vec<i64> = sqlx::query_scalar(
            "SELECT DISTINCT TimeStamp FROM DayData WHERE TimeStamp > ?1 \
             ORDER BY TimeStamp ASC LIMIT ?2",
        )
        .bind(timestamp)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(SbfspotError::Database)?;
        let (Some(first), Some(last)) = (times.first().copied(), times.last().copied()) else {
            return Ok(Vec::new());
        };
        let mut archive = self.archive(first, last).await?;
        let mut spot = self.spot_values(first, last).await?;
        let mut consumption = self.consumption(first, last).await?;
        let mut previous = self.baseline_before(first).await?;
        let identities = self.identities().await?;

        let mut readings = Vec::with_capacity(times.len());
        for time in times {
            let inverters = archive.remove(&time).unwrap_or_default();
            let consumed = consumption.remove(&time);
            let current = Baseline {
                totals: inverters
                    .iter()
                    .filter_map(|(serial, row)| Some((*serial, row.total_yield?)))
                    .collect(),
                consumption_total: consumed.and_then(|row| row.energy_used),
            };
            let mut channels = BTreeMap::new();
            let (mut voltages, mut temperatures) = (Vec::new(), Vec::new());
            for (serial, row) in &inverters {
                let mut put = |measurement: &str, value: Option<i64>| {
                    if let Some(value) = value {
                        channels.insert(
                            format!("inv.{serial}.{measurement}"),
                            ChannelValue { value },
                        );
                    }
                };
                put("energy.lifetime", row.total_yield);
                if let Some(values) = spot.get_mut(serial).and_then(|rows| nearest(rows, time)) {
                    values.channels(&mut put);
                    voltages.extend(values.ac[0].voltage.filter(|voltage| *voltage != 0.0));
                    temperatures.extend(values.temperature);
                }
            }
            readings.push(Reading {
                timestamp: time,
                observed_at_epoch_millis: time
                    .checked_mul(1000)
                    .ok_or(SbfspotError::TimestampOverflow(time))?,
                generation_power_watts: sum(inverters.values().filter_map(|row| row.power)),
                generation_energy_wh: previous
                    .as_ref()
                    .and_then(|previous| generation_energy(&current.totals, &previous.totals)),
                consumption_power_watts: consumed.and_then(|row| row.power_used),
                consumption_energy_wh: previous
                    .as_ref()
                    .and_then(|previous| previous.consumption_total)
                    .zip(current.consumption_total)
                    .and_then(|(previous, current)| checked_delta(current, previous)),
                voltage_millivolts: mean(&voltages)
                    .and_then(|volts| scaled(Some(volts), 1000.0))
                    .and_then(|millivolts| u32::try_from(millivolts).ok()),
                temperature_millidegrees_celsius: mean(&temperatures)
                    .and_then(|degrees| scaled(Some(degrees), 1000.0))
                    .and_then(|millidegrees| i32::try_from(millidegrees).ok()),
                source_reference: format!("sbfspot:daydata:{time}"),
                channels,
                devices: if readings.is_empty() {
                    identities.clone()
                } else {
                    Vec::new()
                },
            });
            previous = Some(current);
        }
        Ok(readings)
    }

    /// Reads one ordered page of events after a position.
    ///
    /// # Errors
    /// Returns an error for malformed values or `SQLite` failures.
    pub async fn read_events_after(
        &self,
        position: EventPosition,
        limit: usize,
    ) -> Result<Vec<DeviceEvent>, SbfspotError> {
        if !self.has_events {
            return Ok(Vec::new());
        }
        let limit = i64::try_from(limit).map_err(|_| SbfspotError::LimitOverflow)?;
        sqlx::query(
            "SELECT EntryID,TimeStamp,Serial,EventCode,EventType,Category,EventGroup,Tag,\
             OldValue,NewValue,UserGroup FROM EventData \
             WHERE (TimeStamp,Serial,EntryID) > (?1,?2,?3) \
             ORDER BY TimeStamp,Serial,EntryID LIMIT ?4",
        )
        .bind(position.timestamp)
        .bind(position.serial)
        .bind(position.entry_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(SbfspotError::Database)?
        .iter()
        .map(|row| {
            let position = EventPosition {
                timestamp: row.try_get("TimeStamp")?,
                serial: row.try_get("Serial")?,
                entry_id: row.try_get("EntryID")?,
            };
            Ok(DeviceEvent {
                position,
                device_id: position.serial.to_string(),
                event_id: position.entry_id.to_string(),
                occurred_at_epoch_millis: position
                    .timestamp
                    .checked_mul(1000)
                    .ok_or(SbfspotError::TimestampOverflow(position.timestamp))?,
                code: row.try_get("EventCode")?,
                event_type: text(row, "EventType")?,
                category: text(row, "Category")?,
                group: text(row, "EventGroup")?,
                message: text(row, "Tag")?,
                old_value: text(row, "OldValue")?,
                new_value: text(row, "NewValue")?,
                user_group: text(row, "UserGroup")?,
            })
        })
        .collect()
    }

    /// Reads the archive rows of a range of times, by time and serial.
    async fn archive(
        &self,
        first: i64,
        last: i64,
    ) -> Result<BTreeMap<i64, BTreeMap<i64, ArchiveRow>>, SbfspotError> {
        let mut archive: BTreeMap<i64, BTreeMap<i64, ArchiveRow>> = BTreeMap::new();
        for row in sqlx::query(
            "SELECT TimeStamp,Serial,TotalYield,Power FROM DayData \
             WHERE TimeStamp BETWEEN ?1 AND ?2",
        )
        .bind(first)
        .bind(last)
        .fetch_all(&self.pool)
        .await
        .map_err(SbfspotError::Database)?
        {
            archive
                .entry(row.try_get("TimeStamp")?)
                .or_default()
                .insert(
                    row.try_get("Serial")?,
                    ArchiveRow {
                        total_yield: row.try_get("TotalYield")?,
                        power: row.try_get("Power")?,
                    },
                );
        }
        Ok(archive)
    }

    /// Reads the spot values around a range of archive times, by serial and
    /// in order of time. A column an older database lacks reads as missing.
    async fn spot_values(
        &self,
        first: i64,
        last: i64,
    ) -> Result<BTreeMap<i64, Vec<SpotValues>>, SbfspotError> {
        let mut spot: BTreeMap<i64, Vec<SpotValues>> = BTreeMap::new();
        if self.spot_columns.is_empty() {
            return Ok(spot);
        }
        let columns: Vec<String> = SPOT_COLUMNS
            .iter()
            .map(|column| {
                if self.spot_columns.contains(*column) {
                    (*column).to_owned()
                } else {
                    format!("NULL AS {column}")
                }
            })
            .collect();
        // The column names come from the fixed list above, never from the database.
        let sql = format!(
            "SELECT {} FROM SpotData WHERE TimeStamp BETWEEN ?1 AND ?2 ORDER BY TimeStamp",
            columns.join(",")
        );
        for row in sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(first - SPOT_WINDOW_SECONDS)
            .bind(last + SPOT_WINDOW_SECONDS)
            .fetch_all(&self.pool)
            .await
            .map_err(SbfspotError::Database)?
        {
            spot.entry(row.try_get("Serial")?)
                .or_default()
                .push(SpotValues::read(&row)?);
        }
        Ok(spot)
    }

    async fn consumption(
        &self,
        first: i64,
        last: i64,
    ) -> Result<BTreeMap<i64, ConsumptionRow>, SbfspotError> {
        let mut consumption = BTreeMap::new();
        if !self.has_consumption {
            return Ok(consumption);
        }
        for row in sqlx::query(
            "SELECT TimeStamp,EnergyUsed,PowerUsed FROM Consumption \
             WHERE TimeStamp BETWEEN ?1 AND ?2",
        )
        .bind(first)
        .bind(last)
        .fetch_all(&self.pool)
        .await
        .map_err(SbfspotError::Database)?
        {
            consumption.insert(
                row.try_get("TimeStamp")?,
                ConsumptionRow {
                    energy_used: row.try_get("EnergyUsed")?,
                    power_used: row.try_get("PowerUsed")?,
                },
            );
        }
        Ok(consumption)
    }

    /// Reads the counters of the archive time before `timestamp`.
    async fn baseline_before(&self, timestamp: i64) -> Result<Option<Baseline>, SbfspotError> {
        let rows = sqlx::query(
            "SELECT Serial,TotalYield FROM DayData \
             WHERE TimeStamp = (SELECT MAX(TimeStamp) FROM DayData WHERE TimeStamp < ?1)",
        )
        .bind(timestamp)
        .fetch_all(&self.pool)
        .await
        .map_err(SbfspotError::Database)?;
        if rows.is_empty() {
            return Ok(None);
        }
        let mut totals = BTreeMap::new();
        for row in rows {
            if let Some(total) = row.try_get::<Option<i64>, _>("TotalYield")? {
                totals.insert(row.try_get("Serial")?, total);
            }
        }
        let consumption_total = if self.has_consumption {
            sqlx::query_scalar::<_, Option<i64>>(
                "SELECT EnergyUsed FROM Consumption WHERE TimeStamp < ?1 \
                 ORDER BY TimeStamp DESC LIMIT 1",
            )
            .bind(timestamp)
            .fetch_optional(&self.pool)
            .await
            .map_err(SbfspotError::Database)?
            .flatten()
        } else {
            None
        };
        Ok(Some(Baseline {
            totals,
            consumption_total,
        }))
    }

    async fn identities(&self) -> Result<Vec<DeviceIdentity>, SbfspotError> {
        if !self.has_inverters {
            return Ok(Vec::new());
        }
        sqlx::query("SELECT Serial,Name,Type,SW_Version FROM Inverters ORDER BY Serial")
            .fetch_all(&self.pool)
            .await
            .map_err(SbfspotError::Database)?
            .iter()
            .map(|row| {
                Ok(DeviceIdentity {
                    id: row.try_get::<i64, _>("Serial")?.to_string(),
                    reported_name: text(row, "Name")?,
                    model: text(row, "Type")?,
                    firmware: text(row, "SW_Version")?,
                })
            })
            .collect()
    }
}

/// Columns of `SpotData` the uploader reads, in the order they are selected.
const SPOT_COLUMNS: &[&str] = &[
    "TimeStamp",
    "Serial",
    "Pdc1",
    "Pdc2",
    "Idc1",
    "Idc2",
    "Udc1",
    "Udc2",
    "Pac1",
    "Pac2",
    "Pac3",
    "Iac1",
    "Iac2",
    "Iac3",
    "Uac1",
    "Uac2",
    "Uac3",
    "EToday",
    "Frequency",
    "OperatingTime",
    "FeedInTime",
    "BT_Signal",
    "Status",
    "GridRelay",
    "Temperature",
];

#[derive(Clone, Copy, Debug)]
struct ArchiveRow {
    total_yield: Option<i64>,
    power: Option<i64>,
}

#[derive(Clone, Copy, Debug)]
struct ConsumptionRow {
    energy_used: Option<i64>,
    power_used: Option<i64>,
}

/// The lifetime counters of one archive time.
#[derive(Clone, Debug)]
struct Baseline {
    /// `TotalYield` in watt-hours by inverter serial.
    totals: BTreeMap<i64, i64>,
    consumption_total: Option<i64>,
}

/// Power, current and voltage of one AC phase or DC input, in W, A and V.
#[derive(Clone, Copy, Debug, Default)]
struct Electrical {
    power: Option<i64>,
    current: Option<f64>,
    voltage: Option<f64>,
}

impl Electrical {
    /// `SBFspot` writes zeros for a phase or input the inverter does not
    /// have, so one that is zero throughout is treated as absent.
    fn is_absent(self) -> bool {
        self.power.unwrap_or(0) == 0
            && self.current.unwrap_or(0.0) == 0.0
            && self.voltage.unwrap_or(0.0) == 0.0
    }
}

/// One row of `SpotData`, in the units `SBFspot` stores.
#[derive(Clone, Debug, Default)]
struct SpotValues {
    timestamp: i64,
    ac: [Electrical; 3],
    dc: [Electrical; 2],
    energy_today: Option<i64>,
    frequency: Option<f64>,
    operating_time_hours: Option<f64>,
    feed_in_time_hours: Option<f64>,
    signal_percent: Option<f64>,
    status: Option<String>,
    grid_relay: Option<String>,
    temperature: Option<f64>,
}

impl SpotValues {
    fn read(row: &SqliteRow) -> Result<Self, SbfspotError> {
        let electrical = |power: &str, current: &str, voltage: &str| {
            Ok::<_, SbfspotError>(Electrical {
                power: row.try_get(power)?,
                current: row.try_get(current)?,
                voltage: row.try_get(voltage)?,
            })
        };
        Ok(Self {
            timestamp: row.try_get("TimeStamp")?,
            ac: [
                electrical("Pac1", "Iac1", "Uac1")?,
                electrical("Pac2", "Iac2", "Uac2")?,
                electrical("Pac3", "Iac3", "Uac3")?,
            ],
            dc: [
                electrical("Pdc1", "Idc1", "Udc1")?,
                electrical("Pdc2", "Idc2", "Udc2")?,
            ],
            energy_today: row.try_get("EToday")?,
            frequency: row.try_get("Frequency")?,
            operating_time_hours: row.try_get("OperatingTime")?,
            feed_in_time_hours: row.try_get("FeedInTime")?,
            signal_percent: row.try_get("BT_Signal")?,
            status: row.try_get("Status")?,
            grid_relay: row.try_get("GridRelay")?,
            temperature: row.try_get("Temperature")?,
        })
    }

    /// Hands every value to `put` under its measurement of the device key
    /// scheme, converted to the integer unit the scheme defines.
    fn channels(&self, put: &mut impl FnMut(&str, Option<i64>)) {
        for (prefix, values) in [("ac.l", self.ac.as_slice()), ("mppt.", self.dc.as_slice())] {
            for (index, values) in values.iter().enumerate() {
                if values.is_absent() {
                    continue;
                }
                let number = index + 1;
                put(&format!("{prefix}{number}.power"), values.power);
                put(
                    &format!("{prefix}{number}.current"),
                    scaled(values.current, 1000.0),
                );
                // A voltage of zero beside a real current is a value that was not read.
                put(
                    &format!("{prefix}{number}.voltage"),
                    scaled(values.voltage.filter(|voltage| *voltage != 0.0), 1000.0),
                );
            }
        }
        put("energy.today", self.energy_today);
        put(
            "grid.frequency",
            scaled(self.frequency.filter(|hertz| *hertz != 0.0), 1000.0),
        );
        put("operating_time", scaled(self.operating_time_hours, 3600.0));
        put("feed_in_time", scaled(self.feed_in_time_hours, 3600.0));
        put("signal", scaled(self.signal_percent, 10.0));
        put("status", self.status.as_deref().and_then(status_code));
        put(
            "grid_relay",
            self.grid_relay.as_deref().and_then(status_code),
        );
        put("temperature", scaled(self.temperature, 1000.0));
    }
}

/// The code SMA uses for a status `SBFspot` stored as text.
///
/// A text `SBFspot` wrote in another language or for a status this list does
/// not know yields no value rather than a wrong code.
fn status_code(text: &str) -> Option<i64> {
    match text.trim().to_ascii_uppercase().as_str() {
        "OK" => Some(307),
        "WARNING" => Some(455),
        "FAULT" => Some(35),
        "OFF" => Some(303),
        "CLOSED" => Some(51),
        "OPEN" => Some(311),
        "N/A" => Some(STATUS_NOT_AVAILABLE),
        _ => None,
    }
}

/// Finds the spot values closest to an archive time, within the window. Rows
/// before the window are dropped, because times only move forward.
fn nearest(rows: &mut Vec<SpotValues>, time: i64) -> Option<&SpotValues> {
    let stale = rows
        .iter()
        .take_while(|row| row.timestamp < time - SPOT_WINDOW_SECONDS)
        .count();
    rows.drain(..stale);
    rows.iter()
        .take_while(|row| row.timestamp <= time + SPOT_WINDOW_SECONDS)
        .min_by_key(|row| (row.timestamp - time).abs())
}

/// What the lifetime counters grew by, when the same inverters reported both
/// times and no counter went back.
fn generation_energy(current: &BTreeMap<i64, i64>, previous: &BTreeMap<i64, i64>) -> Option<i64> {
    if current.is_empty() || !current.keys().eq(previous.keys()) {
        return None;
    }
    current
        .iter()
        .map(|(serial, total)| checked_delta(*total, *previous.get(serial)?))
        .try_fold(0_i64, |sum, delta| sum.checked_add(delta?))
}

fn checked_delta(current: i64, previous: i64) -> Option<i64> {
    current.checked_sub(previous).filter(|delta| *delta >= 0)
}

fn sum(values: impl Iterator<Item = i64>) -> Option<i64> {
    values.fold(None, |total: Option<i64>, value| {
        Some(total.unwrap_or(0).saturating_add(value))
    })
}

#[allow(clippy::cast_precision_loss)]
fn mean(values: &[f64]) -> Option<f64> {
    (!values.is_empty()).then(|| values.iter().sum::<f64>() / values.len() as f64)
}

/// Converts a value `SBFspot` stores as a decimal number into the integer
/// sub-unit of the device key scheme.
#[allow(clippy::cast_possible_truncation)]
fn scaled(value: Option<f64>, factor: f64) -> Option<i64> {
    value
        .map(|value| (value * factor).round())
        .filter(|value| value.is_finite() && value.abs() < 9.0e15)
        .map(|value| value as i64)
}

/// Reads a text column, treating blank text as absent.
fn text(row: &SqliteRow, column: &str) -> Result<Option<String>, SbfspotError> {
    Ok(row
        .try_get::<Option<String>, _>(column)?
        .map(|text| text.trim().to_owned())
        .filter(|text| !text.is_empty()))
}

async fn table_exists(pool: &SqlitePool, name: &str) -> Result<bool, SbfspotError> {
    sqlx::query_scalar::<_, i64>(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
    )
    .bind(name)
    .fetch_one(pool)
    .await
    .map(|value| value != 0)
    .map_err(SbfspotError::Database)
}

#[derive(Debug, Error)]
pub enum SbfspotError {
    #[error("SBFspot database does not exist: {0}")]
    DatabaseMissing(PathBuf),
    #[error("SBFspot database path is not valid UTF-8: {0}")]
    InvalidPath(PathBuf),
    #[error("SBFspot database is missing the required DayData table")]
    MissingDayData,
    #[error("batch size cannot be represented by SQLite")]
    LimitOverflow,
    #[error("SBFspot timestamp {0} cannot be converted to milliseconds")]
    TimestampOverflow(i64),
    #[error("SBFspot SQLite error: {0}")]
    Database(#[from] sqlx::Error),
}
