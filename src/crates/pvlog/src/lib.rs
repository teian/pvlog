//! Runtime composition for the `PVLog` application.

#![forbid(unsafe_code)]

pub mod administration;
pub mod aggregation;
pub mod analytics;
pub mod analytics_series;
pub mod api_keys;
pub mod authentication;
pub mod channels;
pub mod config;
pub mod dashboard;
pub mod embedded_ui;
pub mod geocoding;
pub mod insights;
pub mod inverters;
pub mod notifications;
pub mod operator_bundle;
pub mod reporting;
pub mod sharing;
pub mod tariffs;
pub mod telemetry;

use pvlog_application::Clock;
use pvlog_domain::UtcTimestamp;

/// Wall clock used by production application services.
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> UtcTimestamp {
        UtcTimestamp::new(time::OffsetDateTime::now_utc())
    }
}
