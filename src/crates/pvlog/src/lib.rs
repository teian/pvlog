//! Runtime composition for the `PVLog` application.

#![forbid(unsafe_code)]

pub mod administration;
pub mod aggregation;
pub mod analytics;
pub mod analytics_series;
pub mod api_keys;
pub mod authentication;
pub mod channel_aggregation;
pub mod channels;
pub mod config;
pub mod daily_summaries;
pub mod dashboard;
pub mod data_sources;
pub mod device_events;
pub mod discovery;
pub mod embedded_ui;
pub mod forecasting;
pub mod geocoding;
pub mod history_imports;
pub mod ingest;
pub mod insights;
pub mod inverters;
pub mod notifications;
pub mod operator_bundle;
pub mod reporting;
pub mod setup;
pub mod sharing;
pub mod tariffs;
pub mod telemetry;
pub mod weather;
pub mod yield_cycle;
pub mod yield_model;

use pvlog_application::Clock;
use pvlog_domain::UtcTimestamp;

/// Wall clock used by production application services.
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> UtcTimestamp {
        UtcTimestamp::new(time::OffsetDateTime::now_utc())
    }
}
