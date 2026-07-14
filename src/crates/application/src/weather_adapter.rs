//! Administrator-configured JSON weather adapter with an injectable HTTP transport.

use async_trait::async_trait;
use pvlog_domain::{
    EstimateRange, GeographicPoint, IrradiancePoint, MetresPerSecondMilli, MilliDegreesCelsius,
    NormalizedWeatherPoint, NormalizedWeatherRun, SpatialCoverage, TimeRange, UnsignedBasisPoints,
    UtcTimestamp, WattsPerSquareMetre, WeatherDataKind, WeatherDataProvenance, WeatherDataRunId,
};
use serde::Deserialize;
use url::Url;
use uuid::Uuid;

use crate::{
    ConfiguredExternalDataAdapter, ExternalDataCacheEntry, ExternalDataConfiguration,
    ExternalDataProvenance, ExternalDataRequest, PortError,
};

#[async_trait]
pub trait WeatherJsonTransport: Send + Sync {
    async fn get(
        &self,
        url: Url,
        timeout_milliseconds: u32,
        credential_secret_reference: Option<&str>,
    ) -> Result<Vec<u8>, PortError>;
}

#[derive(Clone, Debug)]
pub struct AdministratorWeatherJsonAdapter<T> {
    transport: T,
}

impl<T> AdministratorWeatherJsonAdapter<T> {
    #[must_use]
    pub const fn new(transport: T) -> Self {
        Self { transport }
    }
}

