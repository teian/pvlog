//! Supplied daily summaries and the daily values a system reports.
//!
//! A source's summary is stored next to the value computed from interval
//! readings. Which of the two a day reports is decided on every read by
//! [`pvlog_application::effective_day`], so neither is ever overwritten.

use std::{collections::BTreeMap, sync::Arc};

use async_trait::async_trait;
use pvlog_api::{
    ComputedSummaryResponse, DailySummaryApiError, DailySummaryApiUseCases, DailyValueResponse,
    SummarySource, SuppliedDeviceSummaryResponse, SuppliedSummaryResponse,
};
use pvlog_application::{
    Clock, ComputedDay, DayBasis, DayEnergy, SuppliedDailySummary, SuppliedDay, effective_day,
};
use pvlog_domain::{AccountId, SystemId};
use pvlog_storage::{
    AccountConfigurationRepository, ComputedDailySummaryRecord, DailySummaryRepository,
    DataSourceRepository, DatabaseTarget, SourceDailySummaryRecord,
};

/// Earliest and latest local dates, for reads that span a system's history.
pub const ALL_DATES: (&str, &str) = ("0001-01-01", "9999-12-31");

/// One local day with the value it reports and both of its origins.
#[derive(Clone, Debug, PartialEq)]
pub struct DailyValue {
    pub date: String,
    pub basis: DayBasis,
    pub energy: DayEnergy,
    /// The summary of the whole system: the one a source supplied, or the sum
    /// of the summaries it supplied for its devices.
    pub supplied: Option<SourceDailySummaryRecord>,
    /// The device summaries `supplied` adds up; empty when the source
    /// summarized the system itself.
    pub supplied_devices: Vec<SourceDailySummaryRecord>,
    pub computed: Option<ComputedDailySummaryRecord>,
}

/// Adds up the summaries one source supplied for its devices on one day.
///
/// Energy and lifetime counters are sums over the devices that reported them;
/// a lifetime sum exists only when every device reported its counter. The day
/// is complete once every device summary is. A peak and weather belong to one
/// device or to the site and are not combined.
fn device_sum(devices: &[SourceDailySummaryRecord]) -> Option<SourceDailySummaryRecord> {
    let first = devices.first()?;
    let sum = |value: fn(&SourceDailySummaryRecord) -> Option<i64>| {
        devices
            .iter()
            .filter_map(value)
            .fold(None, |total: Option<i64>, part| {
                Some(total.unwrap_or_default().saturating_add(part))
            })
    };
    Some(SourceDailySummaryRecord {
        system_id: first.system_id,
        source_key: first.source_key.clone(),
        device_key: String::new(),
        local_date: first.local_date.clone(),
        data_source_id: first.data_source_id,
        generation_energy_wh: sum(|record| record.generation_energy_wh),
        export_energy_wh: sum(|record| record.export_energy_wh),
        consumption_energy_wh: sum(|record| record.consumption_energy_wh),
        generation_lifetime_wh: devices
            .iter()
            .all(|record| record.generation_lifetime_wh.is_some())
            .then(|| sum(|record| record.generation_lifetime_wh))
            .flatten(),
        complete: devices.iter().all(|record| record.complete),
        peak_power_watts: None,
        peak_time: None,
        weather_condition: None,
        temperature_min_millidegrees_c: None,
        temperature_max_millidegrees_c: None,
        import_energy_by_band: serde_json::json!({}),
        received_at: devices
            .iter()
            .map(|record| record.received_at)
            .max()
            .unwrap_or(first.received_at),
    })
}

/// The repositories of one account that daily values are read from.
pub struct DailyRepositories {
    pub summaries: Box<dyn DailySummaryRepository>,
    pub sources: Box<dyn DataSourceRepository>,
    pub configuration: Box<dyn AccountConfigurationRepository>,
}

/// Opens the repositories of an account for the configured storage target.
///
/// # Errors
///
/// Returns an error when the account database cannot be routed or the storage
/// backend is not compiled in.
pub async fn daily_repositories(
    target: &DatabaseTarget,
    account_id: AccountId,
) -> Result<DailyRepositories, DailySummaryApiError> {
    match target {
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
                .map_err(|_| DailySummaryApiError::Unavailable)?
                .route(account_id)
                .await
                .map_err(|_| DailySummaryApiError::Unavailable)?;
                Ok(DailyRepositories {
                    summaries: Box::new(pvlog_storage::SqliteDailySummaryRepository::new(
                        account.clone(),
                    )),
                    sources: Box::new(pvlog_storage::SqliteDataSourceRepository::new(
                        account.clone(),
                    )),
                    configuration: Box::new(
                        pvlog_storage::SqliteAccountConfigurationRepository::new(account),
                    ),
                })
            }
            #[cfg(not(feature = "sqlite"))]
            {
                let _ = (management_path, accounts_dir, account_id);
                Err(DailySummaryApiError::Unavailable)
            }
        }
        DatabaseTarget::Postgres { url } => {
            #[cfg(feature = "postgres")]
            {
                Ok(DailyRepositories {
                    summaries: Box::new(pvlog_storage::PostgresDailySummaryRepository::new(
                        url.clone(),
                        account_id,
                    )),
                    sources: Box::new(pvlog_storage::PostgresDataSourceRepository::new(
                        url.clone(),
                        account_id,
                    )),
                    configuration: Box::new(
                        pvlog_storage::PostgresAccountConfigurationRepository::new(
                            url.clone(),
                            account_id,
                        ),
                    ),
                })
            }
            #[cfg(not(feature = "postgres"))]
            {
                let _ = (url, account_id);
                Err(DailySummaryApiError::Unavailable)
            }
        }
    }
}

