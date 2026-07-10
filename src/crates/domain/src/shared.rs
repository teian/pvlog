use std::{fmt, str::FromStr};

use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

use crate::ValidationError;

/// Visibility policy for a system or shareable resource.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Visibility {
    /// Visible only to explicitly authorized principals.
    #[default]
    Private,
    /// Visible to members of the owning account.
    Account,
    /// Accessible by direct link but excluded from discovery.
    Unlisted,
    /// Eligible for public reads and discovery projections.
    Public,
}

/// Validated uppercase three-letter ISO 4217-style currency code.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CurrencyCode(String);

impl CurrencyCode {
    /// Returns the normalized three-letter code.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for CurrencyCode {
    type Err = ValidationError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.len() == 3 && value.bytes().all(|byte| byte.is_ascii_uppercase()) {
            Ok(Self(value.to_owned()))
        } else {
            Err(ValidationError::new(
                "invalid_currency_code",
                "currency",
                "currency must contain exactly three uppercase ASCII letters",
            ))
        }
    }
}

impl fmt::Display for CurrencyCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl Serialize for CurrencyCode {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for CurrencyCode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::from_str(&value).map_err(de::Error::custom)
    }
}

/// Monetary amount stored exactly in the currency's minor unit.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Money {
    /// Signed amount in minor units, such as euro cents.
    pub minor_units: i64,
    /// Currency that determines the minor-unit scale at presentation boundaries.
    pub currency: CurrencyCode,
}

impl Money {
    /// Creates an exact monetary amount without floating-point conversion.
    #[must_use]
    pub const fn new(minor_units: i64, currency: CurrencyCode) -> Self {
        Self {
            minor_units,
            currency,
        }
    }
}
