//! Modern analytics API boundary shared by HTTP adapters and implementations.

use crate::{
    DataQualityIssue, EnergyStatistics, QueryPlanRequest, SeriesQueryResult, StatisticsPeriod,
};
use async_trait::async_trait;
use pvlog_domain::{JobId, SystemId, UserId};
use thiserror::Error;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AnalysisExportFormat {
    Csv,
    Json,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AnalysisExportRequest {
    pub system_id: SystemId,
    pub actor: UserId,
    pub query: QueryPlanRequest,
    pub format: AnalysisExportFormat,
    pub asynchronous: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AnalysisExportResult {
    Ready {
        content_type: String,
        filename: String,
        bytes: Vec<u8>,
    },
    Queued {
        job_id: JobId,
    },
}

#[async_trait]
pub trait ModernAnalyticsUseCases: Send + Sync {
    async fn time_series(
        &self,
        actor: UserId,
        system_id: SystemId,
        request: QueryPlanRequest,
    ) -> Result<SeriesQueryResult, ModernAnalyticsError>;
    async fn statistics(
        &self,
        actor: UserId,
        system_id: SystemId,
        period: StatisticsPeriod,
    ) -> Result<EnergyStatistics, ModernAnalyticsError>;
    async fn data_quality(
        &self,
        actor: UserId,
        system_id: SystemId,
        start_epoch_millis: i64,
        end_epoch_millis: i64,
    ) -> Result<Vec<DataQualityIssue>, ModernAnalyticsError>;
    async fn export(
        &self,
        request: AnalysisExportRequest,
    ) -> Result<AnalysisExportResult, ModernAnalyticsError>;
}

#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum ModernAnalyticsError {
    #[error("analytics request is invalid")]
    Invalid,
    #[error("analytics resource was not found")]
    NotFound,
    #[error("analytics access is forbidden")]
    Forbidden,
    #[error("analytics result is too large for synchronous processing")]
    RequiresAsync,
    #[error("analytics service is unavailable")]
    Unavailable,
}
