//! `PVOutput` r2 compatibility HTTP adapter.

#![forbid(unsafe_code)]

mod protocol;

pub use protocol::{
    LegacyAuth, LegacyError, LegacyErrorKind, LegacyMethod, LegacyParameters, LegacyProtocolError,
    LegacySuccess, csv_field, csv_record, format_legacy_date, format_legacy_time,
    parse_legacy_auth, parse_legacy_bool, parse_legacy_date, parse_legacy_time,
};
