//! Storage-backed history imports of `PVOutput` exports.
//!
//! A dry run reads the file in the system's timezone and reports what it
//! would store; the file is kept with the report. Committing reads it again,
//! leaves out every time and day another source already delivered, and writes
//! the rest as readings and daily summaries of the import data source. Both
//! are identified by source and time or date, so a second commit of the same
//! file stores nothing new.

use std::{collections::BTreeSet, sync::Arc};

use async_trait::async_trait;
use chrono::TimeZone as _;
use chrono_tz::Tz;
use pvlog_api::{
    HistoryImportApiError, HistoryImportApiUseCases, HistoryImportReport, HistoryImportResponse,
    HistoryImportResult, HistoryImportUpload, ImportOverlap, MAXIMUM_LISTED_REJECTIONS,
    RejectedImportRow,
};
use pvlog_application::{
    Clock, PvOutputFile, PvOutputFileError, PvOutputFileKind, PvOutputStatus, parse_pvoutput_export,
};
use pvlog_domain::{AccountId, DataSourceId, ObservationId, QualityFlags, SystemId, UserId};
use pvlog_storage::{
    AccountConfigurationRepository, DailySummaryRepository, DataSourceRepository, DatabaseTarget,
    HistoryImportRecord, HistoryImportRepository, IMPORT_SOURCE_KIND, IngestRepository,
    ManagementRepository, ObservationUpsert, SourceDailySummaryRecord, StoredMeasurements,
    StoredObservation, UpsertOutcome,
};
use uuid::Uuid;

/// How long a dry run waits for its commit.
const DRY_RUN_LIFETIME_MILLIS: i64 = 24 * 60 * 60 * 1_000;
/// Readings written in one transaction.
const READINGS_PER_BATCH: usize = 5_000;
/// Profile a data source needs to receive imports.
const IMPORT_PROFILE: &str = "import";

/// The account-scoped repositories an import works with.
struct Repositories {
    configuration: Box<dyn AccountConfigurationRepository>,
    data_sources: Box<dyn DataSourceRepository>,
    imports: Box<dyn HistoryImportRepository>,
    ingest: Box<dyn IngestRepository>,
    summaries: Box<dyn DailySummaryRepository>,
}

/// Serves the history import endpoints from account storage.
pub struct StorageHistoryImportApi {
    target: DatabaseTarget,
    management: Arc<dyn ManagementRepository>,
    clock: Arc<dyn Clock>,
}

fn unavailable<E>(_: E) -> HistoryImportApiError {
    HistoryImportApiError::Unavailable
}

