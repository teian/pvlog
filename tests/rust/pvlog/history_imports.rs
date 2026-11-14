//! `PVOutput` history is imported through a dry run and an explicit commit, on
//! both databases: nothing is stored before the commit, a second commit of the
//! same file duplicates nothing, and data of a live source is kept.

use std::{error::Error, sync::Arc};

use pvlog::{history_imports::StorageHistoryImportApi, ingest::StorageIngestApi};
use pvlog_api::{
    HistoryImportApiError, HistoryImportApiUseCases as _, HistoryImportResponse,
    HistoryImportResult, HistoryImportUpload, ImportOverlap, IngestApiUseCases as _, IngestWriter,
    RejectedImportRow,
};
use pvlog_application::{Clock, IngestRequest};
use pvlog_domain::{
    AccountId, DataSourceId, MembershipId, QualityFlags, SystemId, UserId, UtcTimestamp,
};
use pvlog_storage::{
    AccountConfigurationRepository as _, AccountRecord, DailySummaryRepository, DataSourceRecord,
    DataSourceRepository, DatabaseTarget, ManagementRepository, MembershipRecord,
    PostgresAccountConfigurationRepository, PostgresDailySummaryRepository,
    PostgresDataSourceRepository, PostgresManagementRepository, PostgresTelemetryRepository,
    SqliteAccountConfigurationRepository, SqliteAccountPoolConfig, SqliteAccountPoolRouter,
    SqliteAccountProvisioner, SqliteDailySummaryRepository, SqliteDataSourceRepository,
    SqliteManagementRepository, SqliteTelemetryRepository, StoredObservation,
    SystemConfigurationRecord, SystemRegistryRecord, TelemetryRepository, UserRecord,
    apply_migrations,
};
use serde_json::json;
use tempfile::TempDir;
use uuid::Uuid;

/// 2027-02-01 00:00 UTC: every day of the fixtures lies before it.
const NOW: i64 = 1_801_440_000_000;
/// 2027-01-15 07:00 UTC, 08:00 in the system's timezone.
const MORNING: i64 = 1_799_996_400_000;

/// One system with an import data source and direct access to what was stored.
struct Fixture {
    imports: StorageHistoryImportApi,
    ingest: StorageIngestApi,
    telemetry: Box<dyn TelemetryRepository>,
    summaries: Box<dyn DailySummaryRepository>,
    data_sources: Box<dyn DataSourceRepository>,
    system_id: SystemId,
    source: DataSourceId,
    _directory: Option<TempDir>,
}

impl Fixture {
    async fn sqlite() -> Result<Self, Box<dyn Error>> {
        let directory = TempDir::new()?;
        let management_path = directory.path().join("management.sqlite3");
        let accounts_dir = directory.path().join("accounts");
        let target = DatabaseTarget::Sqlite {
            management_path: management_path.clone(),
            accounts_dir: accounts_dir.clone(),
        };
        apply_migrations(&target).await?;
        let management: Arc<dyn ManagementRepository> =
            Arc::new(SqliteManagementRepository::new(management_path.clone()));
        let account_id = seed_account(management.as_ref(), "provisioning").await?;
        SqliteAccountProvisioner::new(management_path.clone(), accounts_dir.clone())
            .provision(account_id)
            .await?;
        let account = SqliteAccountPoolRouter::new(
            management_path,
            accounts_dir,
            SqliteAccountPoolConfig::default(),
        )?
        .route(account_id)
        .await?;
        let system_id = SystemId::new();
        SqliteAccountConfigurationRepository::new(account.clone())
            .save_system(&system(system_id))
            .await?;
        Self::assemble(
            target,
            management,
            account_id,
            system_id,
            Box::new(SqliteTelemetryRepository::new(account.clone())),
            Box::new(SqliteDailySummaryRepository::new(account.clone())),
            Box::new(SqliteDataSourceRepository::new(account)),
            Some(directory),
        )
        .await
    }

