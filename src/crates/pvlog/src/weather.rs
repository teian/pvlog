//! Weather polling over the administrator-configured JSON adapter.
//!
//! `PVLog` bundles no weather service. An operator points `forecasting.adapter_endpoint`
//! at an endpoint that speaks the normalized JSON shape, and this module fetches
//! from it and stores the run and its intervals. Which provider sits behind that
//! endpoint, and under which licence, stays the operator's choice — the run
//! records the attribution it was given.

use std::sync::Arc;

use async_trait::async_trait;
use pvlog_application::{PortError, WeatherJsonTransport};
use pvlog_domain::{NormalizedWeatherRun, SystemId};
use pvlog_storage::DatabaseTarget;
use secrecy::ExposeSecret as _;
use url::Url;

/// HTTP transport for the weather adapter.
pub struct HttpWeatherTransport {
    client: reqwest::Client,
    /// Resolves the administrator's secret reference to a bearer credential.
    secrets: Arc<dyn pvlog_application::SecretResolver>,
}

impl HttpWeatherTransport {
    #[must_use]
    pub fn new(secrets: Arc<dyn pvlog_application::SecretResolver>) -> Self {
        Self {
            client: reqwest::Client::new(),
            secrets,
        }
    }
}

#[async_trait]
impl WeatherJsonTransport for HttpWeatherTransport {
    async fn get(
        &self,
        url: Url,
        timeout_milliseconds: u32,
        credential_secret_reference: Option<&str>,
    ) -> Result<Vec<u8>, PortError> {
        let mut request = self
            .client
            .get(url)
            .timeout(std::time::Duration::from_millis(u64::from(
                timeout_milliseconds,
            )));
        if let Some(reference) = credential_secret_reference {
            let secret = self.secrets.resolve(reference).await?;
            request = request.bearer_auth(secret.expose_secret());
        }
        let response = request.send().await.map_err(|_| PortError::Unavailable)?;
        if !response.status().is_success() {
            return Err(PortError::Unavailable);
        }
        response
            .bytes()
            .await
            .map(|bytes| bytes.to_vec())
            .map_err(|_| PortError::Unavailable)
    }
}

/// Persists one normalized weather run and its intervals.
///
/// # Errors
///
/// Returns an error when the account database rejects the write.
#[cfg(feature = "sqlite")]
pub async fn store_run(
    connection: &mut sqlx::SqliteConnection,
    system_id: SystemId,
    provider_configuration_id: &[u8],
    run: &NormalizedWeatherRun,
) -> Result<(), PortError> {
    let run_id = run.id.as_uuid().as_bytes().to_vec();
    sqlx::query(
        "INSERT INTO weather_data_runs (id,system_id,provider_configuration_id,source_run_key,\
         data_kind,issued_at,fetched_at,valid_from,valid_to,resolution_seconds,spatial_kind,\
         adapter,source_url,license_identifier,attribution) \
         VALUES (?,?,?,?,?,?,?,?,?,?,'point',?,?,?,?) ON CONFLICT(id) DO NOTHING",
    )
    .bind(run_id.clone())
    .bind(system_id.as_uuid().as_bytes().to_vec())
    .bind(provider_configuration_id.to_vec())
    .bind(run.id.as_uuid().to_string())
    .bind(kind_name(run))
    .bind(run.issued_at.map(epoch))
    .bind(epoch(run.provenance.fetched_at))
    .bind(epoch(run.valid_range.start))
    .bind(epoch(run.valid_range.end))
    .bind(i64::from(run.resolution_seconds))
    .bind(&run.provenance.adapter)
    .bind(run.provenance.source_url.to_string())
    .bind(&run.provenance.license_identifier)
    .bind(&run.provenance.attribution)
    .execute(&mut *connection)
    .await
    .map_err(|_| PortError::Unavailable)?;

    for point in &run.points {
        sqlx::query(
            "INSERT INTO weather_data_points (run_id,interval_start,interval_end,\
             global_horizontal_wm2,plane_of_array_wm2,ambient_temperature_millicelsius,\
             cloud_cover_basis_points) VALUES (?,?,?,?,?,?,?) \
             ON CONFLICT(run_id,interval_start) DO NOTHING",
        )
        .bind(run_id.clone())
        .bind(epoch(point.interval.start))
        .bind(epoch(point.interval.end))
        .bind(
            point
                .irradiance
                .global_horizontal
                .map(|value| i64::from(value.central.value())),
        )
        .bind(
            point
                .irradiance
                .plane_of_array
                .map(|value| i64::from(value.central.value())),
        )
        .bind(
            point
                .ambient_temperature
                .map(|value| i64::from(value.value())),
        )
        .bind(point.cloud_cover.map(|value| i64::from(value.value())))
        .execute(&mut *connection)
        .await
        .map_err(|_| PortError::Unavailable)?;
    }
    Ok(())
}

#[cfg(feature = "sqlite")]
fn epoch(value: pvlog_domain::UtcTimestamp) -> i64 {
    i64::try_from(value.epoch_millis()).unwrap_or_default()
}

#[cfg(feature = "sqlite")]
const fn kind_name(run: &NormalizedWeatherRun) -> &'static str {
    match run.kind {
        pvlog_domain::WeatherDataKind::Forecast => "forecast",
        pvlog_domain::WeatherDataKind::Observed => "observed",
        pvlog_domain::WeatherDataKind::Reanalysis => "reanalysis",
    }
}

/// Reports whether weather polling is configured for this deployment.
#[must_use]
pub const fn is_configured(target: &DatabaseTarget) -> bool {
    matches!(target, DatabaseTarget::Sqlite { .. })
}
