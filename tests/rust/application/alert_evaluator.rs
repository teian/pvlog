use async_trait::async_trait;
use pvlog_application::{
    AlertEvaluationState, AlertEvaluator, AlertEvaluatorError, AlertEvaluatorRepository,
    AlertMetrics, AlertTransition, Clock,
};
use pvlog_domain::{
    AccountId, AlertKind, AlertRule, AlertRuleId, AlertSchedule, IanaTimezone, SystemId,
    UtcTimestamp, Watts,
};
use std::{
    collections::BTreeSet,
    error::Error,
    str::FromStr as _,
    sync::{Arc, Mutex},
};

#[tokio::test]
async fn debounce_deduplication_recovery_cooldown_and_lag_are_clock_controlled()
-> Result<(), Box<dyn Error>> {
    let clock = Arc::new(TestClock(Mutex::new(at(10_000)?)));
    let repository = Arc::new(MemoryRepository::default());
    let evaluator = AlertEvaluator::new(repository.clone(), clock.clone());
    let rule = rule()?;
    let triggered = AlertMetrics {
        consumption_watts: Some(2_000),
        ..AlertMetrics::default()
    };
    let clear = AlertMetrics {
        consumption_watts: Some(10),
        ..AlertMetrics::default()
    };

    assert_eq!(
        evaluator.evaluate(&rule, &triggered, at(9_000)?).await?,
        AlertTransition::None
    );
    clock.set(at(12_999)?)?;
    assert_eq!(
        evaluator.evaluate(&rule, &triggered, at(12_900)?).await?,
        AlertTransition::None
    );
    clock.set(at(13_000)?)?;
    assert!(matches!(
        evaluator.evaluate(&rule, &triggered, at(12_900)?).await?,
        AlertTransition::Opened(_)
    ));
    assert_eq!(
        evaluator.evaluate(&rule, &triggered, at(13_000)?).await?,
        AlertTransition::None
    );
    assert!(matches!(
        evaluator.evaluate(&rule, &clear, at(13_000)?).await?,
        AlertTransition::Recovered(_)
    ));

    clock.set(at(14_000)?)?;
    assert_eq!(
        evaluator.evaluate(&rule, &triggered, at(14_000)?).await?,
        AlertTransition::None
    );
    clock.set(at(18_000)?)?;
    assert_eq!(
        evaluator.evaluate(&rule, &triggered, at(18_000)?).await?,
        AlertTransition::None
    );
    clock.set(at(21_000)?)?;
    assert!(matches!(
        evaluator.evaluate(&rule, &triggered, at(20_500)?).await?,
        AlertTransition::Opened(_)
    ));

    let transitions = repository
        .transitions
        .lock()
        .map_err(|_| "transitions lock")?;
    assert_eq!(
        transitions
            .iter()
            .filter(|item| !matches!(item, AlertTransition::None))
            .count(),
        3
    );
    drop(transitions);
    let metrics = evaluator.metrics()?;
    assert_eq!(metrics.evaluations, 8);
    assert_eq!(metrics.maximum_lag_milliseconds, 1_000);
    Ok(())
}

fn rule() -> Result<AlertRule, Box<dyn Error>> {
    Ok(AlertRule {
        id: AlertRuleId::new(),
        account_id: AccountId::new(),
        system_id: SystemId::new(),
        name: "Consumption".to_owned(),
        kind: AlertKind::ConsumptionAbove {
            threshold: Watts::new(1_000),
        },
        schedule: AlertSchedule {
            timezone: IanaTimezone::from_str("UTC")?,
            weekdays: BTreeSet::from([1, 2, 3, 4, 5, 6, 7]),
            start_minute_local: 0,
            end_minute_local: 1_440,
        },
        debounce_seconds: 3,
        cooldown_seconds: 5,
        delivery_channels: BTreeSet::new(),
        enabled: true,
    })
}
fn at(milliseconds: i64) -> Result<UtcTimestamp, pvlog_domain::ValidationError> {
    UtcTimestamp::from_epoch_millis(milliseconds)
}

struct TestClock(Mutex<UtcTimestamp>);
impl TestClock {
    fn set(&self, value: UtcTimestamp) -> Result<(), &'static str> {
        *self.0.lock().map_err(|_| "clock lock")? = value;
        Ok(())
    }
}
impl Clock for TestClock {
    fn now(&self) -> UtcTimestamp {
        self.0.lock().map_or_else(
            |_| UtcTimestamp::new(time::OffsetDateTime::UNIX_EPOCH),
            |value| *value,
        )
    }
}

#[derive(Default)]
struct MemoryRepository {
    state: Mutex<AlertEvaluationState>,
    transitions: Mutex<Vec<AlertTransition>>,
}
#[async_trait]
impl AlertEvaluatorRepository for MemoryRepository {
    async fn state(
        &self,
        _rule_id: AlertRuleId,
    ) -> Result<AlertEvaluationState, AlertEvaluatorError> {
        self.state
            .lock()
            .map(|state| state.clone())
            .map_err(|_| AlertEvaluatorError::Unavailable)
    }
    async fn commit(
        &self,
        _rule_id: AlertRuleId,
        state: &AlertEvaluationState,
        transition: &AlertTransition,
    ) -> Result<(), AlertEvaluatorError> {
        *self
            .state
            .lock()
            .map_err(|_| AlertEvaluatorError::Unavailable)? = state.clone();
        self.transitions
            .lock()
            .map_err(|_| AlertEvaluatorError::Unavailable)?
            .push(transition.clone());
        Ok(())
    }
}