    async fn postgres(url: String) -> Result<Self, Box<dyn Error>> {
        let target = DatabaseTarget::Postgres { url: url.clone() };
        apply_migrations(&target).await?;
        let management: Arc<dyn ManagementRepository> =
            Arc::new(PostgresManagementRepository::new(url.clone()));
        let account_id = seed_account(management.as_ref(), "active").await?;
        let system_id = SystemId::new();
        PostgresAccountConfigurationRepository::new(url.clone(), account_id)
            .save_system(&system(system_id))
            .await?;
        Self::assemble(
            target,
            management,
            account_id,
            system_id,
            Box::new(PostgresTelemetryRepository::new(url.clone(), account_id)),
            Box::new(PostgresDailySummaryRepository::new(url.clone(), account_id)),
            Box::new(PostgresDataSourceRepository::new(url, account_id)),
            None,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn assemble(
        target: DatabaseTarget,
        management: Arc<dyn ManagementRepository>,
        account_id: AccountId,
        system_id: SystemId,
        telemetry: Box<dyn TelemetryRepository>,
        summaries: Box<dyn DailySummaryRepository>,
        data_sources: Box<dyn DataSourceRepository>,
        directory: Option<TempDir>,
    ) -> Result<Self, Box<dyn Error>> {
        management
            .save_system_registry(&SystemRegistryRecord {
                system_id,
                account_id,
                created_at: NOW,
                updated_at: NOW,
            })
            .await?;
        let fixture = Self {
            imports: StorageHistoryImportApi::new(target.clone(), management.clone(), clock()),
            ingest: StorageIngestApi::new(target, management, clock()),
            telemetry,
            summaries,
            data_sources,
            system_id,
            source: DataSourceId::new(),
            _directory: directory,
        };
        fixture.data_source(fixture.source, "import").await?;
        Ok(fixture)
    }

    async fn data_source(&self, id: DataSourceId, profile: &str) -> Result<(), Box<dyn Error>> {
        self.data_sources
            .save_data_source(&DataSourceRecord {
                id,
                system_id: self.system_id,
                name: format!("{profile} {id}"),
                profile: profile.to_owned(),
                credential_id: None,
                created_at: NOW,
                updated_at: NOW,
                deleted_at: None,
            })
            .await?;
        Ok(())
    }

    async fn dry_run(&self, fixture: &str) -> Result<HistoryImportResponse, Box<dyn Error>> {
        self.imports
            .dry_run(
                self.system_id,
                self.source,
                UserId::new(),
                HistoryImportUpload {
                    file_name: fixture.to_owned(),
                    content: read_fixture(fixture)?,
                },
            )
            .await
            .map_err(failed)
    }

    async fn commit(&self, id: Uuid) -> Result<HistoryImportResponse, Box<dyn Error>> {
        self.imports
            .commit(self.system_id, self.source, id)
            .await
            .map_err(failed)
    }

    async fn readings(&self) -> Result<Vec<StoredObservation>, Box<dyn Error>> {
        Ok(self
            .telemetry
            .observations(self.system_id, 0, NOW * 2)
            .await?)
    }

    /// Delivers values through the ingestion API, as a live logger does.
    async fn live(&self, members: serde_json::Value) -> Result<(), Box<dyn Error>> {
        let mut body = json!({ "schemaVersion": 1, "source": { "name": "smalog" } });
        if let (Some(body), Some(members)) = (body.as_object_mut(), members.as_object()) {
            body.extend(members.clone());
        }
        let request: IngestRequest = serde_json::from_value(body)?;
        self.ingest
            .ingest(
                self.system_id,
                IngestWriter {
                    credential_id: None,
                    identity: "user:logger".to_owned(),
                },
                request,
            )
            .await
            .map_err(|error| format!("ingestion failed: {error:?}"))?;
        Ok(())
    }
}

// Passed to `map_err`, which hands the error over by value.
#[allow(clippy::needless_pass_by_value)]
fn failed(error: HistoryImportApiError) -> Box<dyn Error> {
    format!("history import failed: {error:?}").into()
}

fn read_fixture(name: &str) -> Result<String, Box<dyn Error>> {
    Ok(std::fs::read_to_string(format!(
        "{}/../../../tests/fixtures/pvoutput/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))?)
}

fn result(stored: u64, already_present: u64, skipped: u64) -> HistoryImportResult {
    HistoryImportResult {
        stored,
        already_present,
        skipped,
    }
}

#[allow(clippy::too_many_lines)]
async fn statuses_are_imported_once(fixture: Fixture) -> Result<(), Box<dyn Error>> {
    // A live logger already delivered the reading at 08:15 local time.
    fixture
        .live(json!({ "measurements": [
            { "measuredAt": (MORNING + 15 * 60_000) / 1_000,
              "system": { "generationPowerW": 777 } },
        ] }))
        .await?;

    // The dry run reports what it found and stores nothing.
    let dry_run = fixture.dry_run("status-export.csv").await?;
    assert_eq!(dry_run.kind, "status");
    assert_eq!(dry_run.state, "ready");
    assert_eq!(dry_run.expires_at_epoch_millis, Some(NOW + 86_400_000));
    assert_eq!(dry_run.report.rows, 16);
    assert_eq!(dry_run.report.accepted, 9);
    assert_eq!(dry_run.report.rejected_count, 7);
    assert_eq!(
        dry_run.report.rejected[0],
        RejectedImportRow {
            row: 8,
            reason: "invalid_date".to_owned(),
            column: Some("date".to_owned()),
        }
    );
    assert_eq!(dry_run.report.first_date.as_deref(), Some("2027-01-15"));
    assert_eq!(dry_run.report.last_date.as_deref(), Some("2027-10-31"));
    assert_eq!(
        dry_run.report.overlap,
        ImportOverlap {
            already_imported: 0,
            other_sources: 1,
        }
    );
    assert_eq!(fixture.readings().await?.len(), 1);

    // Committing stores the accepted rows and keeps the live reading.
    let committed = fixture.commit(dry_run.id).await?;
    assert_eq!(committed.state, "committed");
    assert_eq!(committed.committed_at_epoch_millis, Some(NOW));
    assert_eq!(committed.expires_at_epoch_millis, None);
    assert_eq!(committed.result, Some(result(8, 0, 1)));
    let readings = fixture.readings().await?;
    assert_eq!(readings.len(), 9);
    let live = readings
        .iter()
        .find(|reading| reading.measured_at == MORNING + 15 * 60_000)
        .ok_or("the live reading is kept")?;
    assert_eq!(live.source_kind, "modern_api");
    assert_eq!(live.generation_power_watts, Some(777));
    let first = readings
        .iter()
        .find(|reading| reading.measured_at == MORNING)
        .ok_or("the first status is imported")?;
    assert_eq!(first.source_kind, "import");
    assert_eq!(first.data_source_id, Some(fixture.source));
    assert_eq!(first.generation_energy_wh, Some(10));
    assert_eq!(first.generation_power_watts, Some(120));
    assert_eq!(first.consumption_energy_wh, Some(250));
    assert_eq!(first.measurements.temperature_millidegrees_c, Some(2_500));
    assert_eq!(first.measurements.voltage_millivolts, Some(231_200));
    assert_eq!(first.provenance["origin"], "pvoutput");
    assert!(
        QualityFlags::from_bits(u16::try_from(first.quality_flags)?)?
            .contains(QualityFlags::DERIVED)
    );

    // Committing the same dry run again changes nothing.
    assert_eq!(fixture.commit(dry_run.id).await?, committed);
    // The same file uploaded and committed again duplicates nothing.
    let again = fixture.dry_run("status-export.csv").await?;
    assert_eq!(
        again.report.overlap,
        ImportOverlap {
            already_imported: 8,
            other_sources: 1,
        }
    );
    assert_eq!(
        fixture.commit(again.id).await?.result,
        Some(result(0, 8, 1))
    );
    assert_eq!(fixture.readings().await?.len(), 9);

    // Both imports are listed, newest first.
    let listed = fixture
        .imports
        .list(fixture.system_id, fixture.source)
        .await
        .map_err(failed)?;
    assert_eq!(
        listed.iter().map(|import| import.id).collect::<Vec<_>>(),
        vec![again.id, dry_run.id]
    );
    Ok(())
}

async fn days_are_imported_once(fixture: Fixture) -> Result<(), Box<dyn Error>> {
    // A live logger already summarized 2027-01-15.
    fixture
        .live(json!({ "dailySummaries": [
            { "date": "2027-01-15", "energyWh": 4_000, "complete": true },
        ] }))
        .await?;
    let dry_run = fixture.dry_run("output-service.csv").await?;
    assert_eq!(dry_run.kind, "output");
    assert_eq!(dry_run.report.accepted, 2);
    assert_eq!(dry_run.report.overlap.other_sources, 1);

    let committed = fixture.commit(dry_run.id).await?;
    assert_eq!(committed.result, Some(result(1, 0, 1)));
    let summaries = fixture
        .summaries
        .source_summaries(fixture.system_id, "2027-01-01", "2027-01-31")
        .await?;
    let imported: Vec<_> = summaries
        .iter()
        .filter(|summary| summary.data_source_id == Some(fixture.source))
        .collect();
    assert_eq!(imported.len(), 1);
    assert_eq!(imported[0].local_date, "2027-01-14");
    assert_eq!(imported[0].generation_energy_wh, Some(5_200));
    assert_eq!(imported[0].peak_time.as_deref(), Some("12:35"));
    // A past day will not change any more.
    assert!(imported[0].complete);
    // The live logger's day is kept as it was.
    assert!(
        summaries
            .iter()
            .any(|summary| summary.local_date == "2027-01-15"
                && summary.generation_energy_wh == Some(4_000))
    );

    let again = fixture.dry_run("output-export.csv").await?;
    assert_eq!(
        fixture.commit(again.id).await?.result,
        Some(result(0, 1, 1))
    );
    Ok(())
}

async fn only_import_sources_take_files(fixture: Fixture) -> Result<(), Box<dyn Error>> {
    let uploader = DataSourceId::new();
    fixture.data_source(uploader, "smalog").await?;
    let upload = || HistoryImportUpload {
        file_name: "status.csv".to_owned(),
        content: "20270115,08:00,5,0,100,100,0,NaN,NaN,NaN,NaN".to_owned(),
    };
    let attempt = |source| {
        fixture
            .imports
            .dry_run(fixture.system_id, source, UserId::new(), upload())
    };
    assert_eq!(
        attempt(uploader).await,
        Err(HistoryImportApiError::NotAnImportSource)
    );
    assert_eq!(
        attempt(DataSourceId::new()).await,
        Err(HistoryImportApiError::NotFound)
    );
    assert!(matches!(
        fixture
            .imports
            .dry_run(
                fixture.system_id,
                fixture.source,
                UserId::new(),
                HistoryImportUpload {
                    file_name: "notes.txt".to_owned(),
                    content: "hello,world".to_owned(),
                },
            )
            .await,
        Err(HistoryImportApiError::UnreadableFile(_))
    ));
    assert_eq!(
        fixture
            .imports
            .commit(fixture.system_id, fixture.source, Uuid::now_v7())
            .await,
        Err(HistoryImportApiError::NotFound)
    );
    Ok(())
}

macro_rules! on_both_databases {
    ($($scenario:ident => $sqlite:ident, $postgres:ident;)*) => {$(
        #[tokio::test]
        async fn $sqlite() -> Result<(), Box<dyn Error>> {
            $scenario(Fixture::sqlite().await?).await
        }

        #[tokio::test]
        async fn $postgres() -> Result<(), Box<dyn Error>> {
            let Ok(url) = std::env::var("TEST_POSTGRES_URL") else {
                return Ok(());
            };
            $scenario(Fixture::postgres(url).await?).await
        }
    )*};
}

on_both_databases! {
    statuses_are_imported_once =>
        sqlite_imports_statuses_once_and_keeps_live_readings,
        postgres_imports_statuses_once_and_keeps_live_readings_when_configured;
    days_are_imported_once =>
        sqlite_imports_days_once_and_keeps_live_summaries,
        postgres_imports_days_once_and_keeps_live_summaries_when_configured;
    only_import_sources_take_files =>
        sqlite_imports_only_into_import_sources,
        postgres_imports_only_into_import_sources_when_configured;
}

fn clock() -> Arc<dyn Clock> {
    Arc::new(FixedClock)
}

struct FixedClock;

impl Clock for FixedClock {
    fn now(&self) -> UtcTimestamp {
        UtcTimestamp::from_epoch_millis(NOW).unwrap_or_else(|_| unreachable!())
    }
}

async fn seed_account(
    management: &dyn ManagementRepository,
    account_status: &str,
) -> Result<AccountId, Box<dyn Error>> {
    let owner = UserId::new();
    let account_id = AccountId::new();
    management
        .save_user(&UserRecord {
            id: owner,
            email: format!("owner-{owner}@example.test"),
            display_name: "Owner".to_owned(),
            status: "active".to_owned(),
            created_at: NOW,
            updated_at: NOW,
        })
        .await?;
    management
        .save_account(&AccountRecord {
            id: account_id,
            slug: format!("import-{account_id}"),
            display_name: "Import".to_owned(),
            status: account_status.to_owned(),
            created_by: Some(owner),
            created_at: NOW,
            updated_at: NOW,
        })
        .await?;
    management
        .save_membership(&MembershipRecord {
            id: MembershipId::new(),
            account_id,
            user_id: owner,
            status: "active".to_owned(),
            joined_at: Some(NOW),
            created_at: NOW,
            updated_at: NOW,
        })
        .await?;
    Ok(account_id)
}

fn system(id: SystemId) -> SystemConfigurationRecord {
    SystemConfigurationRecord {
        id,
        name: "Import".to_owned(),
        description: String::new(),
        timezone: "Europe/Berlin".to_owned(),
        visibility: "private".to_owned(),
        lifecycle: "active".to_owned(),
        status_interval_seconds: 300,
        power_calculation_mode: "reported".to_owned(),
        net_calculation_mode: "separate_flows".to_owned(),
        created_at: NOW,
        updated_at: NOW,
    }
}
