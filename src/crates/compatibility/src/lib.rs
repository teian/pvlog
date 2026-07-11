//! `PVOutput` r2 compatibility HTTP adapter.

#![forbid(unsafe_code)]

mod add_output;
mod protocol;

pub use add_output::{
    AddOutputPolicy, AddOutputServiceError, AddOutputUseCases, DailyOutput, add_output_router,
};
pub use protocol::{
    LegacyAuth, LegacyError, LegacyErrorKind, LegacyMethod, LegacyParameters, LegacyProtocolError,
    LegacySuccess, csv_field, csv_record, format_legacy_date, format_legacy_time, parse_csv_record,
    parse_legacy_auth, parse_legacy_bool, parse_legacy_date, parse_legacy_time,
};
