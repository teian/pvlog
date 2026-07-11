//! Scoped modern management resources backed by account repositories.

use crate::PortError;
use async_trait::async_trait;
use pvlog_domain::{AccountId, ApiScope, SystemId, UserId};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;
use thiserror::Error;
use uuid::Uuid;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModernApiActor {
    pub user_id: UserId,
    pub scopes: BTreeSet<ApiScope>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ManagedResourceKind {
    Equipment,
    Tariff,
    Channel,
    Membership,
    Credential,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagedResource {
    pub id: Uuid,
    pub account_id: AccountId,
    pub system_id: Option<SystemId>,
    pub kind: ManagedResourceKind,
    pub version: u64,
    pub attributes: Value,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CreateManagedResource {
    pub account_id: AccountId,
    pub system_id: Option<SystemId>,
    pub kind: ManagedResourceKind,
    pub attributes: Value,
}

#[async_trait]
pub trait ManagedResourceService: Send + Sync {
    async fn list(
        &self,
        actor: &ModernApiActor,
        account_id: AccountId,
        system_id: Option<SystemId>,
        kind: ManagedResourceKind,
    ) -> Result<Vec<ManagedResource>, ManagedResourceError>;
    async fn create(
        &self,
        actor: &ModernApiActor,
        command: CreateManagedResource,
    ) -> Result<ManagedResource, ManagedResourceError>;
}

#[derive(Debug, Error)]
pub enum ManagedResourceError {
    #[error("required API scope is missing")]
    Forbidden,
    #[error("managed resource input is invalid")]
    InvalidInput,
    #[error("managed resource persistence is unavailable")]
    Repository(PortError),
}
