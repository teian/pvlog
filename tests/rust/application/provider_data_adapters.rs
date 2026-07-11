use async_trait::async_trait;
use pvlog_application::{
    CircuitBreakerPolicy, Clock, ConfiguredExternalDataAdapter, ConfiguredExternalDataService,
    ExternalDataCacheEntry, ExternalDataCacheKey, ExternalDataCacheRepository,
    ExternalDataConfiguration, ExternalDataFreshness, ExternalDataKind, ExternalDataLicense,
    ExternalDataProvenance, ExternalDataRequest, InsolationPoint, PortError,
};
use pvlog_domain::{ProviderId, SystemId, TimeRange, UtcTimestamp};
use std::{
    error::Error,
    sync::{Arc, Mutex},
};
use url::Url;

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
