//! Validated effective-dated system configuration use cases.

use crate::PortError;
use async_trait::async_trait;
use pvlog_domain::{
    CalculationSettings, CapacityPeriod, ChannelDefinition, Equipment, Inverter, SystemId,
    SystemPrivacy, Tariff, UserId,
};
use std::sync::Arc;
use thiserror::Error;

#[async_trait]
pub trait SystemConfigurationRepository: Send + Sync {
    async fn capacity_overlaps(
        &self,
        system_id: SystemId,
        period: CapacityPeriod,
    ) -> Result<bool, PortError>;
    async fn save_capacity(
        &self,
        system_id: SystemId,
        period: CapacityPeriod,
    ) -> Result<(), PortError>;
    async fn save_equipment(&self, equipment: Equipment) -> Result<(), PortError>;
    async fn save_inverter(&self, inverter: Inverter) -> Result<(), PortError>;
    async fn save_tariff(&self, tariff: Tariff) -> Result<(), PortError>;
    async fn save_channel(&self, channel: ChannelDefinition) -> Result<(), PortError>;
    async fn save_settings(
        &self,
        system_id: SystemId,
        privacy: SystemPrivacy,
        calculation: CalculationSettings,
    ) -> Result<(), PortError>;
    async fn audit(
        &self,
        actor: UserId,
        system_id: SystemId,
        action: &'static str,
    ) -> Result<(), PortError>;
}

pub struct SystemConfigurationService {
    repository: Arc<dyn SystemConfigurationRepository>,
}
impl SystemConfigurationService {
    #[must_use]
    pub fn new(repository: Arc<dyn SystemConfigurationRepository>) -> Self {
        Self { repository }
    }
    /// Adds non-overlapping effective capacity.
    /// # Errors
    /// Returns an error for overlap or persistence failure.
    pub async fn add_capacity(
        &self,
        actor: UserId,
        system_id: SystemId,
        period: CapacityPeriod,
    ) -> Result<(), SystemConfigurationError> {
        if self
            .repository
            .capacity_overlaps(system_id, period)
            .await
            .map_err(SystemConfigurationError::Repository)?
        {
            return Err(SystemConfigurationError::OverlappingEffectivePeriod);
        }
        self.repository
            .save_capacity(system_id, period)
            .await
            .map_err(SystemConfigurationError::Repository)?;
        self.audit(actor, system_id, "system.capacity.updated")
            .await
    }
    /// Adds effective-dated equipment.
    /// # Errors
    /// Returns an error for system mismatch or persistence failure.
    pub async fn add_equipment(
        &self,
        actor: UserId,
        system_id: SystemId,
        equipment: Equipment,
    ) -> Result<(), SystemConfigurationError> {
        if equipment.system_id != system_id {
            return Err(SystemConfigurationError::InvalidConfiguration);
        }
        self.repository
            .save_equipment(equipment)
            .await
            .map_err(SystemConfigurationError::Repository)?;
        self.audit(actor, system_id, "system.equipment.updated")
            .await
    }
    /// Adds or replaces one inverter and its complete PV-string subtree.
    /// # Errors
    /// Returns an error for an empty or cross-parent aggregate, invalid physical values, or a
    /// persistence failure.
    pub async fn save_inverter(
        &self,
        actor: UserId,
        system_id: SystemId,
        inverter: Inverter,
    ) -> Result<(), SystemConfigurationError> {
        if inverter.system_id != system_id
            || inverter.name.trim().is_empty()
            || inverter.strings.is_empty()
            || inverter.strings.iter().any(|string| {
                string.inverter_id != inverter.id
                    || string.name.trim().is_empty()
                    || string.panel_count == 0
                    || string.rated_power.value() <= 0
                    || string.orientation_degrees.is_some_and(|value| value > 359)
                    || string.tilt_degrees.is_some_and(|value| value > 90)
            })
        {
            return Err(SystemConfigurationError::InvalidConfiguration);
        }
        self.repository
            .save_inverter(inverter)
            .await
            .map_err(SystemConfigurationError::Repository)?;
        self.audit(actor, system_id, "system.inverter.updated")
            .await
    }
    /// Adds an effective-dated tariff.
    /// # Errors
    /// Returns an error for system mismatch or persistence failure.
    pub async fn add_tariff(
        &self,
        actor: UserId,
        system_id: SystemId,
        tariff: Tariff,
    ) -> Result<(), SystemConfigurationError> {
        if tariff.system_id != system_id {
            return Err(SystemConfigurationError::InvalidConfiguration);
        }
        self.repository
            .save_tariff(tariff)
            .await
            .map_err(SystemConfigurationError::Repository)?;
        self.audit(actor, system_id, "system.tariff.updated").await
    }
    /// Adds or updates a typed extended channel.
    /// # Errors
    /// Returns an error for invalid bounds/identity or persistence failure.
    pub async fn save_channel(
        &self,
        actor: UserId,
        system_id: SystemId,
        channel: ChannelDefinition,
    ) -> Result<(), SystemConfigurationError> {
        if channel.system_id != system_id
            || channel.stable_key.trim().is_empty()
            || channel.name.trim().is_empty()
            || channel
                .minimum_scaled
                .zip(channel.maximum_scaled)
                .is_some_and(|(min, max)| min > max)
        {
            return Err(SystemConfigurationError::InvalidConfiguration);
        }
        self.repository
            .save_channel(channel)
            .await
            .map_err(SystemConfigurationError::Repository)?;
        self.audit(actor, system_id, "system.channel.updated").await
    }
    /// Updates privacy and deterministic calculation modes together.
    /// # Errors
    /// Returns an error when persistence or auditing fails.
    pub async fn save_settings(
        &self,
        actor: UserId,
        system_id: SystemId,
        privacy: SystemPrivacy,
        calculation: CalculationSettings,
    ) -> Result<(), SystemConfigurationError> {
        self.repository
            .save_settings(system_id, privacy, calculation)
            .await
            .map_err(SystemConfigurationError::Repository)?;
        self.audit(actor, system_id, "system.settings.updated")
            .await
    }
    async fn audit(
        &self,
        actor: UserId,
        system_id: SystemId,
        action: &'static str,
    ) -> Result<(), SystemConfigurationError> {
        self.repository
            .audit(actor, system_id, action)
            .await
            .map_err(SystemConfigurationError::Repository)
    }
}

#[derive(Debug, Error)]
pub enum SystemConfigurationError {
    #[error("effective configuration overlaps an existing period")]
    OverlappingEffectivePeriod,
    #[error("system configuration is invalid")]
    InvalidConfiguration,
    #[error("system configuration persistence is unavailable")]
    Repository(PortError),
}
