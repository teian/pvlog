use std::{fmt, str::FromStr};

use chrono_tz::Tz;
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use time::{OffsetDateTime, UtcOffset};

use crate::ValidationError;

/// Instant normalized to UTC and suitable for canonical storage.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct UtcTimestamp(OffsetDateTime);

impl UtcTimestamp {
    /// Normalizes an absolute timestamp to the UTC offset.
    #[must_use]
    pub fn new(value: OffsetDateTime) -> Self {
        Self(value.to_offset(UtcOffset::UTC))
    }

    /// Constructs a UTC timestamp from canonical Unix epoch milliseconds.
    ///
    /// # Errors
    ///
    /// Returns an error when the timestamp is outside the supported calendar range.
    pub fn from_epoch_millis(value: i64) -> Result<Self, ValidationError> {
        OffsetDateTime::from_unix_timestamp_nanos(i128::from(value) * 1_000_000)
            .map(Self::new)
            .map_err(|_| {
                ValidationError::new(
                    "timestamp_out_of_range",
                    "timestamp",
                    "timestamp is outside the supported range",
                )
            })
    }

    /// Returns canonical Unix epoch milliseconds.
    #[must_use]
    pub fn epoch_millis(self) -> i128 {
        self.0.unix_timestamp_nanos() / 1_000_000
    }

    /// Returns the normalized date-time value.
    #[must_use]
    pub const fn as_datetime(self) -> OffsetDateTime {
        self.0
    }
}

/// Validated, case-sensitive name from the embedded IANA timezone database.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct IanaTimezone(String);

impl IanaTimezone {
    /// Returns the canonical IANA name supplied at the boundary.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for IanaTimezone {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl FromStr for IanaTimezone {
    type Err = ValidationError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        value.parse::<Tz>().map_err(|_| {
            ValidationError::new(
                "invalid_iana_timezone",
                "timezone",
                "timezone must be a case-sensitive IANA timezone name",
            )
        })?;
        Ok(Self(value.to_owned()))
    }
}

impl Serialize for IanaTimezone {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for IanaTimezone {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::from_str(&value).map_err(de::Error::custom)
    }
}
