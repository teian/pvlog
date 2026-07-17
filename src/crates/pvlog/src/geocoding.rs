//! Rate-limited and cached Photon-compatible OpenStreetMap geocoding adapter.

use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use async_trait::async_trait;
use pvlog_api::{GeocodingApi, GeocodingError, GeocodingResult};
use serde::Deserialize;
use tokio::sync::Mutex;
use url::Url;

#[derive(Clone)]
pub struct PhotonGeocodingApi {
    endpoint: Url,
    client: reqwest::Client,
    cache: Arc<Mutex<HashMap<String, Vec<GeocodingResult>>>>,
    last_request: Arc<Mutex<Option<Instant>>>,
}

impl PhotonGeocodingApi {
    pub fn new(endpoint: Url) -> Result<Self, GeocodingError> {
        let client = reqwest::Client::builder()
            .user_agent(format!(
                "PVLog/{} (+https://github.com/pvlog/pvlog)",
                env!("CARGO_PKG_VERSION")
            ))
            .build()
            .map_err(|_| GeocodingError::Unavailable)?;
        Ok(Self {
            endpoint,
            client,
            cache: Arc::new(Mutex::new(HashMap::new())),
            last_request: Arc::new(Mutex::new(None)),
        })
    }
}

#[derive(Deserialize)]
struct PhotonResponse {
    features: Vec<PhotonFeature>,
}

#[derive(Deserialize)]
struct PhotonFeature {
    geometry: PhotonGeometry,
    properties: PhotonProperties,
}

#[derive(Deserialize)]
struct PhotonGeometry {
    coordinates: [f64; 2],
}

#[derive(Deserialize)]
struct PhotonProperties {
    name: Option<String>,
    street: Option<String>,
    housenumber: Option<String>,
    postcode: Option<String>,
    city: Option<String>,
    state: Option<String>,
    country: Option<String>,
}

impl PhotonProperties {
    fn display_name(self) -> String {
        let street = match (self.street, self.housenumber) {
            (Some(street), Some(number)) => Some(format!("{street} {number}")),
            (street, None) => street,
            (None, Some(number)) => Some(number),
        };
        [
            self.name,
            street,
            self.postcode,
            self.city,
            self.state,
            self.country,
        ]
        .into_iter()
        .flatten()
        .fold(Vec::<String>::new(), |mut parts, value| {
            if !parts.iter().any(|part| part == &value) {
                parts.push(value);
            }
            parts
        })
        .join(", ")
    }
}

#[async_trait]
impl GeocodingApi for PhotonGeocodingApi {
    async fn search(
        &self,
        query: &str,
        language: Option<&str>,
        limit: u8,
    ) -> Result<Vec<GeocodingResult>, GeocodingError> {
        let language = language
            .and_then(|value| value.split(['-', '_']).next())
            .unwrap_or("en");
        let key = format!("{}|{language}|{limit}", query.to_lowercase());
        if let Some(value) = self.cache.lock().await.get(&key).cloned() {
            return Ok(value);
        }
        let mut last_request = self.last_request.lock().await;
        if let Some(last) = *last_request {
            tokio::time::sleep(Duration::from_secs(1).saturating_sub(last.elapsed())).await;
        }
        *last_request = Some(Instant::now());
        drop(last_request);

        let mut url = self.endpoint.clone();
        url.query_pairs_mut()
            .append_pair("q", query)
            .append_pair("lang", language)
            .append_pair("limit", &limit.to_string());
        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|_| GeocodingError::Unavailable)?;
        if !response.status().is_success() {
            return Err(GeocodingError::Unavailable);
        }
        let result = response
            .json::<PhotonResponse>()
            .await
            .map_err(|_| GeocodingError::Unavailable)?
            .features
            .into_iter()
            .filter_map(|feature| {
                let [longitude, latitude] = feature.geometry.coordinates;
                let display_name = feature.properties.display_name();
                (!display_name.is_empty()
                    && (-90.0..=90.0).contains(&latitude)
                    && (-180.0..=180.0).contains(&longitude))
                .then_some(GeocodingResult {
                    display_name,
                    latitude,
                    longitude,
                    attribution: "© OpenStreetMap contributors".to_owned(),
                })
            })
            .collect::<Vec<_>>();
        self.cache.lock().await.insert(key, result.clone());
        Ok(result)
    }
}
