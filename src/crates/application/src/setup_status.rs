//! What a system still needs before it is fully set up.
//!
//! The status is derived from stored configuration and telemetry, never
//! tracked separately, so it cannot drift from what the system really has. A
//! system that receives data is never presented as unfinished: only connecting
//! a data source and receiving the first reading are ever recommended; the rest
//! is optional detail.

use serde::Serialize;

/// Facts about a system from which its setup status follows.
// Each flag is an independent fact the status is derived from, not a state.
#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SetupFacts {
    pub has_location: bool,
    pub has_declared_capacity: bool,
    /// Strings of the system, whatever their inverter.
    pub strings: u64,
    /// Data sources that were not removed.
    pub data_sources: u64,
    /// Whether any reading that is not a test reading is stored.
    pub has_reading: bool,
    /// Channels that received a value and are bound to nothing.
    pub unassigned_channels: u64,
    /// Whether an import or export tariff is effective now.
    pub has_effective_tariff: bool,
    /// Whether every effective string is ready for a yield forecast; `None`
    /// when readiness cannot be determined.
    pub forecast_ready: Option<bool>,
}

/// One step of setting a system up.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SetupItemKey {
    Location,
    Capacity,
    DataSource,
    FirstData,
    Channels,
    Equipment,
    Tariff,
    Forecast,
}

/// How far one step has come.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SetupItemState {
    Done,
    /// The step to take next.
    Recommended,
    /// Improves the system but is not needed for it to work.
    Optional,
    /// Cannot be completed before another step.
    Blocked,
}

/// One step with its state and, when it is not done, why.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupItem {
    pub key: SetupItemKey,
    pub state: SetupItemState,
    /// Machine-readable reason the step is not done.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<&'static str>,
    /// How many things the reason refers to, where that is meaningful.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<u64>,
}

impl SetupItem {
    const fn done(key: SetupItemKey) -> Self {
        Self {
            key,
            state: SetupItemState::Done,
            reason: None,
            count: None,
        }
    }

    const fn open(key: SetupItemKey, state: SetupItemState, reason: &'static str) -> Self {
        Self {
            key,
            state,
            reason: Some(reason),
            count: None,
        }
    }

    /// Done when `complete`, otherwise optional for the given reason.
    const fn optional_unless(key: SetupItemKey, complete: bool, reason: &'static str) -> Self {
        if complete {
            Self::done(key)
        } else {
            Self::open(key, SetupItemState::Optional, reason)
        }
    }
}

/// Derives the ordered setup status of a system from its facts.
#[must_use]
pub fn setup_status(facts: &SetupFacts) -> Vec<SetupItem> {
    use SetupItemKey::{
        Capacity, Channels, DataSource, Equipment, FirstData, Forecast, Location, Tariff,
    };
    let data_source = if facts.data_sources > 0 || facts.has_reading {
        // Readings that arrived without a data source prove a working uploader.
        SetupItem::done(DataSource)
    } else {
        SetupItem::open(
            DataSource,
            SetupItemState::Recommended,
            "data_source_missing",
        )
    };
    let first_data = if facts.has_reading {
        SetupItem::done(FirstData)
    } else if facts.data_sources == 0 {
        SetupItem::open(FirstData, SetupItemState::Blocked, "data_source_missing")
    } else {
        SetupItem::open(
            FirstData,
            SetupItemState::Recommended,
            "waiting_for_first_reading",
        )
    };
    let channels = if facts.unassigned_channels == 0 {
        SetupItem::done(Channels)
    } else {
        SetupItem {
            count: Some(facts.unassigned_channels),
            ..SetupItem::open(Channels, SetupItemState::Optional, "unassigned_channels")
        }
    };
    let forecast = match facts.forecast_ready {
        Some(true) => SetupItem::done(Forecast),
        Some(false) => SetupItem::open(
            Forecast,
            SetupItemState::Optional,
            "forecast_inputs_missing",
        ),
        None => SetupItem::open(
            Forecast,
            SetupItemState::Optional,
            "forecast_readiness_unknown",
        ),
    };
    vec![
        SetupItem::optional_unless(Location, facts.has_location, "location_missing"),
        SetupItem::optional_unless(
            Capacity,
            facts.has_declared_capacity || facts.strings > 0,
            "capacity_missing",
        ),
        data_source,
        first_data,
        channels,
        SetupItem::optional_unless(Equipment, facts.strings > 0, "equipment_missing"),
        SetupItem::optional_unless(Tariff, facts.has_effective_tariff, "tariff_missing"),
        forecast,
    ]
}
