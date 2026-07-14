use std::{fmt, str::FromStr};

use serde::{Deserialize, Deserializer, Serialize, de};
use url::Url;
use uuid::Uuid;

use crate::{
    IdentifierError, InverterId, MilliDegreesCelsius, ProviderId, StringId, SystemId, TimeRange,
    UtcTimestamp, WattHours, Watts,
};

macro_rules! forecast_identifier {
    ($name:ident, $description:literal) => {
        #[doc = $description]
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
        #[serde(transparent)]
        pub struct $name(Uuid);

        impl $name {
            #[must_use]
            pub fn new() -> Self {
                Self(Uuid::now_v7())
            }

            pub fn from_uuid(value: Uuid) -> Result<Self, IdentifierError> {
                if value.get_version_num() == 7 {
                    Ok(Self(value))
                } else {
                    Err(IdentifierError::NotVersion7)
                }
            }

            #[must_use]
            pub const fn as_uuid(self) -> Uuid {
                self.0
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(formatter)
            }
        }

        impl FromStr for $name {
            type Err = IdentifierError;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                let uuid = Uuid::parse_str(value).map_err(|_| IdentifierError::InvalidUuid)?;
                Self::from_uuid(uuid)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                let uuid = Uuid::deserialize(deserializer)?;
                Self::from_uuid(uuid).map_err(de::Error::custom)
            }
        }
    };
}

forecast_identifier!(
    ForecastSettingsId,
    "Stable effective-dated PV yield forecast settings identifier."
);
forecast_identifier!(
    WeatherDataRunId,
    "Stable immutable normalized weather input run identifier."
);
forecast_identifier!(
    YieldCalculationRunId,
    "Stable versioned PV yield calculation run identifier."
);
forecast_identifier!(YieldResultId, "Stable modeled PV yield result identifier.");

/// Irradiance in watts per square metre.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct WattsPerSquareMetre(pub u32);

impl WattsPerSquareMetre {
    #[must_use]
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn value(self) -> u32 {
        self.0
    }
}

/// Wind speed in thousandths of a metre per second.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct MetresPerSecondMilli(pub u32);

impl MetresPerSecondMilli {
    #[must_use]
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn value(self) -> u32 {
        self.0
    }
}

/// Unsigned ratio in basis points, used for fractions such as cloud cover and coverage.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct UnsignedBasisPoints(pub u16);

impl UnsignedBasisPoints {
    pub const MAX: u16 = 10_000;

    #[must_use]
    pub const fn new_unchecked(value: u16) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn value(self) -> u16 {
        self.0
    }
}

/// Central modeled value with an optional uncertainty interval in the same explicit unit.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct EstimateRange<T> {
    pub central: T,
    pub lower: Option<T>,
    pub upper: Option<T>,
}

impl<T> EstimateRange<T> {
    #[must_use]
    pub const fn without_uncertainty(central: T) -> Self {
        Self {
            central,
            lower: None,
            upper: None,
        }
    }
}

/// Semantic classification that prevents forecasts from becoming historical observations.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WeatherDataKind {
    Forecast,
    Observed,
    Reanalysis,
}

/// Exact point used for provider lookup and solar-position calculations.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct GeographicPoint {
    pub latitude_microdegrees: i32,
    pub longitude_microdegrees: i32,
}

/// Spatial applicability retained with a normalized weather run.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SpatialCoverage {
    Point(GeographicPoint),
    ProviderRegion(String),
}

/// Normalized irradiance components for one weather interval.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct IrradiancePoint {
    pub global_horizontal: Option<EstimateRange<WattsPerSquareMetre>>,
    pub direct_normal: Option<EstimateRange<WattsPerSquareMetre>>,
    pub diffuse_horizontal: Option<EstimateRange<WattsPerSquareMetre>>,
    pub plane_of_array: Option<EstimateRange<WattsPerSquareMetre>>,
}

/// Provider-neutral weather values covering one half-open interval.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct NormalizedWeatherPoint {
    pub interval: TimeRange,
    pub irradiance: IrradiancePoint,
    pub ambient_temperature: Option<MilliDegreesCelsius>,
    pub wind_speed: Option<MetresPerSecondMilli>,
    pub cloud_cover: Option<UnsignedBasisPoints>,
}

/// Audit and licensing information retained independently from provider-specific payloads.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct WeatherDataProvenance {
    pub provider_id: ProviderId,
    pub adapter: String,
    pub source_url: Url,
    pub license_identifier: String,
    pub attribution: String,
    pub fetched_at: UtcTimestamp,
}

/// One immutable, normalized provider run and its ordered interval points.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct NormalizedWeatherRun {
    pub id: WeatherDataRunId,
    pub kind: WeatherDataKind,
    pub issued_at: Option<UtcTimestamp>,
    pub valid_range: TimeRange,
    pub resolution_seconds: u32,
    pub spatial_coverage: SpatialCoverage,
    pub provenance: WeatherDataProvenance,
    pub points: Vec<NormalizedWeatherPoint>,
}

/// Stable calculation algorithm identifier and revision.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct ModelVersion {
    pub identifier: String,
    pub revision: u16,
}

/// Whether modeled yield represents a future forecast or a historical expectation.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CalculationBasis {
    Forecast,
    Expected,
}

/// Hierarchical equipment scope of one modeled yield result.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum YieldScope {
    String(StringId),
    Inverter(InverterId),
    System(SystemId),
}

/// Stable explanation for a missing or partial modeled result.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ForecastCompletenessReason {
    MissingSystemLocation,
    MissingOrientation,
    MissingTilt,
    MissingModuleCapacity,
    MissingWeatherInput,
    UnsupportedWeatherInput,
    IncompatibleInputRun,
    PartialEffectiveCapacity,
    InsufficientWeatherCoverage,
    InsufficientActualCoverage,
    MissingActualTelemetry,
    NonPositiveExpectedEnergy,
}

/// Completeness and effective capacity included in a modeled result.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ForecastCompleteness {
    Complete,
    Partial {
        reasons: Vec<ForecastCompletenessReason>,
    },
    Unavailable {
        reasons: Vec<ForecastCompletenessReason>,
    },
}

/// Versioned interval output that never aliases modeled values to measured telemetry.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct YieldCalculationResult {
    pub id: YieldResultId,
    pub calculation_run_id: YieldCalculationRunId,
    pub weather_run_id: WeatherDataRunId,
    pub basis: CalculationBasis,
    pub scope: YieldScope,
    pub interval: TimeRange,
    pub model_version: ModelVersion,
    pub configuration_digest: [u8; 32],
    pub power: Option<EstimateRange<Watts>>,
    pub energy: Option<EstimateRange<WattHours>>,
    pub included_capacity: Watts,
    pub total_effective_capacity: Watts,
    pub completeness: ForecastCompleteness,
}
