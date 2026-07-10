use std::collections::BTreeMap;

use serde::Serialize;

use crate::{
    BasisPoints, ChannelId, CorrectionId, MilliDegreesCelsius, MilliVolts, ObservationId,
    QualityFlags, SegmentId, SystemId, UtcTimestamp, ValidationError, WattHours, Watts,
};

/// Half-open UTC range used by storage and chart boundaries.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct TimeRange {
    pub start: UtcTimestamp,
    pub end: UtcTimestamp,
}

impl TimeRange {
    /// Creates a non-empty half-open UTC range.
    ///
    /// # Errors
    ///
    /// Returns an error when `end` is not later than `start`.
    pub fn new(start: UtcTimestamp, end: UtcTimestamp) -> Result<Self, ValidationError> {
        if end <= start {
            Err(ValidationError::new(
                "invalid_time_range",
                "end",
                "time range end must be later than its start",
            ))
        } else {
            Ok(Self { start, end })
        }
    }

    #[must_use]
    pub fn contains(self, timestamp: UtcTimestamp) -> bool {
        timestamp >= self.start && timestamp < self.end
    }
}

/// Canonical observation accepted from any ingestion path.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CanonicalObservation {
    pub id: ObservationId,
    pub system_id: SystemId,
    pub observed_at: UtcTimestamp,
    pub received_at: UtcTimestamp,
    pub values: MeasurementValues,
    pub source: ObservationSource,
    pub idempotency: IdempotencyIdentity,
    pub quality: QualityFlags,
}

/// Typed nullable measurement columns carried by one observation.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct MeasurementValues {
    pub generation_power: Option<Watts>,
    pub generation_energy: Option<EnergyReading>,
    pub consumption_power: Option<Watts>,
    pub consumption_energy: Option<EnergyReading>,
    pub grid: Option<GridFlow>,
    pub voltage: Option<MilliVolts>,
    pub temperature: Option<MilliDegreesCelsius>,
    pub battery: Option<BatteryReading>,
    pub extended: BTreeMap<ChannelId, ExtendedValue>,
}

/// Whether energy is an interval quantity or a monotonically interpreted counter.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EnergyReading {
    Interval(WattHours),
    Cumulative {
        total: WattHours,
        reset_sequence: u32,
    },
}

/// Grid flow represented either as net power or independent import/export meters.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GridFlow {
    Net {
        power: Watts,
        positive: NetPositiveDirection,
    },
    Split {
        import_power: Watts,
        export_power: Watts,
    },
}

/// Sign convention attached to a net grid value.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NetPositiveDirection {
    Import,
    Export,
}

/// Battery measurements with explicit directional and state semantics.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct BatteryReading {
    pub energy: Option<WattHours>,
    pub power: Option<Watts>,
    pub state_of_charge: Option<BasisPoints>,
    pub flow_state: BatteryFlowState,
}

/// Interpreted battery flow state.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BatteryFlowState {
    Charging,
    Discharging,
    Idle,
    Unknown,
}

/// Typed value for an administrator-registered extended channel.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtendedValue {
    Integer(i64),
    DecimalScaled(i64),
    Boolean(bool),
}

/// Provider-neutral provenance for an accepted observation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ObservationSource {
    pub kind: ObservationSourceKind,
    pub source_reference: Option<String>,
}

/// Ingestion boundary category, deliberately independent from product names.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservationSourceKind {
    ModernApi,
    CompatibilityApi,
    Import,
    PollingConnector,
    Manual,
    Derived,
}

/// Deterministic identity used to classify safe retries and conflicts.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct IdempotencyIdentity {
    pub namespace: String,
    pub key: String,
    pub payload_hash: [u8; 32],
}

/// Immutable overlay correcting or suppressing a previously accepted observation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Correction {
    pub id: CorrectionId,
    pub observation_id: ObservationId,
    pub corrected_at: UtcTimestamp,
    pub replacement: Option<MeasurementValues>,
    pub reason_code: String,
    pub generation: u32,
}

/// Metadata for one deterministic compressed telemetry segment.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ArchivedSegment {
    pub id: SegmentId,
    pub system_id: SystemId,
    pub range: TimeRange,
    pub encoding: SegmentEncoding,
    pub row_count: u32,
    pub uncompressed_bytes: u64,
    pub compressed_bytes: u64,
    pub content_hash: [u8; 32],
    pub correction_generation: u32,
}

/// Versioned archived payload format.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct SegmentEncoding {
    pub schema_version: u16,
    pub compression: SegmentCompression,
}

/// Compression algorithm attached to archived bytes.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SegmentCompression {
    Zstandard,
}

/// Precomputed aggregate bucket with an explicit generation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Rollup {
    pub system_id: SystemId,
    pub range: TimeRange,
    pub resolution: RollupResolution,
    pub values: RollupValues,
    pub source_rows: u32,
    pub generation: u32,
    pub quality: QualityFlags,
}

/// Supported fixed rollup windows.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RollupResolution {
    FiveMinutes,
    FifteenMinutes,
    Hour,
    Day,
    Month,
}

/// Common chart aggregates, retaining integer base units.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct RollupValues {
    pub generation_power: Option<AggregateValue>,
    pub generation_energy: Option<WattHours>,
    pub consumption_power: Option<AggregateValue>,
    pub consumption_energy: Option<WattHours>,
    pub grid_power: Option<AggregateValue>,
}

/// Minimum, maximum, mean, and count for an integer-valued series.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct AggregateValue {
    pub minimum: i64,
    pub maximum: i64,
    pub mean: i64,
    pub samples: u32,
}

/// Data availability returned alongside chart and export results.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Coverage {
    pub requested: TimeRange,
    pub available: Vec<TimeRange>,
    pub gaps: Vec<CoverageGap>,
    pub complete: bool,
}

/// One unavailable interval and its safe classification.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct CoverageGap {
    pub range: TimeRange,
    pub reason: CoverageGapReason,
}

/// Why requested data is unavailable.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CoverageGapReason {
    NotReported,
    ArchivedUnavailable,
    Processing,
    Redacted,
}
