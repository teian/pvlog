//! Runtime composition for the `PVLog` application.

#![forbid(unsafe_code)]

pub mod administration;
pub mod api_keys;
pub mod authentication;
pub mod config;
pub mod embedded_ui;
pub mod geocoding;
pub mod inverters;
pub mod notifications;
pub mod operator_bundle;
pub mod reporting;

use pvlog_application::Clock;
use pvlog_domain::UtcTimestamp;

/// Wall clock used by production application services.
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> UtcTimestamp {
        UtcTimestamp::new(time::OffsetDateTime::now_utc())
    }
}
