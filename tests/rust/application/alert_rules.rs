use async_trait::async_trait;
use pvlog_application::{
    AlertEvaluation, AlertMetrics, AlertRuleRepository, AlertRuleService, AlertRuleServiceError,
    CreateAlertRule, UpdateAlertRule, evaluate_alert,
};
use pvlog_domain::{
    AccountId, AlertKind, AlertRule, AlertRuleId, AlertSchedule, BasisPoints, IanaTimezone,
    SystemId, UtcTimestamp, WattHours, Watts,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    str::FromStr as _,
    sync::Mutex,
};

#[tokio::test]
async fn alert_rule_crud_is_account_scoped_and_validated() -> Result<(), Box<dyn Error>> {
    let account_id = AccountId::new();
    let system_id = SystemId::new();
    let service = AlertRuleService::new(MemoryRepository::default());
    let created = service
        .create(CreateAlertRule {
            account_id,
            system_id,
            name: "High load".to_owned(),
            kind: AlertKind::ConsumptionAbove {
                threshold: Watts::new(2_000),
            },
            schedule: schedule()?,
            debounce_seconds: 60,
            cooldown_seconds: 300,
            delivery_channels: BTreeSet::from(["webhook".to_owned()]),
            enabled: true,
        })
        .await?;
    assert_eq!(service.list(account_id, system_id).await?.len(), 1);
    let updated = service
        .update(
            account_id,
            created.id,
            UpdateAlertRule {
                name: Some("Very high load".to_owned()),
                enabled: Some(false),
                ..UpdateAlertRule::default()
            },
        )
        .await?;
    assert_eq!(updated.name, "Very high load");
    assert!(!updated.enabled);
    service.delete(account_id, created.id).await?;
    assert_eq!(
        service.get(account_id, created.id).await,
        Err(AlertRuleServiceError::NotFound)
    );
    Ok(())
}

#[test]
fn all_alert_conditions_evaluate_inside_the_configured_timezone_window()
-> Result<(), Box<dyn Error>> {
    let metrics = AlertMetrics {
        idle_seconds: Some(600),
        generation_watts: Some(10),
        consumption_watts: Some(5_000),
        net_power_watts: Some(3_000),
        standby_cost_milli_cents: Some(500),
        performance_basis_points: Some(2_000),
        battery_basis_points: Some(1_000),
        daily_energy_wh: Some(100),
        extended_scaled: BTreeMap::from([("temperature".to_owned(), 50)]),
    };
    let kinds = vec![
        AlertKind::Idle { after_seconds: 300 },
        AlertKind::MissingGeneration { after_seconds: 300 },
        AlertKind::GenerationBelow {
            threshold: Watts::new(100),
        },
        AlertKind::ConsumptionAbove {
            threshold: Watts::new(1_000),
        },
        AlertKind::NetPowerAbove {
            threshold: Watts::new(1_000),
        },
        AlertKind::StandbyCostAbove {
            threshold_milli_cents: 100,
        },
        AlertKind::PerformanceBelow {
            threshold: BasisPoints::new(5_000)?,
        },
        AlertKind::BatteryStateBelow {
            threshold: BasisPoints::new(2_000)?,
        },
        AlertKind::DailyEnergyBelow {
            threshold: WattHours::new(1_000),
        },
        AlertKind::ExtendedBelow {
            channel_key: "temperature".to_owned(),
            scaled_value: 100,
        },
        AlertKind::ExtendedAbove {
            channel_key: "temperature".to_owned(),
            scaled_value: 10,
        },
    ];
    let at = UtcTimestamp::from_epoch_millis(1_704_110_400_000)?;
    for kind in kinds {
        assert_eq!(
            evaluate_alert(&rule(kind, schedule()?), &metrics, at),
            AlertEvaluation::Triggered
        );
    }
    let mut outside = schedule()?;
    outside.weekdays = BTreeSet::from([2]);
    assert_eq!(
        evaluate_alert(
            &rule(AlertKind::Idle { after_seconds: 1 }, outside),
            &metrics,
            at
        ),
        AlertEvaluation::OutsideSchedule
    );
    Ok(())
}

fn schedule() -> Result<AlertSchedule, pvlog_domain::ValidationError> {
    Ok(AlertSchedule {
        timezone: IanaTimezone::from_str("Europe/Berlin")?,
        weekdays: BTreeSet::from([1]),
        start_minute_local: 0,
        end_minute_local: 1_440,
    })
}
fn rule(kind: AlertKind, schedule: AlertSchedule) -> AlertRule {
    AlertRule {
        id: AlertRuleId::new(),
        account_id: AccountId::new(),
        system_id: SystemId::new(),
        name: "test".to_owned(),
        kind,
        schedule,
        debounce_seconds: 0,
        cooldown_seconds: 0,
        delivery_channels: BTreeSet::new(),
        enabled: true,
    }
}

#[derive(Default)]
struct MemoryRepository {
    rules: Mutex<Vec<AlertRule>>,
}
#[async_trait]
impl AlertRuleRepository for MemoryRepository {
    async fn save(&self, rule: &AlertRule) -> Result<(), AlertRuleServiceError> {
        let mut rules = self
            .rules
            .lock()
            .map_err(|_| AlertRuleServiceError::Unavailable)?;
        if let Some(existing) = rules.iter_mut().find(|item| item.id == rule.id) {
            existing.clone_from(rule);
        } else {
            rules.push(rule.clone());
        }
        Ok(())
    }
    async fn find(
        &self,
        account_id: AccountId,
        id: AlertRuleId,
    ) -> Result<Option<AlertRule>, AlertRuleServiceError> {
        Ok(self
            .rules
            .lock()
            .map_err(|_| AlertRuleServiceError::Unavailable)?
            .iter()
            .find(|rule| rule.account_id == account_id && rule.id == id)
            .cloned())
    }
    async fn list(
        &self,
        account_id: AccountId,
        system_id: SystemId,
    ) -> Result<Vec<AlertRule>, AlertRuleServiceError> {
        Ok(self
            .rules
            .lock()
            .map_err(|_| AlertRuleServiceError::Unavailable)?
            .iter()
            .filter(|rule| rule.account_id == account_id && rule.system_id == system_id)
            .cloned()
            .collect())
    }
    async fn delete(
        &self,
        account_id: AccountId,
        id: AlertRuleId,
    ) -> Result<bool, AlertRuleServiceError> {
        let mut rules = self
            .rules
            .lock()
            .map_err(|_| AlertRuleServiceError::Unavailable)?;
        let before = rules.len();
        rules.retain(|rule| rule.account_id != account_id || rule.id != id);
        Ok(rules.len() != before)
    }
}
