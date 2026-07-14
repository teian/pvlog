use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use url::Url;

use crate::{
    AccountId, AlertEventId, AlertRuleId, BasisPoints, ExportId, IanaTimezone, ImportId, JobId,
    ProviderId, SystemId, UserId, UtcTimestamp, WattHours, Watts, WebhookDeliveryId,
    WebhookSubscriptionId,
};

/// Timezone-aware alert rule with debounce and cooldown.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AlertRule {
    pub id: AlertRuleId,
    pub account_id: AccountId,
    pub system_id: SystemId,
    pub name: String,
    pub kind: AlertKind,
    pub schedule: AlertSchedule,
    pub debounce_seconds: u32,
    pub cooldown_seconds: u32,
    pub delivery_channels: BTreeSet<String>,
    pub enabled: bool,
}

/// Supported threshold or data-quality condition.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AlertKind {
    Idle {
        after_seconds: u32,
    },
    MissingGeneration {
        after_seconds: u32,
    },
    GenerationBelow {
        threshold: Watts,
    },
    ConsumptionAbove {
        threshold: Watts,
    },
    NetPowerAbove {
        threshold: Watts,
    },
    StandbyCostAbove {
        threshold_milli_cents: i64,
    },
    PerformanceBelow {
        threshold: BasisPoints,
    },
    BatteryStateBelow {
        threshold: BasisPoints,
    },
    DailyEnergyBelow {
        threshold: WattHours,
    },
    ExtendedBelow {
        channel_key: String,
        scaled_value: i64,
    },
    ExtendedAbove {
        channel_key: String,
        scaled_value: i64,
    },
}

/// Local-time window in which a rule is evaluated.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AlertSchedule {
    pub timezone: IanaTimezone,
    pub weekdays: BTreeSet<u8>,
    pub start_minute_local: u16,
    pub end_minute_local: u16,
}

/// One deduplicated alert occurrence.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AlertEvent {
    pub id: AlertEventId,
    pub rule_id: AlertRuleId,
    pub system_id: SystemId,
    pub opened_at: UtcTimestamp,
    pub resolved_at: Option<UtcTimestamp>,
    pub state: AlertEventState,
    pub deduplication_key: String,
    pub safe_context: serde_json::Value,
}

/// Alert event lifecycle.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AlertEventState {
    Open,
    Acknowledged,
    Resolved,
}

/// Verified webhook destination and event selection.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct WebhookSubscription {
    pub id: WebhookSubscriptionId,
    pub account_id: AccountId,
    pub endpoint: Url,
    pub events: BTreeSet<WebhookEventType>,
    pub state: WebhookSubscriptionState,
    pub signing_key_reference: String,
    pub created_at: UtcTimestamp,
}

/// Webhook lifecycle while ownership is proven.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WebhookSubscriptionState {
    PendingVerification,
    Active,
    Disabled,
}

/// Stable outbound webhook event category.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WebhookEventType {
    ObservationAccepted,
    AlertOpened,
    AlertResolved,
    SystemChanged,
    ExportReady,
}

/// Durable delivery record for exactly one subscription and event.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct WebhookDelivery {
    pub id: WebhookDeliveryId,
    pub subscription_id: WebhookSubscriptionId,
    pub event_id: String,
    pub schema_version: u16,
    pub state: WebhookDeliveryState,
    pub attempts: Vec<DeliveryAttempt>,
    pub next_attempt_at: Option<UtcTimestamp>,
}

/// Webhook delivery lifecycle.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WebhookDeliveryState {
    Pending,
    Delivered,
    Retrying,
    DeadLetter,
}

/// Safe response metadata from one delivery attempt.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DeliveryAttempt {
    pub attempted_at: UtcTimestamp,
    pub response_status: Option<u16>,
    pub error_class: Option<String>,
    pub duration_milliseconds: u32,
}

/// Generic external data connector configuration without embedded credentials.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Provider {
    pub id: ProviderId,
    pub account_id: Option<AccountId>,
    pub name: String,
    pub capabilities: BTreeSet<ProviderCapability>,
    pub credential_reference: Option<String>,
    pub configuration: serde_json::Value,
    pub state: ProviderState,
    pub last_success_at: Option<UtcTimestamp>,
}

/// Provider-neutral external data capability.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderCapability {
    DevicePolling,
    Insolation,
    RegionalSupply,
    WeatherForecast,
    WeatherObserved,
    WeatherReanalysis,
    NotificationDelivery,
}

/// External provider health state.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderState {
    Disabled,
    Healthy,
    Degraded { reason_code: String },
    Unavailable { reason_code: String },
}

/// Durable background work item carrying only safe references.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Job {
    pub id: JobId,
    pub account_id: Option<AccountId>,
    pub kind: JobKind,
    pub state: JobState,
    pub payload: serde_json::Value,
    pub idempotency_key: String,
    pub attempts: u16,
    pub maximum_attempts: u16,
    pub scheduled_at: UtcTimestamp,
    pub lease_expires_at: Option<UtcTimestamp>,
}

/// Background workload category.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum JobKind {
    RebuildRollup,
    ArchiveTelemetry,
    DeliverWebhook,
    PollProvider,
    PollWeatherProvider,
    CalculateYieldForecast,
    RebuildYieldIntervals,
    ReconcileProjection,
    Import,
    Export,
}

/// Durable job lifecycle.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    Pending,
    Running { worker_id: String },
    RetryScheduled { reason_code: String },
    Completed,
    Failed { reason_code: String },
    Cancelled,
}

/// Validated import workflow metadata.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ImportRequest {
    pub id: ImportId,
    pub account_id: AccountId,
    pub source_name: String,
    pub content_hash: [u8; 32],
    pub dry_run: bool,
    pub state: ImportState,
    pub created_at: UtcTimestamp,
}

/// Import validation and commit lifecycle.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportState {
    Uploaded,
    Validating,
    Validated,
    Committing,
    Completed,
    Rejected,
}

/// Asynchronous export workflow metadata.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ExportRequest {
    pub id: ExportId,
    pub account_id: AccountId,
    pub requested_by: UserId,
    pub format: ExportFormat,
    pub state: ExportState,
    pub created_at: UtcTimestamp,
    pub expires_at: Option<UtcTimestamp>,
}

/// Stable export representation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportFormat {
    Csv,
    Json,
    PortableBundle,
}

/// Export preparation lifecycle.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportState {
    Pending,
    Running,
    Ready { artifact_reference: String },
    Failed { reason_code: String },
    Expired,
}
