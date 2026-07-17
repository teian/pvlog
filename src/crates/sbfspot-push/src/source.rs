use serde::Serialize;
use sqlx::{
    Row, SqlitePool,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};
use std::{
    path::{Path, PathBuf},
    str::FromStr,
    time::Duration,
};
use thiserror::Error;

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
}

#[derive(Clone, Debug)]
pub struct SbfspotSource {
    pool: SqlitePool,
    has_consumption: bool,
    has_spot_data: bool,
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
        let has_consumption = table_exists(&pool, "Consumption").await?;
        let has_spot_data = table_exists(&pool, "SpotData").await?;
        Ok(Self {
            pool,
            has_consumption,
            has_spot_data,
        })
    }

    /// Reads one ordered page and converts cumulative `SBFspot` counters to interval energy.
    ///
    /// # Errors
    /// Returns an error for malformed values, overflow, or `SQLite` failures.
    pub async fn read_after(
        &self,
        timestamp: i64,
        limit: usize,
    ) -> Result<Vec<Reading>, SbfspotError> {
        let limit = i64::try_from(limit).map_err(|_| SbfspotError::LimitOverflow)?;
        let sql = reading_query(self.has_consumption, self.has_spot_data);
        let rows = sqlx::query(sql)
            .bind(timestamp)
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .map_err(SbfspotError::Database)?;
        if rows.is_empty() {
            return Ok(Vec::new());
        }

        let first_timestamp: i64 = rows[0].try_get("timestamp")?;
        let mut previous = self.baseline_before(first_timestamp).await?;
        let mut readings = Vec::with_capacity(rows.len());
        for row in rows {
            let current = RawReading {
                timestamp: row.try_get("timestamp")?,
                generation_power: row.try_get("generation_power")?,
                generation_total: row.try_get("generation_total")?,
                inverter_count: row.try_get("inverter_count")?,
                consumption_power: row.try_get("consumption_power")?,
                consumption_total: row.try_get("consumption_total")?,
                voltage: row.try_get("voltage")?,
                temperature: row.try_get("temperature")?,
            };
            readings.push(to_reading(&current, previous.as_ref())?);
            previous = Some(Baseline::from(&current));
        }
        Ok(readings)
    }

    async fn baseline_before(&self, timestamp: i64) -> Result<Option<Baseline>, SbfspotError> {
        let sql = if self.has_consumption {
            "SELECT SUM(TotalYield) AS generation_total, COUNT(*) AS inverter_count, \
             (SELECT EnergyUsed FROM Consumption WHERE TimeStamp < ?1 ORDER BY TimeStamp DESC LIMIT 1) AS consumption_total \
             FROM DayData WHERE TimeStamp = (SELECT MAX(TimeStamp) FROM DayData WHERE TimeStamp < ?1)"
        } else {
            "SELECT SUM(TotalYield) AS generation_total, COUNT(*) AS inverter_count, \
             NULL AS consumption_total FROM DayData \
             WHERE TimeStamp = (SELECT MAX(TimeStamp) FROM DayData WHERE TimeStamp < ?1)"
        };
        let row = sqlx::query(sql)
            .bind(timestamp)
            .fetch_one(&self.pool)
            .await
            .map_err(SbfspotError::Database)?;
        let generation_total: Option<i64> = row.try_get("generation_total")?;
        Ok(generation_total.map(|generation_total| Baseline {
            generation_total,
            inverter_count: row.try_get("inverter_count").unwrap_or(0),
            consumption_total: row.try_get("consumption_total").unwrap_or(None),
        }))
    }
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