/// Reads the days in `from..=to` that have a supplied or a computed value and
/// decides for each which one it reports.
///
/// A source that summarizes its devices instead of the system takes part
/// with the sum of those summaries. A summary of the system itself wins over
/// such a sum of the same standing.
///
/// # Errors
///
/// Returns an error when the summaries cannot be read.
pub async fn daily_values(
    summaries: &dyn DailySummaryRepository,
    system_id: SystemId,
    from: &str,
    to: &str,
) -> Result<Vec<DailyValue>, DailySummaryApiError> {
    let unavailable = |_| DailySummaryApiError::Unavailable;
    let (of_devices, of_systems): (Vec<_>, Vec<_>) = summaries
        .source_summaries(system_id, from, to)
        .await
        .map_err(unavailable)?
        .into_iter()
        .partition(|record| !record.device_key.is_empty());
    let mut by_source: BTreeMap<(String, String), Vec<SourceDailySummaryRecord>> = BTreeMap::new();
    for record in of_devices {
        by_source
            .entry((record.local_date.clone(), record.source_key.clone()))
            .or_default()
            .push(record);
    }
    // Device sums come first, so that among summaries of the same standing a
    // summary of the system itself is the one kept.
    let candidates = by_source
        .into_values()
        .filter_map(|devices| Some((device_sum(&devices)?, devices)))
        .chain(of_systems.into_iter().map(|record| (record, Vec::new())));
    let mut supplied: BTreeMap<String, (SourceDailySummaryRecord, Vec<SourceDailySummaryRecord>)> =
        BTreeMap::new();
    for (record, devices) in candidates {
        // Several sources may describe the same day. A complete summary beats
        // an incomplete one; among equals the one received last is kept, which
        // is the order the records arrive in.
        match supplied.get(&record.local_date) {
            Some((kept, _)) if kept.complete && !record.complete => {}
            _ => {
                supplied.insert(record.local_date.clone(), (record, devices));
            }
        }
    }
    let mut computed: BTreeMap<String, ComputedDailySummaryRecord> = summaries
        .computed_summaries(system_id, from, to)
        .await
        .map_err(unavailable)?
        .into_iter()
        .map(|record| (record.local_date.clone(), record))
        .collect();
    let dates: std::collections::BTreeSet<String> =
        supplied.keys().chain(computed.keys()).cloned().collect();
    Ok(dates
        .into_iter()
        .filter_map(|date| {
            let (supplied, supplied_devices) = supplied.remove(&date).unzip();
            let computed = computed.remove(&date);
            let (energy, basis) = effective_day(
                computed.as_ref().map(|record| ComputedDay {
                    energy: DayEnergy {
                        generation_energy_wh: record.generation_energy_wh,
                        consumption_energy_wh: record.consumption_energy_wh,
                        export_energy_wh: record.export_energy_wh,
                    },
                    coverage_basis_points: u16::try_from(
                        record.coverage_basis_points.clamp(0, 10_000),
                    )
                    .unwrap_or_default(),
                }),
                supplied.as_ref().map(|record| SuppliedDay {
                    energy: DayEnergy {
                        generation_energy_wh: record.generation_energy_wh,
                        consumption_energy_wh: record.consumption_energy_wh,
                        export_energy_wh: record.export_energy_wh,
                    },
                    complete: record.complete,
                }),
            )?;
            Some(DailyValue {
                date,
                basis,
                energy,
                supplied,
                supplied_devices: supplied_devices.unwrap_or_default(),
                computed,
            })
        })
        .collect())
}

/// Serves the daily summary endpoints from account storage.
pub struct StorageDailySummaryApi {
    target: DatabaseTarget,
    clock: Arc<dyn Clock>,
}

impl StorageDailySummaryApi {
    /// Creates the daily summary API over the given storage target and clock.
    #[must_use]
    pub const fn new(target: DatabaseTarget, clock: Arc<dyn Clock>) -> Self {
        Self { target, clock }
    }
}

