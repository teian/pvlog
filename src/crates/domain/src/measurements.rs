use std::ops::BitOr;

use serde::{Deserialize, Deserializer, Serialize, de};

use crate::ValidationError;

macro_rules! integer_unit {
    ($name:ident, $storage:ty, $description:literal) => {
        #[doc = $description]
        #[derive(
            Clone, Copy, Debug, Default, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize,
        )]
        #[serde(transparent)]
        pub struct $name(pub $storage);

        impl $name {
            /// Creates the explicitly unit-typed value.
            #[must_use]
            pub const fn new(value: $storage) -> Self {
                Self(value)
            }

            /// Returns the integer base-unit value.
            #[must_use]
            pub const fn value(self) -> $storage {
                self.0
            }
        }
    };
}

integer_unit!(Watts, i64, "Power in watts; signed for directional flows.");
integer_unit!(
    WattHours,
    i64,
    "Energy in watt-hours; signed for interval deltas and directional flows."
);
integer_unit!(MilliVolts, u32, "Electrical potential in millivolts.");
integer_unit!(
    MilliDegreesCelsius,
    i32,
    "Temperature in thousandths of a degree Celsius."
);

/// Ratio in basis points, bounded to the signed -100% through +100% interval.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct BasisPoints(i32);

impl BasisPoints {
    /// Creates a validated signed ratio.
    ///
    /// # Errors
    ///
    /// Returns an error outside -10,000 through 10,000 basis points.
    pub fn new(value: i32) -> Result<Self, ValidationError> {
        if (-10_000..=10_000).contains(&value) {
            Ok(Self(value))
        } else {
            Err(ValidationError::new(
                "basis_points_out_of_range",
                "basis_points",
                "basis points must be between -10000 and 10000",
            ))
        }
    }

    /// Returns the integer number of basis points.
    #[must_use]
    pub const fn value(self) -> i32 {
        self.0
    }
}

impl<'de> Deserialize<'de> for BasisPoints {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::new(i32::deserialize(deserializer)?).map_err(de::Error::custom)
    }
}

/// Validated bit-set describing observation quality and processing provenance.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct QualityFlags(u16);

impl QualityFlags {
    /// No exceptional quality condition is known.
    pub const NONE: Self = Self(0);
    /// Value was estimated rather than directly measured.
    pub const ESTIMATED: Self = Self(1 << 0);
    /// Value was deterministically derived from other measurements.
    pub const DERIVED: Self = Self(1 << 1);
    /// A later correction supersedes an earlier accepted value.
    pub const CORRECTED: Self = Self(1 << 2);
    /// Value passed ingestion but is marked suspect for downstream display.
    pub const SUSPECT: Self = Self(1 << 3);
    /// Source reported a gap or unavailable value.
    pub const MISSING: Self = Self(1 << 4);
    const KNOWN_BITS: u16 =
        Self::ESTIMATED.0 | Self::DERIVED.0 | Self::CORRECTED.0 | Self::SUSPECT.0 | Self::MISSING.0;

    /// Validates persisted or wire-format bits.
    ///
    /// # Errors
    ///
    /// Returns an error when a bit is not defined by this release.
    pub fn from_bits(bits: u16) -> Result<Self, ValidationError> {
        if bits & !Self::KNOWN_BITS == 0 {
            Ok(Self(bits))
        } else {
            Err(ValidationError::new(
                "unknown_quality_flag",
                "quality_flags",
                "quality flags contain an unknown bit",
            ))
        }
    }

    /// Returns the stable storage representation.
    #[must_use]
    pub const fn bits(self) -> u16 {
        self.0
    }

    /// Reports whether every bit in `other` is present.
    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}

impl BitOr for QualityFlags {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

impl<'de> Deserialize<'de> for QualityFlags {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::from_bits(u16::deserialize(deserializer)?).map_err(de::Error::custom)
    }
}
