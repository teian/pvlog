//! Runtime composition for the `PVLog` application.

#![forbid(unsafe_code)]

pub mod authentication;
pub mod config;
pub mod inverters;
pub mod operator_bundle;

use pvlog_application::Clock;
use pvlog_domain::UtcTimestamp;

/// Wall clock used by production application services.
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> UtcTimestamp {
        UtcTimestamp::new(time::OffsetDateTime::now_utc())
    }
}