impl StorageHistoryImportApi {
    /// Creates the import API over the given database target, management repository, and clock.
    #[must_use]
    pub const fn new(
        target: DatabaseTarget,
        management: Arc<dyn ManagementRepository>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            target,
            management,
            clock,
        }
    }

    fn now(&self) -> i64 {
        i64::try_from(self.clock.now().epoch_millis()).unwrap_or_default()
    }

    /// Opens the repositories of the system's account; an unregistered system is `NotFound`.
    async fn repositories(
        &self,
        system_id: SystemId,
    ) -> Result<Repositories, HistoryImportApiError> {
        let account_id = self
            .management
            .system_registry(system_id)
            .await
            .map_err(unavailable)?
            .ok_or(HistoryImportApiError::NotFound)?
            .account_id;
        self.account_repositories(account_id)
            .await
            .ok_or(HistoryImportApiError::Unavailable)
    }

    async fn account_repositories(&self, account_id: AccountId) -> Option<Repositories> {
        match &self.target {
            DatabaseTarget::Sqlite {
                management_path,
                accounts_dir,
            } => {
                #[cfg(feature = "sqlite")]
                {
                    let account = pvlog_storage::SqliteAccountPoolRouter::new(
                        management_path.clone(),
                        accounts_dir.clone(),
                        pvlog_storage::SqliteAccountPoolConfig::default(),
                    )
                    .ok()?
                    .route(account_id)
                    .await
                    .ok()?;
                    Some(Repositories {
                        configuration: Box::new(
                            pvlog_storage::SqliteAccountConfigurationRepository::new(
                                account.clone(),
                            ),
                        ),
                        data_sources: Box::new(pvlog_storage::SqliteDataSourceRepository::new(
                            account.clone(),
                        )),
                        imports: Box::new(pvlog_storage::SqliteHistoryImportRepository::new(
                            account.clone(),
                        )),
                        ingest: Box::new(pvlog_storage::SqliteIngestRepository::new(
                            account.clone(),
                        )),
                        summaries: Box::new(pvlog_storage::SqliteDailySummaryRepository::new(
                            account,
                        )),
                    })
                }
                #[cfg(not(feature = "sqlite"))]
                {
                    let _ = (management_path, accounts_dir, account_id);
                    None
                }
            }
            DatabaseTarget::Postgres { url } => {
                #[cfg(feature = "postgres")]
                {
                    Some(Repositories {
                        configuration: Box::new(
                            pvlog_storage::PostgresAccountConfigurationRepository::new(
                                url.clone(),
                                account_id,
                            ),
                        ),
                        data_sources: Box::new(pvlog_storage::PostgresDataSourceRepository::new(
                            url.clone(),
                            account_id,
                        )),
                        imports: Box::new(pvlog_storage::PostgresHistoryImportRepository::new(
                            url.clone(),
                            account_id,
                        )),
                        ingest: Box::new(pvlog_storage::PostgresIngestRepository::new(
                            url.clone(),
                            account_id,
                        )),
                        summaries: Box::new(pvlog_storage::PostgresDailySummaryRepository::new(
                            url.clone(),
                            account_id,
                        )),
                    })
                }
                #[cfg(not(feature = "postgres"))]
                {
                    let _ = (url, account_id);
                    None
                }
            }
        }
    }

    /// Opens the repositories and checks that the data source is a live
    /// source of the system with the import profile.
    async fn import_source(
        &self,
        system_id: SystemId,
        data_source_id: DataSourceId,
    ) -> Result<Repositories, HistoryImportApiError> {
        let repositories = self.repositories(system_id).await?;
        let source = repositories
            .data_sources
            .data_source(system_id, data_source_id)
            .await
            .map_err(unavailable)?
            .filter(|source| source.deleted_at.is_none())
            .ok_or(HistoryImportApiError::NotFound)?;
        if source.profile != IMPORT_PROFILE {
            return Err(HistoryImportApiError::NotAnImportSource);
        }
        Ok(repositories)
    }

    /// The timezone the local dates and times of the system's files are in.
    async fn timezone(
        repositories: &Repositories,
        system_id: SystemId,
    ) -> Result<Tz, HistoryImportApiError> {
        Ok(repositories
            .configuration
            .system(system_id)
            .await
            .map_err(unavailable)?
            .ok_or(HistoryImportApiError::NotFound)?
            .timezone
            .parse()
            .unwrap_or(Tz::UTC))
    }
}

/// Name under which an import source's readings and summaries are stored.
fn source_key(data_source_id: DataSourceId) -> String {
    data_source_id.to_string()
}

fn read_file(content: &str, timezone: Tz) -> Result<PvOutputFile, HistoryImportApiError> {
    parse_pvoutput_export(content, timezone).map_err(|error| {
        HistoryImportApiError::UnreadableFile(match error {
            PvOutputFileError::Empty => "the file contains no rows".to_owned(),
            PvOutputFileError::MissingColumn(column) => {
                format!("the header names no {column} column")
            }
            PvOutputFileError::UnknownLayout => {
                "the file is neither a PVOutput status nor an output export".to_owned()
            }
        })
    })
}

/// Which rows of a file already have stored data: by time for statuses, by
/// date for days. The first set holds what this source stored, the second
/// what other sources did.
struct Occupied {
    own: BTreeSet<String>,
    others: BTreeSet<String>,
}

impl Occupied {
    fn of(&self, key: &str) -> Option<bool> {
        if self.others.contains(key) {
            Some(false)
        } else if self.own.contains(key) {
            Some(true)
        } else {
            None
        }
    }
}

/// Key of a status in [`Occupied`].
fn status_key(status: &PvOutputStatus) -> String {
    status.observed_at_epoch_millis.to_string()
}

