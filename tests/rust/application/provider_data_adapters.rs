use async_trait::async_trait;
use pvlog_application::{
    AdministratorWeatherJsonAdapter, CircuitBreakerPolicy, Clock, ConfiguredExternalDataAdapter,
    ConfiguredExternalDataService, ExternalDataCacheEntry, ExternalDataCacheKey,
    ExternalDataCacheRepository, ExternalDataConfiguration, ExternalDataFreshness,
    ExternalDataKind, ExternalDataLicense, ExternalDataProvenance, ExternalDataRequest,
    InsolationPoint, PortError, WeatherJsonTransport,
};
use pvlog_domain::{
    GeographicPoint, ProviderId, SpatialCoverage, SystemId, TimeRange, UtcTimestamp,
    WeatherDataKind,
};
use std::{
    error::Error,
    sync::{Arc, Mutex},
};
use url::Url;
use uuid::Uuid;

#[tokio::test]
async fn administrator_weather_json_adapter_normalizes_deterministic_fixture()
-> Result<(), Box<dyn Error>> {
    let run_id = Uuid::now_v7();
    let body = serde_json::to_vec(&serde_json::json!({
        "id": run_id,
        "kind": "forecast",
        "issuedAtMs": 800,
        "validFromMs": 1_000,
        "validToMs": 3_000,
        "resolutionSeconds": 2,
        "coverage": {"kind": "point", "latitude_e6": 52_520_000, "longitude_e6": 13_405_000},
        "units": {
            "irradiance": "W/m2",
            "temperature": "mC",
            "windSpeed": "mm/s",
            "cloudCover": "basis_points"
        },
        "sourceUrl": "https://weather.example.test/runs/revision-1",
        "points": [{
            "intervalStartMs": 1_000,
            "intervalEndMs": 3_000,
            "globalHorizontal": {"central": 500, "lower": 450, "upper": 550},
            "directNormal": null,
            "diffuseHorizontal": null,
            "planeOfArray": null,
            "ambientTemperature": 20_000,
            "windSpeed": 3_000,
            "cloudCover": 2_500
        }]
    }))?;
    let adapter = AdministratorWeatherJsonAdapter::new(FixtureTransport(body));
    let configuration = weather_configuration()?;
    let request = ExternalDataRequest::Weather {
        system_id: SystemId::new(),
        kind: WeatherDataKind::Forecast,
        range: TimeRange::new(
            UtcTimestamp::from_epoch_millis(1_000)?,
            UtcTimestamp::from_epoch_millis(3_000)?,
        )?,
        spatial_coverage: SpatialCoverage::Point(GeographicPoint {
            latitude_microdegrees: 52_520_000,
            longitude_microdegrees: 13_405_000,
        }),
        issued_before: Some(UtcTimestamp::from_epoch_millis(900)?),
    };
    let entry = adapter
        .fetch(
            &configuration,
            &request,
            UtcTimestamp::from_epoch_millis(900)?,
        )
        .await?;
    let ExternalDataCacheEntry::Weather { run, provenance } = entry else {
        return Err("weather adapter returned a different data class".into());
    };
    assert_eq!(run.id.as_uuid(), run_id);
    assert_eq!(run.kind, WeatherDataKind::Forecast);
    assert_eq!(
        run.points[0]
            .irradiance
            .global_horizontal
            .map(|value| value.central.value()),
        Some(500)
    );
    assert_eq!(provenance.license_identifier, "operator-supplied");
    Ok(())
}

struct FixtureTransport(Vec<u8>);

#[async_trait]
impl WeatherJsonTransport for FixtureTransport {
    async fn get(
        &self,
        url: Url,
        timeout_milliseconds: u32,
        credential_secret_reference: Option<&str>,
    ) -> Result<Vec<u8>, PortError> {
        assert!(
            url.query()
                .is_some_and(|query| query.contains("kind=forecast"))
        );
        assert_eq!(timeout_milliseconds, 500);
        assert_eq!(credential_secret_reference, Some("secret:weather/test"));
        Ok(self.0.clone())
    }
}

fn weather_configuration() -> Result<ExternalDataConfiguration, url::ParseError> {
    Ok(ExternalDataConfiguration {
        provider_id: ProviderId::new(),
        kind: ExternalDataKind::WeatherForecast,
        adapter: "administrator_weather_json_v1".to_owned(),
        endpoint: Url::parse("https://weather.example.test/v1/run")?,
        credential_secret_reference: Some("secret:weather/test".to_owned()),
        request_timeout_milliseconds: 500,
        cache_ttl_seconds: 300,
        maximum_stale_seconds: 900,
        license: ExternalDataLicense {
            identifier: "operator-supplied".to_owned(),
            attribution: "Fixture weather".to_owned(),
            source_url: Url::parse("https://weather.example.test/license")?,
            redistribution_permitted: false,
        },
        enabled: true,
    })
}

