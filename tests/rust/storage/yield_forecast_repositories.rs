//! Cross-engine contracts for forecast inputs, immutable results, invalidation, and retention.

use std::error::Error;

use pvlog_domain::{
    AccountId, CalculationBasis, EquipmentValueProvenance, EstimateRange, ForecastCompleteness,
    ForecastCompletenessReason, ForecastSettingsId, GeographicPoint, InverterId, IrradiancePoint,
    ModelVersion, NormalizedWeatherPoint, NormalizedWeatherRun, ProviderId, SpatialCoverage,
    StringId, SystemId, TimeRange, UtcTimestamp, WattHours, Watts, WattsPerSquareMetre,
    WeatherDataKind, WeatherDataProvenance, WeatherDataRunId, YieldCalculationResult,
    YieldCalculationRunId, YieldResultId, YieldScope,
};
use pvlog_storage::{
    AccountConfigurationRepository, DatabaseTarget, ForecastRetentionClass, ForecastSettingsRecord,
    InverterRecord, OperationalRepository, PostgresAccountConfigurationRepository,
    PostgresOperationalRepository, PostgresYieldForecastInputRepository,
    PostgresYieldResultRepository, ProviderRecord, PvStringRecord,
    SqliteAccountConfigurationRepository, SqliteAccountPoolConfig, SqliteAccountPoolRouter,
    SqliteAccountProvisioner, SqliteOperationalRepository, SqliteYieldForecastInputRepository,
    SqliteYieldResultRepository, StoredYieldResult, SystemConfigurationRecord,
    WeatherRunInsertOutcome, WeatherRunRecord, YieldCalculationRunRecord, YieldCalculationState,
    YieldForecastInputRepository, YieldInvalidationReason, YieldInvalidationRecord,
    YieldInvalidationState, YieldResultRepository, apply_migrations,
};
use sqlx::{Connection as _, PgConnection, SqliteConnection, sqlite::SqliteConnectOptions};
use tempfile::TempDir;
use url::Url;
use uuid::Uuid;

#[tokio::test]
async fn sqlite_yield_forecast_repository_contract() -> Result<(), Box<dyn Error>> {
    let directory = TempDir::new()?;
    let management = directory.path().join("management.sqlite3");
    let accounts = directory.path().join("accounts");
    apply_migrations(&DatabaseTarget::Sqlite {
        management_path: management.clone(),
        accounts_dir: accounts.clone(),
    })
    .await?;
    let account_id = create_sqlite_account(&management, &accounts).await?;
    let router = SqliteAccountPoolRouter::new(
        management.clone(),
        accounts,
        SqliteAccountPoolConfig::default(),
    )?;
    let account_route = router.route(account_id).await?;
    verify_contract(
        &SqliteAccountConfigurationRepository::new(account_route.clone()),
        &SqliteOperationalRepository::new(management, account_route.clone()),
        &SqliteYieldForecastInputRepository::new(account_route.clone()),
        &SqliteYieldResultRepository::new(account_route),
    )
    .await
}

#[tokio::test]
async fn postgres_yield_forecast_repository_contract_when_configured() -> Result<(), Box<dyn Error>>
{
    let Ok(url) = std::env::var("TEST_POSTGRES_URL") else {
        return Ok(());
    };
    apply_migrations(&DatabaseTarget::Postgres { url: url.clone() }).await?;
    let account_id = create_postgres_account(&url).await?;
    verify_contract(
        &PostgresAccountConfigurationRepository::new(url.clone(), account_id),
        &PostgresOperationalRepository::new(url.clone(), account_id),
        &PostgresYieldForecastInputRepository::new(url.clone(), account_id),
        &PostgresYieldResultRepository::new(url, account_id),
    )
    .await
}