/// Finds the stored data the rows of a file meet.
async fn occupied(
    repositories: &Repositories,
    system_id: SystemId,
    data_source_id: DataSourceId,
    file: &PvOutputFile,
) -> Result<Occupied, HistoryImportApiError> {
    let own_key = source_key(data_source_id);
    let mut occupied = Occupied {
        own: BTreeSet::new(),
        others: BTreeSet::new(),
    };
    match file.kind {
        PvOutputFileKind::Status => {
            let times = || file.statuses.iter().map(|s| s.observed_at_epoch_millis);
            let (Some(from), Some(to)) = (times().min(), times().max()) else {
                return Ok(occupied);
            };
            for (at, kind, identity) in repositories
                .ingest
                .reading_sources(system_id, from, to)
                .await
                .map_err(unavailable)?
            {
                if kind == IMPORT_SOURCE_KIND && identity == own_key {
                    occupied.own.insert(at.to_string());
                } else {
                    occupied.others.insert(at.to_string());
                }
            }
        }
        PvOutputFileKind::Output => {
            let (Some(first), Some(last)) = (file.days.first(), file.days.last()) else {
                return Ok(occupied);
            };
            for summary in repositories
                .summaries
                .source_summaries(system_id, &first.summary.date, &last.summary.date)
                .await
                .map_err(unavailable)?
            {
                if !summary.device_key.is_empty() {
                    continue;
                }
                if summary.source_key == own_key {
                    occupied.own.insert(summary.local_date);
                } else {
                    occupied.others.insert(summary.local_date);
                }
            }
        }
    }
    Ok(occupied)
}

/// Builds the report of a dry run.
fn report(file: &PvOutputFile, occupied: &Occupied) -> HistoryImportReport {
    let count = |value: usize| u64::try_from(value).unwrap_or(u64::MAX);
    let mut overlap = ImportOverlap::default();
    let keys: Vec<String> = match file.kind {
        PvOutputFileKind::Status => file.statuses.iter().map(status_key).collect(),
        PvOutputFileKind::Output => file.days.iter().map(|d| d.summary.date.clone()).collect(),
    };
    for key in &keys {
        match occupied.of(key) {
            Some(true) => overlap.already_imported += 1,
            Some(false) => overlap.other_sources += 1,
            None => {}
        }
    }
    let dates: Vec<&str> = match file.kind {
        PvOutputFileKind::Status => file
            .statuses
            .iter()
            .map(|s| s.local_date.as_str())
            .collect(),
        PvOutputFileKind::Output => file.days.iter().map(|d| d.summary.date.as_str()).collect(),
    };
    HistoryImportReport {
        rows: count(file.rows),
        accepted: count(keys.len()),
        rejected_count: count(file.rejected.len()),
        rejected: file
            .rejected
            .iter()
            .take(MAXIMUM_LISTED_REJECTIONS)
            .map(|rejection| RejectedImportRow {
                row: count(rejection.row),
                reason: rejection.reason.to_owned(),
                column: rejection.column.map(ToOwned::to_owned),
            })
            .collect(),
        first_date: dates.iter().min().map(|date| (*date).to_owned()),
        last_date: dates.iter().max().map(|date| (*date).to_owned()),
        overlap,
    }
}

fn kind_name(kind: PvOutputFileKind) -> &'static str {
    match kind {
        PvOutputFileKind::Status => "status",
        PvOutputFileKind::Output => "output",
    }
}

fn response(record: HistoryImportRecord) -> HistoryImportResponse {
    let committed = record.state == "committed";
    HistoryImportResponse {
        id: record.id,
        data_source_id: record.data_source_id.as_uuid(),
        file_name: record.file_name,
        kind: record.file_kind,
        state: record.state,
        created_at_epoch_millis: record.created_at,
        expires_at_epoch_millis: (!committed).then_some(record.expires_at),
        committed_at_epoch_millis: record.committed_at,
        report: serde_json::from_value(record.report).unwrap_or_default(),
        result: record
            .result
            .and_then(|result| serde_json::from_value(result).ok()),
    }
}

/// A status as a reading of the import source. Its interval energy is
/// derived from the energy since midnight the file states.
fn imported_reading(
    status: &PvOutputStatus,
    system_id: SystemId,
    data_source_id: DataSourceId,
    import_id: Uuid,
    received_at: i64,
) -> ObservationUpsert {
    let mut flags = QualityFlags::NONE;
    if status.generation_energy_wh.is_some() || status.consumption_energy_wh.is_some() {
        flags = flags | QualityFlags::DERIVED;
    }
    if status.counter_reset {
        flags = flags | QualityFlags::COUNTER_RESET;
    }
    let quality_flags = i32::from(flags.bits());
    // The hash covers what a reader can see, so that the same row imported
    // again is recognized as stored.
    let canonical_hash = *blake3::hash(
        serde_json::json!({
            "generationPowerWatts": status.generation_power_watts,
            "generationEnergyWh": status.generation_energy_wh,
            "consumptionPowerWatts": status.consumption_power_watts,
            "consumptionEnergyWh": status.consumption_energy_wh,
            "temperature": status.temperature_millidegrees_celsius,
            "voltage": status.voltage_millivolts,
            "qualityFlags": quality_flags,
        })
        .to_string()
        .as_bytes(),
    )
    .as_bytes();
    ObservationUpsert {
        observation: StoredObservation {
            id: ObservationId::new(),
            system_id,
            measured_at: status.observed_at_epoch_millis,
            received_at,
            source_kind: IMPORT_SOURCE_KIND.to_owned(),
            source_identity: source_key(data_source_id),
            idempotency_identity: None,
            quality_flags,
            generation_power_watts: status.generation_power_watts,
            generation_energy_wh: status.generation_energy_wh,
            consumption_power_watts: status.consumption_power_watts,
            consumption_energy_wh: status.consumption_energy_wh,
            provenance: serde_json::json!({
                "sourceKind": IMPORT_SOURCE_KIND,
                "origin": "pvoutput",
                "importId": import_id,
                "row": status.row,
            }),
            canonical_hash,
            version: 1,
            data_source_id: Some(data_source_id),
            measurements: StoredMeasurements {
                temperature_millidegrees_c: status.temperature_millidegrees_celsius.map(i64::from),
                voltage_millivolts: status.voltage_millivolts.map(i64::from),
                ..StoredMeasurements::default()
            },
        },
        channel_values: Vec::new(),
        fills_gap: false,
    }
}

