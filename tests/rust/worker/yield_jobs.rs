use std::{
    error::Error,
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use pvlog_domain::{AccountId, ProviderId, SystemId, TimeRange, UtcTimestamp, WeatherDataRunId};
use pvlog_storage::{
    AccountConfigurationRepository, DatabaseTarget, OperationalRepository,
    SqliteAccountConfigurationRepository, SqliteAccountPoolConfig, SqliteAccountPoolRouter,
    SqliteAccountProvisioner, SqliteOperationalRepository, SqliteYieldResultRepository,
    SystemConfigurationRecord, WeatherRunInsertOutcome, YieldInvalidationReason,
    YieldInvalidationRecord, YieldInvalidationState, YieldResultRepository, apply_migrations,
};
use pvlog_worker::{
    WeatherPollJobPayload, YieldCalculationJobPayload, YieldJobCoordinator, YieldJobExecutionError,
    YieldJobHandler, YieldJobOutcome, YieldJobPolicy, YieldRebuildJobPayload,
};
use sqlx::{Connection as _, SqliteConnection, sqlite::SqliteConnectOptions};
use tempfile::TempDir;
use uuid::Uuid;

#[tokio::test]
async fn yield_jobs_are_idempotent_retry_bounded_and_coalesce_invalidations()
-> Result<(), Box<dyn Error>> {
    let directory = TempDir::new()?;
    let management = directory.path().join("management.sqlite3");
    let accounts = directory.path().join("accounts");
    apply_migrations(&DatabaseTarget::Sqlite {
        management_path: management.clone(),
        accounts_dir: accounts.clone(),
    })
    .await?;
    let account_id = create_account(&management, &accounts).await?;
    let router = SqliteAccountPoolRouter::new(
        management.clone(),
        accounts,
        SqliteAccountPoolConfig::default(),
    )?;
    let account_pool = router.route(account_id).await?;
    let operational = Arc::new(SqliteOperationalRepository::new(
        management,
        account_pool.clone(),
    ));
    let results = SqliteYieldResultRepository::new(account_pool.clone());
    let configuration = SqliteAccountConfigurationRepository::new(account_pool);
    let system_id = SystemId::new();
    configuration.save_system(&system(system_id)).await?;

    let coordinator = YieldJobCoordinator::new(
        operational.clone(),
        YieldJobPolicy {
            lease_milliseconds: 1_000,
            base_retry_milliseconds: 100,
            maximum_retry_milliseconds: 1_000,
            maximum_attempts: 2,
            concurrency: 1,
        },
    )?;
    let payload = WeatherPollJobPayload {
        provider_id: ProviderId::new(),
        system_id,
        range_start: 1_000,
        range_end: 2_000,
    };
    let first = coordinator
        .enqueue_weather_poll(payload.clone(), "poll-1".to_owned(), 10)
        .await?;
    let duplicate = coordinator
        .enqueue_weather_poll(payload, "poll-1".to_owned(), 10)
        .await?;
    assert_eq!(first, duplicate);

    let handler = TestHandler::failing(2);
    let retry_at = match coordinator.execute_one("worker-a", 10, &handler).await? {
        Some(YieldJobOutcome::RetryAt(retry_at)) => retry_at,
        outcome => panic!("expected retry outcome, got {outcome:?}"),
    };
    assert!((110..=135).contains(&retry_at));
    assert_eq!(
        coordinator
            .execute_one("worker-a", retry_at, &handler)
            .await?,
        Some(YieldJobOutcome::DeadLetter)
    );
    assert_eq!(operational.dead_letter_jobs(10).await?.len(), 1);

    let calculation_id = coordinator
        .enqueue_yield_calculation(
            YieldCalculationJobPayload {
                system_id,
                weather_run_id: WeatherDataRunId::new(),
                range_start: 1_000,
                range_end: 2_000,
                configuration_digest: [7; 32],
            },
            "calculate-1".to_owned(),
            200,
        )
        .await?;
    let success = TestHandler::failing(0);
    assert_eq!(
        coordinator.execute_one("worker-a", 200, &success).await?,
        Some(YieldJobOutcome::Completed)
    );
    assert_eq!(
        operational.job(calculation_id).await?.map(|job| job.state),
        Some("completed".to_owned())
    );

    insert_invalidations(&results, system_id).await?;
    let rebuild_id = coordinator
        .enqueue_pending_rebuild(&results, system_id, range(900, 3_100)?, 100, 300)
        .await?
        .ok_or("missing rebuild")?;
    let rebuild = operational
        .job(rebuild_id)
        .await?
        .ok_or("missing rebuild job")?;
    let payload: YieldRebuildJobPayload = serde_json::from_value(rebuild.payload)?;
    assert_eq!((payload.range_start, payload.range_end), (1_000, 3_000));
    assert_eq!(payload.invalidation_ids.len(), 2);
    Ok(())
}

async fn insert_invalidations(
    results: &SqliteYieldResultRepository,
    system_id: SystemId,
) -> Result<(), Box<dyn Error>> {
    for (reason, start, end, key) in [
        (YieldInvalidationReason::Settings, 1_000, 2_000, "settings"),
        (
            YieldInvalidationReason::Correction,
            1_500,
            3_000,
            "correction",
        ),
    ] {
        assert_eq!(
            results
                .insert_invalidation(&YieldInvalidationRecord {
                    id: Uuid::now_v7(),
                    system_id,
                    range: range(start, end)?,
                    reason,
                    state: YieldInvalidationState::Pending,
                    idempotency_key: key.to_owned(),
                    created_at: start,
                    completed_at: None,
                })
                .await?,
            WeatherRunInsertOutcome::Inserted
        );
    }
    Ok(())
}

struct TestHandler {
    failures: Mutex<usize>,
}

impl TestHandler {
    fn failing(count: usize) -> Self {
        Self {
            failures: Mutex::new(count),
        }
    }

    fn outcome(&self) -> Result<(), YieldJobExecutionError> {
        let mut failures = self
            .failures
            .lock()
            .map_err(|_| YieldJobExecutionError { safe_code: "lock" })?;
        if *failures == 0 {
            Ok(())
        } else {
            *failures -= 1;
            Err(YieldJobExecutionError {
                safe_code: "provider_unavailable",
            })
        }
    }
}

#[async_trait]
impl YieldJobHandler for TestHandler {
    async fn poll_weather(&self, _: WeatherPollJobPayload) -> Result<(), YieldJobExecutionError> {
        self.outcome()
    }
    async fn calculate_yield(
        &self,
        _: YieldCalculationJobPayload,
    ) -> Result<(), YieldJobExecutionError> {
        self.outcome()
    }
    async fn rebuild_yield_intervals(
        &self,
        _: YieldRebuildJobPayload,
    ) -> Result<(), YieldJobExecutionError> {
        self.outcome()
    }
}

fn system(id: SystemId) -> SystemConfigurationRecord {
    SystemConfigurationRecord {
        id,
        name: "Worker forecast".to_owned(),
        description: String::new(),
        timezone: "UTC".to_owned(),
        visibility: "private".to_owned(),
        lifecycle: "active".to_owned(),
        status_interval_seconds: 300,
        power_calculation_mode: "reported".to_owned(),
        net_calculation_mode: "separate_flows".to_owned(),
        created_at: 1,
        updated_at: 1,
    }
}

fn range(start: i64, end: i64) -> Result<TimeRange, Box<dyn Error>> {
    Ok(TimeRange::new(
        UtcTimestamp::from_epoch_millis(start)?,
        UtcTimestamp::from_epoch_millis(end)?,
    )?)
}

async fn create_account(
    management_path: &std::path::Path,
    accounts_dir: &std::path::Path,
) -> Result<AccountId, Box<dyn Error>> {
    let account_id = AccountId::new();
    let mut management = SqliteConnection::connect_with(
        &SqliteConnectOptions::new()
            .filename(management_path)
            .create_if_missing(false)
            .foreign_keys(true),
    )
    .await?;
    sqlx::query("INSERT INTO accounts (id,slug,display_name,status,created_at,updated_at) VALUES (?,?,?,'provisioning',1,1)")
        .bind(account_id.as_uuid().as_bytes().as_slice())
        .bind(format!("worker-{account_id}"))
        .bind("Worker forecast")
        .execute(&mut management).await?;
    management.close().await?;
    SqliteAccountProvisioner::new(management_path.to_owned(), accounts_dir.to_owned())
        .provision(account_id)
        .await?;
    Ok(account_id)
}
