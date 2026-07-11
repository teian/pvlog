//! Timezone-aware alert rule management and condition evaluation.

use async_trait::async_trait;
use chrono::{DateTime, Datelike as _, Timelike as _, Utc};
use chrono_tz::Tz;
use pvlog_domain::{
    AccountId, AlertKind, AlertRule, AlertRuleId, AlertSchedule, SystemId, UtcTimestamp,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    str::FromStr as _,
};
use thiserror::Error;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CreateAlertRule {
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

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct UpdateAlertRule {
    pub name: Option<String>,
    pub kind: Option<AlertKind>,
    pub schedule: Option<AlertSchedule>,
    pub enabled: Option<bool>,
}

#[async_trait]
pub trait AlertRuleRepository: Send + Sync {
    async fn save(&self, rule: &AlertRule) -> Result<(), AlertRuleServiceError>;
    async fn find(
        &self,
        account_id: AccountId,
        id: AlertRuleId,
    ) -> Result<Option<AlertRule>, AlertRuleServiceError>;
    async fn list(
        &self,
        account_id: AccountId,
        system_id: SystemId,
    ) -> Result<Vec<AlertRule>, AlertRuleServiceError>;
    async fn delete(
        &self,
        account_id: AccountId,
        id: AlertRuleId,
    ) -> Result<bool, AlertRuleServiceError>;
}

pub struct AlertRuleService<R> {
    repository: R,
}
#[allow(clippy::missing_errors_doc)]
impl<R: AlertRuleRepository> AlertRuleService<R> {
    #[must_use]
    pub const fn new(repository: R) -> Self {
        Self { repository }
    }
    pub async fn create(&self, input: CreateAlertRule) -> Result<AlertRule, AlertRuleServiceError> {
        validate(&input.name, &input.schedule)?;
        let rule = AlertRule {
            id: AlertRuleId::new(),
            account_id: input.account_id,
            system_id: input.system_id,
            name: input.name,
            kind: input.kind,
            schedule: input.schedule,
            debounce_seconds: input.debounce_seconds,
            cooldown_seconds: input.cooldown_seconds,
            delivery_channels: input.delivery_channels,
            enabled: input.enabled,
        };
        self.repository.save(&rule).await?;
        Ok(rule)
    }
    pub async fn get(
        &self,
        account_id: AccountId,
        id: AlertRuleId,
    ) -> Result<AlertRule, AlertRuleServiceError> {
        self.repository
            .find(account_id, id)
            .await?
            .ok_or(AlertRuleServiceError::NotFound)
    }
    pub async fn list(
        &self,
        account_id: AccountId,
        system_id: SystemId,
    ) -> Result<Vec<AlertRule>, AlertRuleServiceError> {
        self.repository.list(account_id, system_id).await
    }
    pub async fn update(
        &self,
        account_id: AccountId,
        id: AlertRuleId,
        update: UpdateAlertRule,
    ) -> Result<AlertRule, AlertRuleServiceError> {
        let mut rule = self.get(account_id, id).await?;
        if let Some(name) = update.name {
            rule.name = name;
        }
        if let Some(kind) = update.kind {
            rule.kind = kind;
        }
        if let Some(schedule) = update.schedule {
            rule.schedule = schedule;
        }
        if let Some(enabled) = update.enabled {
            rule.enabled = enabled;
        }
        validate(&rule.name, &rule.schedule)?;
        self.repository.save(&rule).await?;
        Ok(rule)
    }
    pub async fn delete(
        &self,
        account_id: AccountId,
        id: AlertRuleId,
    ) -> Result<(), AlertRuleServiceError> {
        if self.repository.delete(account_id, id).await? {
            Ok(())
        } else {
            Err(AlertRuleServiceError::NotFound)
        }
    }
}

fn validate(name: &str, schedule: &AlertSchedule) -> Result<(), AlertRuleServiceError> {
    if name.trim().is_empty() {
        return Err(AlertRuleServiceError::Invalid("name is required"));
    }
    if schedule.weekdays.iter().any(|day| !(1..=7).contains(day))
        || schedule.start_minute_local >= 1_440
        || schedule.end_minute_local > 1_440
    {
        return Err(AlertRuleServiceError::Invalid("schedule is invalid"));
    }
    Ok(())
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AlertMetrics {
    pub idle_seconds: Option<u32>,
    pub generation_watts: Option<i64>,
    pub consumption_watts: Option<i64>,
    pub net_power_watts: Option<i64>,
    pub standby_cost_milli_cents: Option<i64>,
    pub performance_basis_points: Option<u16>,
    pub battery_basis_points: Option<u16>,
    pub daily_energy_wh: Option<i64>,
    pub extended_scaled: BTreeMap<String, i64>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AlertEvaluation {
    Triggered,
    Clear,
    OutsideSchedule,
    InsufficientData,
}

#[must_use]
pub fn evaluate_alert(
    rule: &AlertRule,
    metrics: &AlertMetrics,
    at: UtcTimestamp,
) -> AlertEvaluation {
    if !rule.enabled || !within_schedule(&rule.schedule, at) {
        return AlertEvaluation::OutsideSchedule;
    }
    let triggered = match &rule.kind {
        AlertKind::Idle { after_seconds } | AlertKind::MissingGeneration { after_seconds } => {
            metrics.idle_seconds.map(|value| value >= *after_seconds)
        }
        AlertKind::GenerationBelow { threshold } => metrics
            .generation_watts
            .map(|value| value < threshold.value()),
        AlertKind::ConsumptionAbove { threshold } => metrics
            .consumption_watts
            .map(|value| value > threshold.value()),
        AlertKind::NetPowerAbove { threshold } => metrics
            .net_power_watts
            .map(|value| value > threshold.value()),
        AlertKind::StandbyCostAbove {
            threshold_milli_cents,
        } => metrics
            .standby_cost_milli_cents
            .map(|value| value > *threshold_milli_cents),
        AlertKind::PerformanceBelow { threshold } => metrics
            .performance_basis_points
            .map(|value| i32::from(value) < threshold.value()),
        AlertKind::BatteryStateBelow { threshold } => metrics
            .battery_basis_points
            .map(|value| i32::from(value) < threshold.value()),
        AlertKind::DailyEnergyBelow { threshold } => metrics
            .daily_energy_wh
            .map(|value| value < threshold.value()),
        AlertKind::ExtendedBelow {
            channel_key,
            scaled_value,
        } => metrics
            .extended_scaled
            .get(channel_key)
            .map(|value| value < scaled_value),
        AlertKind::ExtendedAbove {
            channel_key,
            scaled_value,
        } => metrics
            .extended_scaled
            .get(channel_key)
            .map(|value| value > scaled_value),
    };
    match triggered {
        Some(true) => AlertEvaluation::Triggered,
        Some(false) => AlertEvaluation::Clear,
        None => AlertEvaluation::InsufficientData,
    }
}

fn within_schedule(schedule: &AlertSchedule, at: UtcTimestamp) -> bool {
    let Ok(timezone) = Tz::from_str(schedule.timezone.as_str()) else {
        return false;
    };
    let Ok(epoch) = i64::try_from(at.epoch_millis()) else {
        return false;
    };
    let Some(utc) = DateTime::<Utc>::from_timestamp_millis(epoch) else {
        return false;
    };
    let local = utc.with_timezone(&timezone);
    let weekday = u8::try_from(local.weekday().number_from_monday()).unwrap_or_default();
    let minute = u16::try_from(local.hour() * 60 + local.minute()).unwrap_or_default();
    schedule.weekdays.contains(&weekday)
        && if schedule.start_minute_local <= schedule.end_minute_local {
            minute >= schedule.start_minute_local && minute < schedule.end_minute_local
        } else {
            minute >= schedule.start_minute_local || minute < schedule.end_minute_local
        }
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum AlertRuleServiceError {
    #[error("alert rule was not found")]
    NotFound,
    #[error("alert rule is invalid: {0}")]
    Invalid(&'static str),
    #[error("alert rule repository is unavailable")]
    Unavailable,
}