#[async_trait]
impl DailySummaryApiUseCases for StorageDailySummaryApi {
    async fn supply(
        &self,
        account_id: AccountId,
        system_id: SystemId,
        source: SummarySource,
        summary: SuppliedDailySummary,
    ) -> Result<DailyValueResponse, DailySummaryApiError> {
        summary
            .validate()
            .map_err(DailySummaryApiError::InvalidField)?;
        let unavailable = |_| DailySummaryApiError::Unavailable;
        let repositories = daily_repositories(&self.target, account_id).await?;
        // The summary belongs to the data source of the credential when it has
        // one, so that regenerating a key does not start a second series.
        let data_source = match source.credential_id {
            Some(credential_id) => repositories
                .sources
                .data_source_by_credential(credential_id)
                .await
                .map_err(unavailable)?
                .filter(|record| record.system_id == system_id)
                .map(|record| record.id),
            None => None,
        };
        let received_at = i64::try_from(self.clock.now().epoch_millis())
            .map_err(|_| DailySummaryApiError::Unavailable)?;
        repositories
            .summaries
            .save_source_summary(&SourceDailySummaryRecord {
                system_id,
                source_key: data_source.map_or(source.identity, |id| id.to_string()),
                device_key: summary.device_id.clone().unwrap_or_default(),
                local_date: summary.date.clone(),
                data_source_id: data_source,
                generation_energy_wh: summary.generation_energy_wh,
                export_energy_wh: summary.export_energy_wh,
                consumption_energy_wh: summary.consumption_energy_wh,
                generation_lifetime_wh: summary.generation_lifetime_wh,
                complete: summary.complete,
                peak_power_watts: summary.peak_power_watts,
                peak_time: summary.peak_time,
                weather_condition: summary.weather_condition,
                temperature_min_millidegrees_c: summary.temperature_min_millidegrees_celsius,
                temperature_max_millidegrees_c: summary.temperature_max_millidegrees_celsius,
                import_energy_by_band: serde_json::to_value(&summary.import_energy_wh_by_band)
                    .map_err(|_| DailySummaryApiError::Unavailable)?,
                received_at,
            })
            .await
            .map_err(unavailable)?;
        daily_values(
            repositories.summaries.as_ref(),
            system_id,
            &summary.date,
            &summary.date,
        )
        .await?
        .into_iter()
        .next()
        .map(response)
        .ok_or(DailySummaryApiError::Unavailable)
    }

    async fn list(
        &self,
        account_id: AccountId,
        system_id: SystemId,
        from: String,
        to: String,
    ) -> Result<Vec<DailyValueResponse>, DailySummaryApiError> {
        let repositories = daily_repositories(&self.target, account_id).await?;
        Ok(
            daily_values(repositories.summaries.as_ref(), system_id, &from, &to)
                .await?
                .into_iter()
                .map(response)
                .collect(),
        )
    }
}

/// Renders a daily value for the API.
fn response(value: DailyValue) -> DailyValueResponse {
    DailyValueResponse {
        date: value.date,
        basis: value.basis,
        generation_energy_wh: value.energy.generation_energy_wh,
        consumption_energy_wh: value.energy.consumption_energy_wh,
        export_energy_wh: value.energy.export_energy_wh,
        supplied: value.supplied.map(|record| SuppliedSummaryResponse {
            data_source_id: record.data_source_id,
            generation_energy_wh: record.generation_energy_wh,
            export_energy_wh: record.export_energy_wh,
            consumption_energy_wh: record.consumption_energy_wh,
            generation_lifetime_wh: record.generation_lifetime_wh,
            complete: record.complete,
            peak_power_watts: record.peak_power_watts,
            peak_time: record.peak_time,
            weather_condition: record.weather_condition,
            temperature_min_millidegrees_celsius: record.temperature_min_millidegrees_c,
            temperature_max_millidegrees_celsius: record.temperature_max_millidegrees_c,
            import_energy_wh_by_band: serde_json::from_value(record.import_energy_by_band)
                .unwrap_or_default(),
            received_at_epoch_millis: record.received_at,
        }),
        supplied_devices: value
            .supplied_devices
            .into_iter()
            .map(|record| SuppliedDeviceSummaryResponse {
                device_id: record.device_key,
                generation_energy_wh: record.generation_energy_wh,
                generation_lifetime_wh: record.generation_lifetime_wh,
                complete: record.complete,
                received_at_epoch_millis: record.received_at,
            })
            .collect(),
        computed: value.computed.map(|record| ComputedSummaryResponse {
            generation_energy_wh: record.generation_energy_wh,
            consumption_energy_wh: record.consumption_energy_wh,
            export_energy_wh: record.export_energy_wh,
            coverage_basis_points: u16::try_from(record.coverage_basis_points.clamp(0, 10_000))
                .unwrap_or_default(),
        }),
    }
}
