//! Provider- and persistence-neutral `PVLog` domain types and policies.

#![forbid(unsafe_code)]

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

pub use identifiers::{
    AccountId, AlertEventId, AlertRuleId, ApiCredentialId, AuditEventId, ChannelId, ConnectorId,
    CorrectionId, EquipmentId, ExportId, ExternalIdentityId, FavouriteId, IdentifierError,
    ImportId, JobId, MembershipId, ObservationId, ProviderId, RequestId, RoleAssignmentId, RoleId,
    SegmentId, SessionId, SystemId, TariffId, TeamId, TeamMembershipId, UserId, WebhookDeliveryId,
    WebhookSubscriptionId,
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
    ExportFormat, ExportRequest, ExportState, Favourite, ImportRequest, ImportState, Job, JobKind,
    JobState, Provider, ProviderCapability, ProviderState, RankingEligibility, RankingState, Team,
    TeamMembership, TeamMembershipState, TeamVisibility, WebhookDelivery, WebhookDeliveryState,
    WebhookEventType, WebhookSubscription, WebhookSubscriptionState,
};
pub use rbac::{AccessDecision, AccessDenial, AccessRequest, RbacEvaluator};
pub use shared::{CurrencyCode, Money, Visibility};
pub use systems::{
    CalculationSettings, CapacityPeriod, ChannelDataType, ChannelDefinition, ChannelDisplay,
    ChannelLifecycle, ChannelScale, EffectivePeriod, Equipment, EquipmentKind, GeographicPrecision,
    NetCalculationMode, PowerCalculationMode, PvSystem, SystemLifecycle, SystemPrivacy, Tariff,
    TariffDirection,
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