#[tokio::test]
async fn configured_adapter_caches_provenance_and_serves_stale_data_when_degraded()
-> Result<(), Box<dyn Error>> {
    let now = UtcTimestamp::from_epoch_millis(10_000)?;
    let adapter = Arc::new(FakeAdapter {
        fail: Mutex::new(false),
    });
    let cache = Arc::new(MemoryCache::default());
    let service = build_service(adapter.clone(), cache, now)?;
    let (key, request) = request(now)?;

    let fresh = service.query(&key, &request).await?;
    assert_eq!(fresh.freshness, ExternalDataFreshness::Fresh);
    assert_eq!(
        fresh.entry.provenance().license_identifier,
        "operator-supplied"
    );

    *adapter.fail.lock().map_err(|_| "adapter lock")? = true;
    let degraded = service.query(&key, &request).await?;
    assert_eq!(degraded.freshness, ExternalDataFreshness::StaleDegraded);

    let empty_service = build_service(adapter, Arc::new(MemoryCache::default()), now)?;
    assert!(matches!(
        empty_service.query(&key, &request).await,
        Err(PortError::Unavailable)
    ));
    Ok(())
}

fn build_service(
    adapter: Arc<FakeAdapter>,
    cache: Arc<MemoryCache>,
    now: UtcTimestamp,
) -> Result<ConfiguredExternalDataService<FakeAdapter, MemoryCache, FixedClock>, url::ParseError> {
    Ok(ConfiguredExternalDataService::new(
        configuration()?,
        adapter,
        cache,
        Arc::new(FixedClock(now)),
        CircuitBreakerPolicy {
            failure_threshold: 1,
            recovery_timeout_milliseconds: 60_000,
        },
    ))
}

struct FixedClock(UtcTimestamp);
impl Clock for FixedClock {
    fn now(&self) -> UtcTimestamp {
        self.0
    }
}

struct FakeAdapter {
    fail: Mutex<bool>,
}

#[async_trait]
impl ConfiguredExternalDataAdapter for FakeAdapter {
    async fn fetch(
        &self,
        configuration: &ExternalDataConfiguration,
        request: &ExternalDataRequest,
        fetched_at: UtcTimestamp,
    ) -> Result<ExternalDataCacheEntry, PortError> {
        if *self.fail.lock().map_err(|_| PortError::Unavailable)? {
            return Err(PortError::Unavailable);
        }
        assert!(matches!(request, ExternalDataRequest::Insolation { .. }));
        let valid_until =
            UtcTimestamp::from_epoch_millis(9_999).map_err(|_| PortError::Unavailable)?;
        Ok(entry(configuration, fetched_at, valid_until))
    }
}

#[derive(Default)]
struct MemoryCache {
    entry: Mutex<Option<ExternalDataCacheEntry>>,
}

#[async_trait]
impl ExternalDataCacheRepository for MemoryCache {
    async fn get(
        &self,
        _key: &ExternalDataCacheKey,
    ) -> Result<Option<ExternalDataCacheEntry>, PortError> {
        Ok(self
            .entry
            .lock()
            .map_err(|_| PortError::Unavailable)?
            .clone())
    }
    async fn put(
        &self,
        _key: &ExternalDataCacheKey,
        entry: &ExternalDataCacheEntry,
    ) -> Result<(), PortError> {
        *self.entry.lock().map_err(|_| PortError::Unavailable)? = Some(entry.clone());
        Ok(())
    }
}

fn configuration() -> Result<ExternalDataConfiguration, url::ParseError> {
    Ok(ExternalDataConfiguration {
        provider_id: ProviderId::new(),
        kind: ExternalDataKind::Insolation,
        adapter: "fake".to_owned(),
        endpoint: Url::parse("https://provider.example/data")?,
        credential_secret_reference: None,
        request_timeout_milliseconds: 500,
        cache_ttl_seconds: 1,
        maximum_stale_seconds: 300,
        license: ExternalDataLicense {
            identifier: "operator-supplied".to_owned(),
            attribution: "Example".to_owned(),
            source_url: Url::parse("https://provider.example/license")?,
            redistribution_permitted: false,
        },
        enabled: true,
    })
}

fn request(
    now: UtcTimestamp,
) -> Result<(ExternalDataCacheKey, ExternalDataRequest), Box<dyn Error>> {
    let end = UtcTimestamp::from_epoch_millis(20_000)?;
    let key = ExternalDataCacheKey {
        provider_id: ProviderId::new(),
        resource_key: "system-1".to_owned(),
        range_start: now,
        range_end: end,
    };
    let request = ExternalDataRequest::Insolation {
        system_id: SystemId::new(),
        range: TimeRange::new(now, end)?,
    };
    Ok((key, request))
}

fn entry(
    configuration: &ExternalDataConfiguration,
    fetched_at: UtcTimestamp,
    valid_until: UtcTimestamp,
) -> ExternalDataCacheEntry {
    ExternalDataCacheEntry::Insolation {
        points: vec![InsolationPoint {
            timestamp: fetched_at,
            watts_per_square_metre: 850,
        }],
        provenance: ExternalDataProvenance {
            provider_id: configuration.provider_id,
            adapter: configuration.adapter.clone(),
            source_url: configuration.endpoint.clone(),
            license_identifier: configuration.license.identifier.clone(),
            attribution: configuration.license.attribution.clone(),
            fetched_at,
            valid_until,
        },
    }
}
