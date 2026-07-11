//! Dry-run/commit metadata imports and asynchronous checksummed exports.

use crate::{Clock, PortError};
use async_trait::async_trait;
use pvlog_domain::{AccountId, ExportFormat, ExportId, ImportId, SystemId, UserId};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, sync::Arc};
use thiserror::Error;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportValidationIssue {
    pub index: usize,
    pub code: &'static str,
    pub field: &'static str,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportPlan {
    pub id: ImportId,
    pub account_id: AccountId,
    pub content_hash: [u8; 32],
    pub item_count: usize,
    pub issues: Vec<ImportValidationIssue>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportJobResource {
    pub id: ExportId,
    pub account_id: AccountId,
    pub system_id: SystemId,
    pub format: ExportFormat,
    pub content_checksum: Option<[u8; 32]>,
    pub expires_at: i64,
}

#[derive(Deserialize)]
struct ImportDocument {
    systems: Vec<ImportedSystem>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ImportedSystem {
    source_id: String,
    name: String,
}

#[async_trait]
pub trait ImportExportRepository: Send + Sync {
    async fn save_plan(&self, plan: ImportPlan, document: Vec<u8>) -> Result<(), PortError>;
    async fn commit_plan(&self, id: ImportId, account_id: AccountId) -> Result<bool, PortError>;
    async fn enqueue_export(
        &self,
        resource: ExportJobResource,
        requested_by: UserId,
    ) -> Result<(), PortError>;
}

pub struct ImportExportService {
    repository: Arc<dyn ImportExportRepository>,
    clock: Arc<dyn Clock>,
    export_lifetime_seconds: u32,
}
impl ImportExportService {
    #[must_use]
    pub fn new(
        repository: Arc<dyn ImportExportRepository>,
        clock: Arc<dyn Clock>,
        export_lifetime_seconds: u32,
    ) -> Self {
        Self {
            repository,
            clock,
            export_lifetime_seconds,
        }
    }
    /// Validates every imported item and persists an immutable dry-run plan.
    /// # Errors
    /// Returns an error for malformed JSON, time, or persistence failure.
    pub async fn dry_run(
        &self,
        account_id: AccountId,
        document: Vec<u8>,
    ) -> Result<ImportPlan, ImportExportError> {
        let parsed: ImportDocument =
            serde_json::from_slice(&document).map_err(|_| ImportExportError::MalformedDocument)?;
        let mut seen = BTreeSet::new();
        let mut issues = Vec::new();
        for (index, system) in parsed.systems.iter().enumerate() {
            if system.source_id.trim().is_empty() {
                issues.push(ImportValidationIssue {
                    index,
                    code: "required",
                    field: "sourceId",
                });
            } else if !seen.insert(system.source_id.as_str()) {
                issues.push(ImportValidationIssue {
                    index,
                    code: "duplicate",
                    field: "sourceId",
                });
            }
            if system.name.trim().is_empty() {
                issues.push(ImportValidationIssue {
                    index,
                    code: "required",
                    field: "name",
                });
            }
        }
        let plan = ImportPlan {
            id: ImportId::new(),
            account_id,
            content_hash: *blake3::hash(&document).as_bytes(),
            item_count: parsed.systems.len(),
            issues,
        };
        self.repository
            .save_plan(plan.clone(), document)
            .await
            .map_err(ImportExportError::Repository)?;
        Ok(plan)
    }
    /// Commits a previously validated error-free plan exactly once.
    /// # Errors
    /// Returns an error for validation issues, replay/missing plan, or persistence failure.
    pub async fn commit(&self, plan: &ImportPlan) -> Result<(), ImportExportError> {
        if !plan.issues.is_empty() {
            return Err(ImportExportError::ValidationFailed);
        }
        if !self
            .repository
            .commit_plan(plan.id, plan.account_id)
            .await
            .map_err(ImportExportError::Repository)?
        {
            return Err(ImportExportError::PlanUnavailable);
        }
        Ok(())
    }
    /// Enqueues an expiring asynchronous system export job.
    /// # Errors
    /// Returns an error when time or persistence is unavailable.
    pub async fn request_export(
        &self,
        account_id: AccountId,
        system_id: SystemId,
        requested_by: UserId,
        format: ExportFormat,
    ) -> Result<ExportJobResource, ImportExportError> {
        let now = self.now()?;
        let resource = ExportJobResource {
            id: ExportId::new(),
            account_id,
            system_id,
            format,
            content_checksum: None,
            expires_at: now
                .checked_add(i64::from(self.export_lifetime_seconds) * 1_000)
                .ok_or(ImportExportError::Time)?,
        };
        self.repository
            .enqueue_export(resource.clone(), requested_by)
            .await
            .map_err(ImportExportError::Repository)?;
        Ok(resource)
    }
    /// Verifies a completed artifact against its published checksum.
    #[must_use]
    pub fn verify_artifact(resource: &ExportJobResource, bytes: &[u8]) -> bool {
        resource
            .content_checksum
            .is_some_and(|checksum| checksum == *blake3::hash(bytes).as_bytes())
    }
    fn now(&self) -> Result<i64, ImportExportError> {
        i64::try_from(self.clock.now().epoch_millis()).map_err(|_| ImportExportError::Time)
    }
}

#[derive(Debug, Error)]
pub enum ImportExportError {
    #[error("import document is malformed")]
    MalformedDocument,
    #[error("import validation failed")]
    ValidationFailed,
    #[error("import plan is unavailable or already committed")]
    PlanUnavailable,
    #[error("clock value is invalid")]
    Time,
    #[error("import/export persistence is unavailable")]
    Repository(PortError),
}