#[allow(clippy::too_many_lines)]
async fn verify_contract(
    configuration: &dyn AccountConfigurationRepository,
    operations: &dyn OperationalRepository,
    inputs: &dyn YieldForecastInputRepository,
    results: &dyn YieldResultRepository,
) -> Result<(), Box<dyn Error>> {
    let system_id = SystemId::new();
    configuration.save_system(&system(system_id)).await?;
    let inverter = inverter(system_id);
    let string_id = inverter.strings[0].id;
    configuration.save_inverter_aggregate(&inverter).await?;
    let provider_id = ProviderId::new();
    operations.save_provider(&provider(provider_id)).await?;

    let first_settings = settings(system_id, string_id, 0, Some(100));
    let second_settings = settings(system_id, string_id, 100, None);
    inputs.insert_forecast_settings(&first_settings).await?;
    inputs.insert_forecast_settings(&second_settings).await?;
    assert_eq!(
        inputs.effective_forecast_settings(string_id, 99).await?,
        Some(first_settings.clone())
    );
    assert_eq!(
        inputs.effective_forecast_settings(string_id, 100).await?,
        Some(second_settings.clone())
    );
    let mut overlapping = settings(system_id, string_id, 50, Some(150));
    overlapping.configuration_digest = [9; 32];
    assert!(inputs.insert_forecast_settings(&overlapping).await.is_err());

    let weather = weather_run(system_id, provider_id, "revision-1", 1_000, 3_000)?;
    assert_eq!(
        inputs.insert_weather_run(&weather).await?,
        WeatherRunInsertOutcome::Inserted
    );
    assert_eq!(
        inputs.insert_weather_run(&weather).await?,
        WeatherRunInsertOutcome::AlreadyPresent
    );
    assert_eq!(
        inputs.weather_run(weather.run.id).await?,
        Some(weather.clone())
    );
    let selected = inputs
        .select_weather_run(
            system_id,
            WeatherDataKind::Forecast,
            range(1_000, 3_000)?,
            Some(timestamp(900)?),
        )
        .await?;
    assert_eq!(selected, Some(weather.clone()));

    let revised = weather_run(system_id, provider_id, "revision-2", 1_000, 3_000)?;
    inputs.insert_weather_run(&revised).await?;
    assert_eq!(
        inputs
            .select_weather_run(
                system_id,
                WeatherDataKind::Forecast,
                range(1_000, 3_000)?,
                None,
            )
            .await?
            .map(|record| record.run.id),
        Some(revised.run.id)
    );

    let run = calculation_run(system_id, revised.run.id);
    assert_eq!(
        results.insert_run(&run).await?,
        WeatherRunInsertOutcome::Inserted
    );
    assert_eq!(
        results.insert_run(&run).await?,
        WeatherRunInsertOutcome::AlreadyPresent
    );
    let stored = yield_result(&run, 1_000, 2_000)?;
    results
        .insert_results_and_project(&run, std::slice::from_ref(&stored), 4_000)
        .await?;
    assert_eq!(
        results
            .active_results(
                system_id,
                CalculationBasis::Forecast,
                YieldScope::System(system_id),
                range(1_000, 3_000)?,
                100,
            )
            .await?,
        vec![stored.clone()]
    );
    assert_eq!(
        results
            .result_history(
                system_id,
                CalculationBasis::Forecast,
                YieldScope::System(system_id),
                range(1_000, 3_000)?,
                100,
            )
            .await?,
        vec![stored]
    );

    let invalidation = YieldInvalidationRecord {
        id: Uuid::now_v7(),
        system_id,
        range: range(1_500, 2_500)?,
        reason: YieldInvalidationReason::ProviderRevision,
        state: YieldInvalidationState::Pending,
        idempotency_key: "provider-revision-2".to_owned(),
        created_at: 5_000,
        completed_at: None,
    };
    assert_eq!(
        results.insert_invalidation(&invalidation).await?,
        WeatherRunInsertOutcome::Inserted
    );
    assert_eq!(
        results
            .pending_invalidations(system_id, range(1_000, 3_000)?, 100)
            .await?,
        vec![invalidation.clone()]
    );
    assert!(
        results
            .complete_invalidation(invalidation.id, 6_000)
            .await?
    );
    assert!(
        results
            .pending_invalidations(system_id, range(1_000, 3_000)?, 100)
            .await?
            .is_empty()
    );

    assert!(
        inputs
            .retain_weather_run(
                revised.run.id,
                ForecastRetentionClass::Referenced,
                None,
                Some(7_000),
            )
            .await?
    );
    assert!(
        results
            .retain_calculation_run(
                run.id,
                ForecastRetentionClass::Referenced,
                None,
                Some(7_000),
            )
            .await?
    );
    assert_eq!(inputs.purge_expired_weather_runs(10_000, 100).await?, 1);
    assert!(inputs.weather_run(revised.run.id).await?.is_some());
    assert_eq!(
        results.purge_expired_calculation_runs(10_000, 100).await?,
        0
    );
    Ok(())
}

