use async_trait::async_trait;
use pvlog_application::{
    PortError, SystemConfigurationError, SystemConfigurationRepository, SystemConfigurationService,
};
use pvlog_domain::{
    CalculationSettings, CapacityPeriod, ChannelDataType, ChannelDefinition, ChannelDisplay,
    ChannelId, ChannelLifecycle, ChannelScale, Equipment, SystemId, SystemPrivacy, Tariff, UserId,
};
use std::{error::Error, sync::Arc};

#[tokio::test]
async fn configuration_rejects_overlapping_periods_and_invalid_channel_bounds()
-> Result<(), Box<dyn Error>> {
    let service = SystemConfigurationService::new(Arc::new(FakeRepository));
    let system = SystemId::new();
    let period = CapacityPeriod {
        period: pvlog_domain::EffectivePeriod::new(
            time::Date::from_calendar_date(2026, time::Month::January, 1)?,
            None,
        )?,
        capacity: pvlog_domain::Watts::new(1_000),
    };
    assert!(matches!(
        service.add_capacity(UserId::new(), system, period).await,
        Err(SystemConfigurationError::OverlappingEffectivePeriod)
    ));
    let channel = ChannelDefinition {
        id: ChannelId::new(),
        system_id: system,
        stable_key: "temperature".to_owned(),
        name: "Temperature".to_owned(),
        data_type: ChannelDataType::Decimal,
        unit: "degC".to_owned(),
        scale: ChannelScale::new(-3)?,
        minimum_scaled: Some(50),
        maximum_scaled: Some(10),
        display: ChannelDisplay {
            color_token: None,
            chart_by_default: true,
            decimal_places: 1,
        },
        lifecycle: ChannelLifecycle::Active,
    };
    assert!(matches!(
        service.save_channel(UserId::new(), system, channel).await,
        Err(SystemConfigurationError::InvalidConfiguration)
    ));
    Ok(())
}

struct FakeRepository;
#[async_trait]
impl SystemConfigurationRepository for FakeRepository {
    async fn capacity_overlaps(
        &self,
        _system: SystemId,
        _period: CapacityPeriod,
    ) -> Result<bool, PortError> {
        Ok(true)
    }
    async fn save_capacity(
        &self,
        _system: SystemId,
        _period: CapacityPeriod,
    ) -> Result<(), PortError> {
        Ok(())
    }
    async fn save_equipment(&self, _equipment: Equipment) -> Result<(), PortError> {
        Ok(())
    }
    async fn save_tariff(&self, _tariff: Tariff) -> Result<(), PortError> {
        Ok(())
    }
    async fn save_channel(&self, _channel: ChannelDefinition) -> Result<(), PortError> {
        Ok(())
    }
    async fn save_settings(
        &self,
        _system: SystemId,
        _privacy: SystemPrivacy,
        _calculation: CalculationSettings,
    ) -> Result<(), PortError> {
        Ok(())
    }
    async fn audit(
        &self,
        _actor: UserId,
        _system: SystemId,
        _action: &'static str,
    ) -> Result<(), PortError> {
        Ok(())
    }
}
