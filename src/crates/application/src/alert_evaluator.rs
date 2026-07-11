//! Stateful alert evaluation with transactional event queuing.

use crate::{AlertEvaluation, AlertMetrics, Clock, evaluate_alert};
use async_trait::async_trait;
use pvlog_domain::{
    AlertEvent, AlertEventId, AlertEventState, AlertRule, AlertRuleId, UtcTimestamp,
};
use std::sync::{Arc, Mutex};
use thiserror::Error;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AlertEvaluationState {
    pub pending_since: Option<UtcTimestamp>,
    pub open_event: Option<AlertEvent>,
    pub last_resolved_at: Option<UtcTimestamp>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AlertTransition {
    None,
    Opened(AlertEvent),
    Recovered(AlertEvent),
}

#[async_trait]
pub trait AlertEvaluatorRepository: Send + Sync {
    async fn state(
        &self,
        rule_id: AlertRuleId,
    ) -> Result<AlertEvaluationState, AlertEvaluatorError>;
    /// Atomically persists evaluator state and queues the transition for downstream delivery.
    async fn commit(
        &self,
        rule_id: AlertRuleId,
        state: &AlertEvaluationState,
        transition: &AlertTransition,
    ) -> Result<(), AlertEvaluatorError>;
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AlertEvaluatorMetrics {
    pub evaluations: u64,
    pub maximum_lag_milliseconds: u64,
}

pub struct AlertEvaluator<R, C> {
    repository: Arc<R>,
    clock: Arc<C>,
    metrics: Mutex<AlertEvaluatorMetrics>,
}

impl<R: AlertEvaluatorRepository, C: Clock> AlertEvaluator<R, C> {
    #[must_use]
    pub fn new(repository: Arc<R>, clock: Arc<C>) -> Self {
        Self {
            repository,
            clock,
            metrics: Mutex::new(AlertEvaluatorMetrics::default()),
        }
    }

    /// Evaluates one rule and transactionally records any opened or recovery event.
    ///
    /// # Errors
    ///
    /// Returns an error when evaluator state cannot be loaded or atomically committed.
    pub async fn evaluate(
        &self,
        rule: &AlertRule,
        values: &AlertMetrics,
        evaluated_through: UtcTimestamp,
    ) -> Result<AlertTransition, AlertEvaluatorError> {
        let now = self.clock.now();
        self.observe_lag(now, evaluated_through)?;
        let mut state = self.repository.state(rule.id).await?;
        let transition = match evaluate_alert(rule, values, evaluated_through) {
            AlertEvaluation::Triggered => triggered(rule, now, &mut state),
            AlertEvaluation::Clear => recovered(now, &mut state),
            AlertEvaluation::OutsideSchedule | AlertEvaluation::InsufficientData => {
                AlertTransition::None
            }
        };
        self.repository.commit(rule.id, &state, &transition).await?;
        Ok(transition)
    }

    /// Returns a safe snapshot of evaluator counters.
    ///
    /// # Errors
    ///
    /// Returns an error if the metrics lock has been poisoned.
    pub fn metrics(&self) -> Result<AlertEvaluatorMetrics, AlertEvaluatorError> {
        self.metrics
            .lock()
            .map(|metrics| metrics.clone())
            .map_err(|_| AlertEvaluatorError::Unavailable)
    }

    fn observe_lag(
        &self,
        now: UtcTimestamp,
        through: UtcTimestamp,
    ) -> Result<(), AlertEvaluatorError> {
        let lag = now.epoch_millis().saturating_sub(through.epoch_millis());
        let lag = u64::try_from(lag).unwrap_or_default();
        let mut metrics = self
            .metrics
            .lock()
            .map_err(|_| AlertEvaluatorError::Unavailable)?;
        metrics.evaluations = metrics.evaluations.saturating_add(1);
        metrics.maximum_lag_milliseconds = metrics.maximum_lag_milliseconds.max(lag);
        Ok(())
    }
}

fn triggered(
    rule: &AlertRule,
    now: UtcTimestamp,
    state: &mut AlertEvaluationState,
) -> AlertTransition {
    if state.open_event.is_some() {
        return AlertTransition::None;
    }
    if state
        .last_resolved_at
        .is_some_and(|resolved| elapsed(resolved, now) < u64::from(rule.cooldown_seconds) * 1_000)
    {
        return AlertTransition::None;
    }
    let pending = *state.pending_since.get_or_insert(now);
    if elapsed(pending, now) < u64::from(rule.debounce_seconds) * 1_000 {
        return AlertTransition::None;
    }
    let event = AlertEvent {
        id: AlertEventId::new(),
        rule_id: rule.id,
        system_id: rule.system_id,
        opened_at: now,
        resolved_at: None,
        state: AlertEventState::Open,
        deduplication_key: format!("alert:{}:open", rule.id),
        safe_context: serde_json::json!({"rule_name": rule.name}),
    };
    state.pending_since = None;
    state.open_event = Some(event.clone());
    AlertTransition::Opened(event)
}

fn recovered(now: UtcTimestamp, state: &mut AlertEvaluationState) -> AlertTransition {
    state.pending_since = None;
    let Some(mut event) = state.open_event.take() else {
        return AlertTransition::None;
    };
    event.resolved_at = Some(now);
    event.state = AlertEventState::Resolved;
    event.deduplication_key = format!("alert:{}:resolved:{}", event.rule_id, event.id);
    state.last_resolved_at = Some(now);
    AlertTransition::Recovered(event)
}

fn elapsed(start: UtcTimestamp, end: UtcTimestamp) -> u64 {
    u64::try_from(end.epoch_millis().saturating_sub(start.epoch_millis())).unwrap_or_default()
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum AlertEvaluatorError {
    #[error("alert evaluator repository is unavailable")]
    Unavailable,
}