fn reading_query(has_consumption: bool, has_spot_data: bool) -> &'static str {
    const FULL: &str = concat!(
        "WITH generation AS (SELECT TimeStamp AS timestamp, SUM(Power) AS generation_power, ",
        "SUM(TotalYield) AS generation_total, COUNT(*) AS inverter_count FROM DayData ",
        "WHERE TimeStamp > ?1 GROUP BY TimeStamp ORDER BY TimeStamp ASC LIMIT ?2) ",
        "SELECT g.timestamp, g.generation_power, g.generation_total, g.inverter_count, ",
        "c.PowerUsed AS consumption_power, c.EnergyUsed AS consumption_total, ",
        "(SELECT AVG(NULLIF(s.Uac1, 0)) FROM SpotData s WHERE s.TimeStamp BETWEEN g.timestamp - 150 AND g.timestamp + 150) AS voltage, ",
        "(SELECT AVG(s.Temperature) FROM SpotData s WHERE s.TimeStamp BETWEEN g.timestamp - 150 AND g.timestamp + 150) AS temperature ",
        "FROM generation g LEFT JOIN Consumption c ON c.TimeStamp = g.timestamp ORDER BY g.timestamp ASC"
    );
    const CONSUMPTION_ONLY: &str = concat!(
        "WITH generation AS (SELECT TimeStamp AS timestamp, SUM(Power) AS generation_power, ",
        "SUM(TotalYield) AS generation_total, COUNT(*) AS inverter_count FROM DayData ",
        "WHERE TimeStamp > ?1 GROUP BY TimeStamp ORDER BY TimeStamp ASC LIMIT ?2) ",
        "SELECT g.timestamp, g.generation_power, g.generation_total, g.inverter_count, ",
        "c.PowerUsed AS consumption_power, c.EnergyUsed AS consumption_total, ",
        "NULL AS voltage, NULL AS temperature FROM generation g ",
        "LEFT JOIN Consumption c ON c.TimeStamp = g.timestamp ORDER BY g.timestamp ASC"
    );
    const SPOT_ONLY: &str = concat!(
        "WITH generation AS (SELECT TimeStamp AS timestamp, SUM(Power) AS generation_power, ",
        "SUM(TotalYield) AS generation_total, COUNT(*) AS inverter_count FROM DayData ",
        "WHERE TimeStamp > ?1 GROUP BY TimeStamp ORDER BY TimeStamp ASC LIMIT ?2) ",
        "SELECT g.timestamp, g.generation_power, g.generation_total, g.inverter_count, ",
        "NULL AS consumption_power, NULL AS consumption_total, ",
        "(SELECT AVG(NULLIF(s.Uac1, 0)) FROM SpotData s WHERE s.TimeStamp BETWEEN g.timestamp - 150 AND g.timestamp + 150) AS voltage, ",
        "(SELECT AVG(s.Temperature) FROM SpotData s WHERE s.TimeStamp BETWEEN g.timestamp - 150 AND g.timestamp + 150) AS temperature ",
        "FROM generation g ORDER BY g.timestamp ASC"
    );
    const GENERATION_ONLY: &str = concat!(
        "WITH generation AS (SELECT TimeStamp AS timestamp, SUM(Power) AS generation_power, ",
        "SUM(TotalYield) AS generation_total, COUNT(*) AS inverter_count FROM DayData ",
        "WHERE TimeStamp > ?1 GROUP BY TimeStamp ORDER BY TimeStamp ASC LIMIT ?2) ",
        "SELECT g.timestamp, g.generation_power, g.generation_total, g.inverter_count, ",
        "NULL AS consumption_power, NULL AS consumption_total, NULL AS voltage, NULL AS temperature ",
        "FROM generation g ORDER BY g.timestamp ASC"
    );
    match (has_consumption, has_spot_data) {
        (true, true) => FULL,
        (true, false) => CONSUMPTION_ONLY,
        (false, true) => SPOT_ONLY,
        (false, false) => GENERATION_ONLY,
    }
}

#[derive(Debug)]
struct RawReading {
    timestamp: i64,
    generation_power: Option<i64>,
    generation_total: i64,
    inverter_count: i64,
    consumption_power: Option<i64>,
    consumption_total: Option<i64>,
    voltage: Option<f64>,
    temperature: Option<f64>,
}

#[derive(Clone, Copy, Debug)]
struct Baseline {
    generation_total: i64,
    inverter_count: i64,
    consumption_total: Option<i64>,
}

impl From<&RawReading> for Baseline {
    fn from(reading: &RawReading) -> Self {
        Self {
            generation_total: reading.generation_total,
            inverter_count: reading.inverter_count,
            consumption_total: reading.consumption_total,
        }
    }
}

fn to_reading(current: &RawReading, previous: Option<&Baseline>) -> Result<Reading, SbfspotError> {
    let generation_energy = previous.and_then(|previous| {
        (previous.inverter_count == current.inverter_count)
            .then(|| checked_delta(current.generation_total, previous.generation_total))
            .flatten()
    });
    let consumption_energy = previous
        .and_then(|previous| previous.consumption_total)
        .zip(current.consumption_total)
        .and_then(|(previous, current)| checked_delta(current, previous));
    let observed_at_epoch_millis = current
        .timestamp
        .checked_mul(1000)
        .ok_or(SbfspotError::TimestampOverflow(current.timestamp))?;
    Ok(Reading {
        timestamp: current.timestamp,
        observed_at_epoch_millis,
        generation_power_watts: current.generation_power,
        generation_energy_wh: generation_energy,
        consumption_power_watts: current.consumption_power,
        consumption_energy_wh: consumption_energy,
        voltage_millivolts: scaled_u32(current.voltage, 1000.0),
        temperature_millidegrees_celsius: scaled_i32(current.temperature, 1000.0),
        source_reference: format!("sbfspot:daydata:{}", current.timestamp),
    })
}

fn checked_delta(current: i64, previous: i64) -> Option<i64> {
    current.checked_sub(previous).filter(|delta| *delta >= 0)
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn scaled_u32(value: Option<f64>, scale: f64) -> Option<u32> {
    value
        .map(|value| value * scale)
        .filter(|value| value.is_finite() && *value >= 0.0 && *value <= f64::from(u32::MAX))
        .map(|value| value.round() as u32)
}

#[allow(clippy::cast_possible_truncation)]
fn scaled_i32(value: Option<f64>, scale: f64) -> Option<i32> {
    value
        .map(|value| value * scale)
        .filter(|value| {
            value.is_finite() && *value >= f64::from(i32::MIN) && *value <= f64::from(i32::MAX)
        })
        .map(|value| value.round() as i32)
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