/// Writes the statuses no other source covers.
async fn commit_statuses(
    repositories: &Repositories,
    record: &HistoryImportRecord,
    file: &PvOutputFile,
    occupied: &Occupied,
    received_at: i64,
) -> Result<HistoryImportResult, HistoryImportApiError> {
    let mut result = HistoryImportResult::default();
    let mut readings = Vec::new();
    for status in &file.statuses {
        if occupied.of(&status_key(status)) == Some(false) {
            result.skipped += 1;
        } else {
            readings.push(imported_reading(
                status,
                record.system_id,
                record.data_source_id,
                record.id,
                received_at,
            ));
        }
    }
    for batch in readings.chunks(READINGS_PER_BATCH) {
        for outcome in repositories
            .ingest
            .upsert_observations(batch)
            .await
            .map_err(unavailable)?
        {
            match outcome {
                UpsertOutcome::Inserted | UpsertOutcome::Replaced => result.stored += 1,
                UpsertOutcome::Unchanged | UpsertOutcome::Kept => result.already_present += 1,
            }
        }
    }
    Ok(result)
}

/// Writes the days no other source covers. A day before today in the
/// system's timezone will not change any more and is complete.
async fn commit_days(
    repositories: &Repositories,
    record: &HistoryImportRecord,
    file: &PvOutputFile,
    occupied: &Occupied,
    timezone: Tz,
    received_at: i64,
) -> Result<HistoryImportResult, HistoryImportApiError> {
    let today = timezone
        .timestamp_millis_opt(received_at)
        .single()
        .map(|now| now.format("%Y-%m-%d").to_string())
        .unwrap_or_default();
    let mut result = HistoryImportResult::default();
    let mut summaries = Vec::new();
    for day in &file.days {
        let summary = &day.summary;
        if occupied.of(&summary.date) == Some(false) {
            result.skipped += 1;
            continue;
        }
        summaries.push(SourceDailySummaryRecord {
            system_id: record.system_id,
            source_key: source_key(record.data_source_id),
            device_key: String::new(),
            local_date: summary.date.clone(),
            data_source_id: Some(record.data_source_id),
            generation_energy_wh: summary.generation_energy_wh,
            export_energy_wh: summary.export_energy_wh,
            consumption_energy_wh: summary.consumption_energy_wh,
            generation_lifetime_wh: None,
            complete: summary.date < today,
            peak_power_watts: summary.peak_power_watts,
            peak_time: summary.peak_time.clone(),
            weather_condition: summary.weather_condition.clone(),
            temperature_min_millidegrees_c: summary.temperature_min_millidegrees_celsius,
            temperature_max_millidegrees_c: summary.temperature_max_millidegrees_celsius,
            import_energy_by_band: serde_json::to_value(&summary.import_energy_wh_by_band)
                .map_err(unavailable)?,
            received_at,
        });
    }
    if !summaries.is_empty() {
        let stored = repositories
            .ingest
            .save_daily_summaries(&summaries)
            .await
            .map_err(unavailable)?;
        let total = u64::try_from(summaries.len()).unwrap_or(u64::MAX);
        result.stored = stored;
        result.already_present = total.saturating_sub(stored);
    }
    Ok(result)
}

