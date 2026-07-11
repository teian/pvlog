//! `PVLog` application use cases and ports.

#![forbid(unsafe_code)]

mod alert_evaluator;
mod alert_rules;
mod analytics_api;
mod api_token;
mod authorization_boundary;
mod batch_ingestion;
mod browser_session;
mod community_catalog;
mod comparison;
mod data_quality;
mod identity_linking;
mod import_export;
mod ingestion_admission;
mod ingestion_normalization;
mod ingestion_validation;
mod legacy_credential;
mod local_password;
mod managed_resource;
mod modern_telemetry;
mod oauth2_connector;
mod observation_correction;
mod oidc;
mod pagination;
mod ports;
mod provider_data;
mod query_planner;
mod rate_limit;
mod rbac_management;
mod series_query;
mod statistics;
mod system_configuration;
mod system_lifecycle;
mod teams;
mod user_lifecycle;
mod webhooks;

pub use alert_evaluator::{
    AlertEvaluationState, AlertEvaluator, AlertEvaluatorError, AlertEvaluatorMetrics,
    AlertEvaluatorRepository, AlertTransition,
};
pub use alert_rules::{
    AlertEvaluation, AlertMetrics, AlertRuleRepository, AlertRuleService, AlertRuleServiceError,
    CreateAlertRule, UpdateAlertRule, evaluate_alert,
};
pub use analytics_api::{
    AnalysisExportFormat, AnalysisExportRequest, AnalysisExportResult, ModernAnalyticsError,
    ModernAnalyticsUseCases,
};
pub use api_token::{ApiToken, ApiTokenError, ApiTokenRecord, ApiTokenRepository, ApiTokenService};
pub use authorization_boundary::{
    AuthorizationBoundary, AuthorizationBoundaryError, AuthorizationBoundaryPorts,
    AuthorizedAccountRoute, ProtectedAccountRequest,
};
pub use batch_ingestion::{
    BatchIngestionError, BatchIngestionMode, BatchIngestionRepository, BatchIngestionResult,
    BatchIngestionService, BatchItemOutcome, BatchItemStatus,
};
pub use browser_session::{
    BrowserSession, BrowserSessionError, BrowserSessionPolicy, BrowserSessionRecord,
    BrowserSessionRepository, BrowserSessionService, BrowserSessionUseCases, SessionCookie,
};
pub use community_catalog::{
    CommunityActivity, CommunityCatalogError, CommunityCatalogPolicy, CommunityCatalogRepository,
    CommunityCatalogService, CommunityCatalogUseCases, CommunityLocationPrecision,
    CommunityProjection, CommunityProjectionEvent, CommunitySearchFilter, CommunitySystemResource,
};
pub use comparison::{
    CapacityPerformanceSlice, ComparisonCandidate, ComparisonEntry, ComparisonError,
    ComparisonMetric, ComparisonPolicy, build_ladder, compare_systems,
};
pub use data_quality::{
    DataQualityError, DataQualityIssue, DataQualityKind, DataQualityPolicy, QualitySample,
    RejectedIngestion, detect_data_quality,
};
pub use identity_linking::{
    ExternalIdentityLinkingError, ExternalIdentityLinkingRepository,
    ExternalIdentityLinkingService, ExternalIdentityLinkingUseCases, ExternalLoginOutcome,
    ExternalLoginPolicy, LinkExternalIdentity, LinkedIdentityRecord, UnlinkExternalIdentity,
};
pub use import_export::{
    ExportJobResource, ImportExportError, ImportExportRepository, ImportExportService, ImportPlan,
    ImportValidationIssue,
};
pub use ingestion_admission::{
    IngestionAdmission, IngestionAdmissionError, IngestionAdmissionMetrics, IngestionPermit,
};
pub use ingestion_normalization::{
    EnergyInput, EnergyUnit, IngestionNormalizationError, NormalizeObservation, PowerUnit,
    normalize_observation,
};
pub use ingestion_validation::{
    IngestionValidationError, IngestionValidationPolicy, validate_observation,
};
pub use legacy_credential::{
    LegacyCredentialError, LegacyCredentialInput, LegacyCredentialPolicy, LegacyCredentialRecord,
    LegacyCredentialRepository, LegacyCredentialService, LegacyPrincipal,
};
pub use local_password::{
    Argon2CredentialConfig, Argon2CredentialService, AuthenticatePassword, AuthenticationOutcome,
    ChangePassword, CommonPasswordHook, DiscardingRecoveryNotifier, LocalCredentialRecord,
    LocalCredentialRepository, LocalPasswordPolicy, LocalPasswordService, LocalPasswordUseCases,
    PasswordPolicyError, PasswordPolicyHook, PasswordRecoveryNotifier, PasswordRecoveryRecord,
    PasswordServiceError, SetInitialPassword,
};
pub use managed_resource::{
    CreateManagedResource, ManagedResource, ManagedResourceError, ManagedResourceKind,
    ManagedResourceService, ModernApiActor,
};
pub use modern_telemetry::{ModernTelemetryError, ModernTelemetryUseCases};
pub use oauth2_connector::{
    EncryptedProviderToken, OAuth2AuthorizationRequest, OAuth2ClaimMappings,
    OAuth2ClientAuthMethod, OAuth2ConnectorSettings, OAuth2ProtocolClient, OAuth2ProtocolError,
    OAuth2UserInfo, ProtectedOAuth2Tokens, ProviderTokenKind, TokenCipher, TokenCipherConfigError,
    XChaCha20Poly1305TokenCipher,
};
pub use observation_correction::{
    CorrectObservation, CorrectionRepository, CorrectionService, ObservationCorrectionError,
    VersionedObservation,
};
pub use oidc::{
    OidcAuthorizationRequest, OidcConnectorSettings, OidcProtocolClient, OidcProtocolError,
};
pub use pagination::{CursorPosition, PageCursorCodec, PaginationError};
pub use ports::{
    AuthorizationRequest, Clock, CredentialService, EntityRepository, IdentityClaims,
    IdentityService, InsolationPoint, InsolationProvider, JobQueue, PortError, SecretResolver,
    SupplyPoint, SupplyProvider, Transaction, UnitOfWork, WebhookRequest, WebhookResponse,
    WebhookSender,
};
pub use provider_data::{
    CircuitBreaker, CircuitBreakerPolicy, CircuitState, ConfiguredExternalDataAdapter,
    ConfiguredExternalDataService, ExternalDataCacheEntry, ExternalDataCacheKey,
    ExternalDataCacheRepository, ExternalDataConfiguration, ExternalDataFreshness,
    ExternalDataKind, ExternalDataLicense, ExternalDataProvenance, ExternalDataRequest,
    ExternalDataResult, ProviderConfigurationError,
};
pub use query_planner::{
    QueryPlan, QueryPlanError, QueryPlanRequest, QueryResolution, QuerySource, RawSources,
    RequestedResolution, SeriesField, plan_query,
};
pub use rate_limit::{
    PrincipalQuota, RateLimitDecision, RateLimitError, RateLimitMetadata, RateLimitRepository,
    RateLimitService,
};
pub use rbac_management::{
    AssignRole, CreateCustomRole, RbacManagementError, RbacRepository, RbacRoleRecord,
    RoleManagementService, UpdateCustomRole, built_in_account_roles,
};
pub use series_query::{
    GapKind, PlannedSeries, SeriesGap, SeriesPoint, SeriesQueryError, SeriesQueryRepository,
    SeriesQueryRepositoryError, SeriesQueryResult, SeriesQueryService, SeriesUnit,
};
pub use statistics::{
    EnergyStatistics, StatisticsBucket, StatisticsError, StatisticsPeriod, calculate_statistics,
};
pub use system_configuration::{
    SystemConfigurationError, SystemConfigurationRepository, SystemConfigurationService,
};
pub use system_lifecycle::{
    CreateSystem, SystemLifecycleError, SystemLifecycleRecord, SystemLifecycleRepository,
    SystemLifecycleService, SystemLifecycleUseCases, UpdateSystem,
};
pub use teams::{
    CreateTeam, JoinTeam, TeamAccess, TeamAggregateResource, TeamEligibility,
    TeamMembershipResource, TeamPolicy, TeamRepository, TeamResource, TeamService,
    TeamServiceError, TeamUseCases,
};
pub use user_lifecycle::{
    AdminUserActor, CreateLocalUser, InvitationRecord, InvitationResult, InviteLocalUser,
    LifecycleCreateOutcome, LifecycleUserRecord, LocalUserPolicy, PublicLifecycleOutcome,
    RegisterLocalUser, UserLifecycleError, UserLifecycleRepository, UserLifecycleService,
    UserLifecycleUseCases,
};
pub use webhooks::{
    CreateWebhookSubscription, PendingWebhookVerification, RotateWebhookSecret, SignedWebhookEvent,
    StoredWebhookSubscription, WebhookEventEnvelope, WebhookReplayPolicy, WebhookService,
    WebhookServiceError, WebhookSubscriptionRepository, sign_webhook_event,
};
