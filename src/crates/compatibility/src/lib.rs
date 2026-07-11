//! `PVOutput` r2 compatibility HTTP adapter.

#![forbid(unsafe_code)]

mod add_batch_status;
mod add_output;
mod add_status;
mod protocol;
mod queries;

pub use add_batch_status::{AddBatchStatusUseCases, BatchStatusOutcome, add_batch_status_router};
pub use add_output::{
    AddOutputPolicy, AddOutputServiceError, AddOutputUseCases, DailyOutput, add_output_router,
};
pub use add_status::{
    AddStatusPolicy, AddStatusServiceError, AddStatusUseCases, LegacyStatus, LegacyStatusEnergy,
    add_status_router,
};
pub use protocol::{
    LegacyAuth, LegacyError, LegacyErrorKind, LegacyMethod, LegacyParameters, LegacyProtocolError,
    LegacySuccess, csv_field, csv_record, format_legacy_date, format_legacy_time, parse_csv_record,
    parse_legacy_auth, parse_legacy_bool, parse_legacy_date, parse_legacy_time,
};
pub use queries::{
    LegacyDayStatistics, LegacyHistoryStatus, LegacyQueryError, LegacyQueryUseCases,
    LegacyRangeStatistics, LegacyStatisticQuery, LegacyStatusQuery, LegacyStatusRecord,
    legacy_query_router,
};