#[async_trait]
impl<T: WeatherJsonTransport> ConfiguredExternalDataAdapter for AdministratorWeatherJsonAdapter<T> {
    async fn fetch(
        &self,
        configuration: &ExternalDataConfiguration,
        request: &ExternalDataRequest,
        fetched_at: UtcTimestamp,
    ) -> Result<ExternalDataCacheEntry, PortError> {
        if configuration.adapter != "administrator_weather_json_v1" {
            return Err(rejected("unsupported weather adapter"));
        }
        let ExternalDataRequest::Weather {
            system_id,
            kind,
            range,
            spatial_coverage,
            issued_before,
        } = request
        else {
            return Err(rejected("weather adapter requires a weather request"));
        };
        let mut url = configuration.endpoint.clone();
        {
            let mut query = url.query_pairs_mut();
            query
                .append_pair("system_id", &system_id.to_string())
                .append_pair("kind", weather_kind(*kind))
                .append_pair("start_ms", &range.start.epoch_millis().to_string())
                .append_pair("end_ms", &range.end.epoch_millis().to_string());
            if let Some(issued_before) = issued_before {
                query.append_pair(
                    "issued_before_ms",
                    &issued_before.epoch_millis().to_string(),
                );
            }
            match spatial_coverage {
                SpatialCoverage::Point(point) => {
                    query
                        .append_pair("latitude_e6", &point.latitude_microdegrees.to_string())
                        .append_pair("longitude_e6", &point.longitude_microdegrees.to_string());
                }
                SpatialCoverage::ProviderRegion(region) => {
                    query.append_pair("region", region);
                }
            }
        }
        let body = self
            .transport
            .get(
                url,
                configuration.request_timeout_milliseconds,
                configuration.credential_secret_reference.as_deref(),
            )
            .await?;
        let payload: WeatherJsonRun =
            serde_json::from_slice(&body).map_err(|_| rejected("malformed weather JSON"))?;
        let run = payload.normalize(configuration, fetched_at)?;
        if run.kind != *kind || run.spatial_coverage != *spatial_coverage {
            return Err(rejected("weather response does not match request"));
        }
        run.validate()
            .map_err(|_| rejected("weather response failed normalization"))?;
        let valid_until_millis =
            fetched_at.epoch_millis() + i128::from(configuration.cache_ttl_seconds) * 1_000;
        let valid_until = UtcTimestamp::from_epoch_millis(
            i64::try_from(valid_until_millis)
                .map_err(|_| rejected("weather cache expiry is out of range"))?,
        )
        .map_err(|_| rejected("weather cache expiry is out of range"))?;
        Ok(ExternalDataCacheEntry::Weather {
            provenance: ExternalDataProvenance {
                provider_id: configuration.provider_id,
                adapter: configuration.adapter.clone(),
                source_url: configuration.endpoint.clone(),
                license_identifier: configuration.license.identifier.clone(),
                attribution: configuration.license.attribution.clone(),
                fetched_at,
                valid_until,
            },
            run: Box::new(run),
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WeatherJsonRun {
    id: Uuid,
    kind: WeatherDataKind,
    issued_at_ms: Option<i64>,
    valid_from_ms: i64,
    valid_to_ms: i64,
    resolution_seconds: u32,
    coverage: WeatherJsonCoverage,
    units: WeatherJsonUnits,
    source_url: Url,
    points: Vec<WeatherJsonPoint>,
}

impl WeatherJsonRun {
    fn normalize(
        self,
        configuration: &ExternalDataConfiguration,
        fetched_at: UtcTimestamp,
    ) -> Result<NormalizedWeatherRun, PortError> {
        if self.units.irradiance != "W/m2"
            || self.units.temperature != "mC"
            || self.units.wind_speed != "mm/s"
            || self.units.cloud_cover != "basis_points"
        {
            return Err(rejected("unsupported weather units"));
        }
        Ok(NormalizedWeatherRun {
            id: WeatherDataRunId::from_uuid(self.id)
                .map_err(|_| rejected("invalid weather run identifier"))?,
            kind: self.kind,
            issued_at: self.issued_at_ms.map(timestamp).transpose()?,
            valid_range: TimeRange::new(
                timestamp(self.valid_from_ms)?,
                timestamp(self.valid_to_ms)?,
            )
            .map_err(|_| rejected("invalid weather run range"))?,
            resolution_seconds: self.resolution_seconds,
            spatial_coverage: self.coverage.normalize(),
            provenance: WeatherDataProvenance {
                provider_id: configuration.provider_id,
                adapter: configuration.adapter.clone(),
                source_url: self.source_url,
                license_identifier: configuration.license.identifier.clone(),
                attribution: configuration.license.attribution.clone(),
                fetched_at,
            },
            points: self
                .points
                .into_iter()
                .map(WeatherJsonPoint::normalize)
                .collect::<Result<Vec<_>, _>>()?,
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", deny_unknown_fields)]
enum WeatherJsonCoverage {
    Point { latitude_e6: i32, longitude_e6: i32 },
    ProviderRegion { region: String },
}

impl WeatherJsonCoverage {
    fn normalize(self) -> SpatialCoverage {
        match self {
            Self::Point {
                latitude_e6,
                longitude_e6,
            } => SpatialCoverage::Point(GeographicPoint {
                latitude_microdegrees: latitude_e6,
                longitude_microdegrees: longitude_e6,
            }),
            Self::ProviderRegion { region } => SpatialCoverage::ProviderRegion(region),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WeatherJsonUnits {
    irradiance: String,
    temperature: String,
    wind_speed: String,
    cloud_cover: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WeatherJsonPoint {
    interval_start_ms: i64,
    interval_end_ms: i64,
    global_horizontal: Option<WeatherJsonEstimate>,
    direct_normal: Option<WeatherJsonEstimate>,
    diffuse_horizontal: Option<WeatherJsonEstimate>,
    plane_of_array: Option<WeatherJsonEstimate>,
    ambient_temperature: Option<i32>,
    wind_speed: Option<u32>,
    cloud_cover: Option<u16>,
}

impl WeatherJsonPoint {
    fn normalize(self) -> Result<NormalizedWeatherPoint, PortError> {
        Ok(NormalizedWeatherPoint {
            interval: TimeRange::new(
                timestamp(self.interval_start_ms)?,
                timestamp(self.interval_end_ms)?,
            )
            .map_err(|_| rejected("invalid weather point interval"))?,
            irradiance: IrradiancePoint {
                global_horizontal: self.global_horizontal.map(WeatherJsonEstimate::normalize),
                direct_normal: self.direct_normal.map(WeatherJsonEstimate::normalize),
                diffuse_horizontal: self.diffuse_horizontal.map(WeatherJsonEstimate::normalize),
                plane_of_array: self.plane_of_array.map(WeatherJsonEstimate::normalize),
            },
            ambient_temperature: self.ambient_temperature.map(MilliDegreesCelsius::new),
            wind_speed: self.wind_speed.map(MetresPerSecondMilli::new),
            cloud_cover: self
                .cloud_cover
                .map(UnsignedBasisPoints::new)
                .transpose()
                .map_err(|_| rejected("cloud cover is out of range"))?,
        })
    }
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WeatherJsonEstimate {
    central: u32,
    lower: Option<u32>,
    upper: Option<u32>,
}

impl WeatherJsonEstimate {
    fn normalize(self) -> EstimateRange<WattsPerSquareMetre> {
        EstimateRange {
            central: WattsPerSquareMetre::new(self.central),
            lower: self.lower.map(WattsPerSquareMetre::new),
            upper: self.upper.map(WattsPerSquareMetre::new),
        }
    }
}

fn timestamp(value: i64) -> Result<UtcTimestamp, PortError> {
    UtcTimestamp::from_epoch_millis(value)
        .map_err(|_| rejected("weather timestamp is out of range"))
}

const fn weather_kind(kind: WeatherDataKind) -> &'static str {
    match kind {
        WeatherDataKind::Forecast => "forecast",
        WeatherDataKind::Observed => "observed",
        WeatherDataKind::Reanalysis => "reanalysis",
    }
}

fn rejected(message: &'static str) -> PortError {
    PortError::Rejected(message.to_owned())
}