fn system(id: SystemId) -> SystemConfigurationRecord {
    SystemConfigurationRecord {
        id,
        name: "Forecast contract".to_owned(),
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

fn inverter(system_id: SystemId) -> InverterRecord {
    let inverter_id = InverterId::new();
    InverterRecord {
        id: inverter_id,
        system_id,
        name: "Forecast inverter".to_owned(),
        manufacturer: None,
        model: None,
        serial_reference: None,
        rated_power_watts: Some(8_000),
        catalog_entry_id: None,
        catalog_revision: None,
        value_provenance: EquipmentValueProvenance::Manual,
        specification_snapshot: None,
        effective_from: 0,
        effective_to: None,
        created_at: 1,
        updated_at: 1,
        strings: vec![PvStringRecord {
            id: StringId::new(),
            inverter_id,
            name: "South".to_owned(),
            panel_count: 20,
            panel_manufacturer: Some("Example".to_owned()),
            panel_model: Some("P400".to_owned()),
            rated_power_watts: 8_000,
            module_catalog_entry_id: None,
            module_catalog_revision: None,
            value_provenance: EquipmentValueProvenance::Manual,
            module_specification_snapshot: None,
            module_peak_power_watts: Some(400),
            total_peak_power_watts: Some(8_000),
            orientation_degrees: Some(180),
            tilt_degrees: Some(35),
            effective_from: 0,
            effective_to: None,
            created_at: 1,
            updated_at: 1,
        }],
    }
}

fn provider(id: ProviderId) -> ProviderRecord {
    ProviderRecord {
        id,
        provider_kind: "weather".to_owned(),
        name: "Forecast fixture".to_owned(),
        enabled: true,
        endpoint_url: Some("https://weather.example.test".to_owned()),
        credential_secret_ref: None,
        configuration: serde_json::json!({}),
        license_metadata: serde_json::json!({"license": "fixture"}),
        circuit_state: "closed".to_owned(),
        created_at: 1,
        updated_at: 1,
    }
}

fn settings(
    system_id: SystemId,
    string_id: StringId,
    effective_from: i64,
    effective_to: Option<i64>,
) -> ForecastSettingsRecord {
    ForecastSettingsRecord {
        id: ForecastSettingsId::new(),
        system_id,
        string_id,
        effective_from,
        effective_to,
        model_identifier: "pv-yield-v1".to_owned(),
        model_revision: 1,
        soiling_loss_basis_points: 200,
        shading_loss_basis_points: 100,
        mismatch_loss_basis_points: 100,
        wiring_loss_basis_points: 100,
        unavailability_loss_basis_points: 50,
        calibration_basis_points: 0,
        configuration_digest: [u8::from(effective_from != 0); 32],
        created_at: effective_from + 1,
        created_by: None,
    }
}

fn weather_run(
    system_id: SystemId,
    provider_id: ProviderId,
    source_key: &str,
    start: i64,
    end: i64,
) -> Result<WeatherRunRecord, Box<dyn Error>> {
    let revision = if source_key.ends_with('2') { 800 } else { 700 };
    Ok(WeatherRunRecord {
        system_id,
        source_run_key: source_key.to_owned(),
        run: NormalizedWeatherRun {
            id: WeatherDataRunId::new(),
            kind: WeatherDataKind::Forecast,
            issued_at: Some(timestamp(revision)?),
            valid_range: range(start, end)?,
            resolution_seconds: u32::try_from((end - start) / 1_000)?,
            spatial_coverage: SpatialCoverage::Point(GeographicPoint {
                latitude_microdegrees: 52_520_000,
                longitude_microdegrees: 13_405_000,
            }),
            provenance: WeatherDataProvenance {
                provider_id,
                adapter: "fixture-v1".to_owned(),
                source_url: Url::parse("https://weather.example.test/run")?,
                license_identifier: "fixture".to_owned(),
                attribution: "Forecast fixture".to_owned(),
                fetched_at: timestamp(revision + 10)?,
            },
            points: vec![NormalizedWeatherPoint {
                interval: range(start, end)?,
                irradiance: IrradiancePoint {
                    global_horizontal: Some(EstimateRange::without_uncertainty(
                        WattsPerSquareMetre::new(500),
                    )),
                    direct_normal: None,
                    diffuse_horizontal: None,
                    plane_of_array: None,
                },
                ambient_temperature: None,
                wind_speed: None,
                cloud_cover: None,
            }],
        },
        retention_class: ForecastRetentionClass::Working,
        retain_until: Some(9_000),
        referenced_at: None,
        created_at: revision + 20,
    })
}

fn calculation_run(
    system_id: SystemId,
    weather_run_id: WeatherDataRunId,
) -> YieldCalculationRunRecord {
    YieldCalculationRunRecord {
        id: YieldCalculationRunId::new(),
        system_id,
        weather_run_id,
        basis: CalculationBasis::Forecast,
        model_version: ModelVersion {
            identifier: "pv-yield-v1".to_owned(),
            revision: 1,
        },
        configuration_digest: [4; 32],
        state: YieldCalculationState::Pending,
        requested_at: 3_500,
        completed_at: None,
        safe_error_code: None,
        retention_class: ForecastRetentionClass::Working,
        retain_until: Some(9_000),
        referenced_at: None,
        idempotency_key: "forecast-revision-2".to_owned(),
    }
}

fn yield_result(
    run: &YieldCalculationRunRecord,
    start: i64,
    end: i64,
) -> Result<StoredYieldResult, Box<dyn Error>> {
    Ok(StoredYieldResult {
        result: YieldCalculationResult {
            id: YieldResultId::new(),
            calculation_run_id: run.id,
            weather_run_id: run.weather_run_id,
            basis: run.basis,
            scope: YieldScope::System(run.system_id),
            interval: range(start, end)?,
            model_version: run.model_version.clone(),
            configuration_digest: run.configuration_digest,
            power: Some(EstimateRange {
                central: Watts::new(4_000),
                lower: Some(Watts::new(3_500)),
                upper: Some(Watts::new(4_500)),
            }),
            energy: Some(EstimateRange {
                central: WattHours::new(1_000),
                lower: Some(WattHours::new(900)),
                upper: Some(WattHours::new(1_100)),
            }),
            included_capacity: Watts::new(7_000),
            total_effective_capacity: Watts::new(8_000),
            completeness: ForecastCompleteness::Partial {
                reasons: vec![ForecastCompletenessReason::PartialEffectiveCapacity],
            },
        },
        created_at: 3_600,
    })
}

fn timestamp(value: i64) -> Result<UtcTimestamp, Box<dyn Error>> {
    Ok(UtcTimestamp::from_epoch_millis(value)?)
}

fn range(start: i64, end: i64) -> Result<TimeRange, Box<dyn Error>> {
    Ok(TimeRange::new(timestamp(start)?, timestamp(end)?)?)
}

async fn create_sqlite_account(
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
    sqlx::query(
        "INSERT INTO accounts (id,slug,display_name,status,created_at,updated_at) \
         VALUES (?,?,?,'provisioning',1,1)",
    )
    .bind(account_id.as_uuid().as_bytes().as_slice())
    .bind(format!("forecast-{account_id}"))
    .bind("Forecast contract")
    .execute(&mut management)
    .await?;
    management.close().await?;
    SqliteAccountProvisioner::new(management_path.to_owned(), accounts_dir.to_owned())
        .provision(account_id)
        .await?;
    Ok(account_id)
}

async fn create_postgres_account(url: &str) -> Result<AccountId, Box<dyn Error>> {
    let account_id = AccountId::new();
    let mut connection = PgConnection::connect(url).await?;
    sqlx::query(
        "INSERT INTO management.accounts \
         (id,slug,display_name,status,created_at,updated_at) \
         VALUES ($1,$2,$3,'active',1,1)",
    )
    .bind(account_id.as_uuid())
    .bind(format!("forecast-{account_id}"))
    .bind("Forecast contract")
    .execute(&mut connection)
    .await?;
    connection.close().await?;
    Ok(account_id)
}