#[async_trait]
impl HistoryImportApiUseCases for StorageHistoryImportApi {
    async fn dry_run(
        &self,
        system_id: SystemId,
        data_source_id: DataSourceId,
        requested_by: UserId,
        upload: HistoryImportUpload,
    ) -> Result<HistoryImportResponse, HistoryImportApiError> {
        let repositories = self.import_source(system_id, data_source_id).await?;
        let timezone = Self::timezone(&repositories, system_id).await?;
        let file = read_file(&upload.content, timezone)?;
        let occupied = occupied(&repositories, system_id, data_source_id, &file).await?;
        let now = self.now();
        let record = HistoryImportRecord {
            id: Uuid::now_v7(),
            system_id,
            data_source_id,
            requested_by: Some(requested_by.as_uuid()),
            file_name: upload.file_name,
            file_kind: kind_name(file.kind).to_owned(),
            content_hash: *blake3::hash(upload.content.as_bytes()).as_bytes(),
            content: Some(upload.content.into_bytes()),
            state: "ready".to_owned(),
            report: serde_json::to_value(report(&file, &occupied)).map_err(unavailable)?,
            result: None,
            created_at: now,
            committed_at: None,
            expires_at: now + DRY_RUN_LIFETIME_MILLIS,
        };
        repositories
            .imports
            .save_history_import(&record)
            .await
            .map_err(unavailable)?;
        Ok(response(HistoryImportRecord {
            content: None,
            ..record
        }))
    }

    async fn commit(
        &self,
        system_id: SystemId,
        data_source_id: DataSourceId,
        import_id: Uuid,
    ) -> Result<HistoryImportResponse, HistoryImportApiError> {
        let repositories = self.import_source(system_id, data_source_id).await?;
        let now = self.now();
        let record = repositories
            .imports
            .history_import(system_id, import_id, now)
            .await
            .map_err(unavailable)?
            .filter(|record| record.data_source_id == data_source_id)
            .ok_or(HistoryImportApiError::NotFound)?;
        let Some(content) = record
            .content
            .as_deref()
            .filter(|_| record.state == "ready")
        else {
            return Ok(response(record));
        };
        let timezone = Self::timezone(&repositories, system_id).await?;
        let content = String::from_utf8(content.to_vec()).map_err(unavailable)?;
        let file = read_file(&content, timezone)?;
        // Data that arrived since the dry run counts as well.
        let occupied = occupied(&repositories, system_id, data_source_id, &file).await?;
        let result = match file.kind {
            PvOutputFileKind::Status => {
                commit_statuses(&repositories, &record, &file, &occupied, now).await?
            }
            PvOutputFileKind::Output => {
                commit_days(&repositories, &record, &file, &occupied, timezone, now).await?
            }
        };
        let result_json = serde_json::to_value(result).map_err(unavailable)?;
        repositories
            .imports
            .commit_history_import(system_id, import_id, &result_json, now)
            .await
            .map_err(unavailable)?;
        // A commit that ran at the same time may have recorded its result first.
        repositories
            .imports
            .history_import(system_id, import_id, now)
            .await
            .map_err(unavailable)?
            .map(response)
            .ok_or(HistoryImportApiError::NotFound)
    }

    async fn list(
        &self,
        system_id: SystemId,
        data_source_id: DataSourceId,
    ) -> Result<Vec<HistoryImportResponse>, HistoryImportApiError> {
        let repositories = self.import_source(system_id, data_source_id).await?;
        Ok(repositories
            .imports
            .history_imports(system_id, self.now())
            .await
            .map_err(unavailable)?
            .into_iter()
            .filter(|record| record.data_source_id == data_source_id)
            .map(response)
            .collect())
    }

    async fn get(
        &self,
        system_id: SystemId,
        data_source_id: DataSourceId,
        import_id: Uuid,
    ) -> Result<HistoryImportResponse, HistoryImportApiError> {
        let repositories = self.import_source(system_id, data_source_id).await?;
        repositories
            .imports
            .history_import(system_id, import_id, self.now())
            .await
            .map_err(unavailable)?
            .filter(|record| record.data_source_id == data_source_id)
            .map(|record| {
                response(HistoryImportRecord {
                    content: None,
                    ..record
                })
            })
            .ok_or(HistoryImportApiError::NotFound)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_time_another_source_holds_wins_over_one_this_source_holds() {
        let occupied = Occupied {
            own: BTreeSet::from(["1".to_owned(), "2".to_owned()]),
            others: BTreeSet::from(["2".to_owned()]),
        };
        assert_eq!(occupied.of("1"), Some(true));
        assert_eq!(occupied.of("2"), Some(false));
        assert_eq!(occupied.of("3"), None);
    }
}
