//! Provider- and persistence-neutral `PVLog` domain types and policies.

#![forbid(unsafe_code)]

mod equipment_catalog;
mod forecasting;
mod identifiers;
mod identity;
mod measurements;
mod operations;
mod rbac;
mod shared;
mod systems;
mod telemetry;
mod time;
mod validation;
mod yield_model;

pub use equipment_catalog::{
    CatalogEntryId, CatalogProvenance, CatalogRevision, CoolingMethod, DimensionsMillimetres,
    EquipmentTemplateReference, EquipmentValueProvenance, InverterAcSpecification,
    InverterCatalogEntry, InverterDcSpecification, InverterOperationalSpecification,
    InverterSpecificationSnapshot, InverterTopology, MpptInputSpecification,
    PvStringModuleComposition, SolarCellTechnology, SolarModuleCatalogEntry,
    SolarModuleSpecification, SolarModuleSpecificationSnapshot, TemperatureRange,
};
pub use forecasting::{
    CalculationBasis, EffectiveInverterCapacity, EffectiveStringCapacity, EffectiveSystemCapacity,
    EstimateRange, ForecastCompleteness, ForecastCompletenessReason, ForecastConfigurationDigest,
    ForecastDigestError, ForecastInputSnapshot, ForecastLossFactors, ForecastSettings,
    ForecastSettingsId, GeographicPoint, IrradiancePoint, MetresPerSecondMilli, ModelVersion,
    NormalizedWeatherPoint, NormalizedWeatherRun, SpatialCoverage, UnsignedBasisPoints,
    WattsPerSquareMetre, WeatherDataKind, WeatherDataProvenance, WeatherDataRunId,
    WeatherRunValidationError, YieldCalculationResult, YieldCalculationRunId, YieldResultId,
    YieldScope,
};
pub use identifiers::{
    AccountId, AlertEventId, AlertRuleId, ApiCredentialId, AuditEventId, ChannelId, ConnectorId,
    CorrectionId, EquipmentId, ExportId, ExternalIdentityId, IdentifierError, ImportId, InverterId,
    JobId, MembershipId, ObservationId, PasswordRecoveryId, ProviderId, RequestId,
    RoleAssignmentId, RoleId, SegmentId, SessionId, StringId, SystemId, TariffId, UserId,
    UserInvitationId, WebhookDeliveryId, WebhookSubscriptionId,
};
pub use identity::{
    Account, AccountStatus, ApiCredential, ApiScope, AuditEvent, AuditOutcome, BuiltInRole,
    CredentialDigest, ExternalIdentity, ExternalProfile, LocalUser, Membership, MembershipStatus,
    PasswordHash, PasswordState, Permission, PrincipalId, QuotaPolicy, RecoveryState, Role,
    RoleAssignment, RoleKind, RoleScope, Session, SessionState, StorageRoutingState, UserStatus,
};
pub use measurements::{
    BasisPoints, MilliDegreesCelsius, MilliVolts, QualityFlags, WattHours, Watts,
};
pub use operations::{
    AlertEvent, AlertEventState, AlertKind, AlertRule, AlertSchedule, DeliveryAttempt,
    ExportFormat, ExportRequest, ExportState, ImportRequest, ImportState, Job, JobKind, JobState,
    Provider, ProviderCapability, ProviderState, WebhookDelivery, WebhookDeliveryState,
    WebhookEventType, WebhookSubscription, WebhookSubscriptionState,
};
pub use rbac::{AccessDecision, AccessDenial, AccessRequest, RbacEvaluator, built_in_permissions};
pub use shared::{CurrencyCode, Money, Visibility};
pub use systems::{
    CalculationSettings, CapacityPeriod, ChannelDataType, ChannelDefinition, ChannelDisplay,
    ChannelLifecycle, ChannelScale, EffectivePeriod, Equipment, EquipmentKind, GeographicPrecision,
    Inverter, NetCalculationMode, PowerCalculationMode, PvString, PvSystem, SystemLifecycle,
    SystemPrivacy, Tariff, TariffDirection,
};
pub use telemetry::{
    AggregateValue, ArchivedSegment, BatteryFlowState, BatteryReading, CanonicalObservation,
    Correction, Coverage, CoverageGap, CoverageGapReason, EnergyReading, ExtendedValue, GridFlow,
    IdempotencyIdentity, MeasurementValues, NetPositiveDirection, ObservationSource,
    ObservationSourceKind, Rollup, RollupResolution, RollupValues, SegmentCompression,
    SegmentEncoding, TimeRange,
};
pub use time::{IanaTimezone, UtcTimestamp};
pub use validation::ValidationError;
pub use yield_model::{
    SolarPosition, StringDcEstimate, StringDcInput, SurfaceOrientation, YIELD_MODEL_V1_IDENTIFIER,
    YIELD_MODEL_V1_REVISION, YieldModelError, calculate_string_dc, plane_of_array_irradiance,
    solar_position,
};
