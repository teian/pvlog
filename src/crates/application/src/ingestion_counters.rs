//! Interval energy derived from lifetime counters.
//!
//! A source may report how much energy it has produced or consumed in total
//! instead of, or in addition to, the energy of the last interval. The interval
//! is then the difference to the counter the same source reported before.

use pvlog_domain::{CanonicalObservation, EnergyReading, QualityFlags, WattHours};

/// Lifetime counters the same source reported most recently before a reading.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CounterBaseline {
    pub generation_lifetime_wh: Option<i64>,
    pub consumption_lifetime_wh: Option<i64>,
}

/// What a lifetime counter says about the interval that ended with it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CounterStep {
    /// The counter rose by this many watt-hours.
    Increase(i64),
    /// The counter is lower than before: the meter was replaced or reset.
    Reset,
    /// There is no earlier counter to compare with.
    NoPredecessor,
}

/// Compares a counter with its predecessor.
#[must_use]
pub const fn counter_step(previous: Option<i64>, current: i64) -> CounterStep {
    match previous {
        None => CounterStep::NoPredecessor,
        Some(previous) if current < previous => CounterStep::Reset,
        Some(previous) => CounterStep::Increase(current - previous),
    }
}

/// Fills in interval energy from the lifetime counters of a reading.
///
/// Supplied interval energy is never replaced. Nothing is derived across a
/// reset or without a predecessor, because the energy of that interval is
/// unknown rather than zero; a reset is flagged on the reading.
pub fn derive_counter_energy(observation: &mut CanonicalObservation, baseline: CounterBaseline) {
    let mut flags = observation.quality;
    for (lifetime, previous, interval) in [
        (
            observation.values.generation_lifetime,
            baseline.generation_lifetime_wh,
            &mut observation.values.generation_energy,
        ),
        (
            observation.values.consumption_lifetime,
            baseline.consumption_lifetime_wh,
            &mut observation.values.consumption_energy,
        ),
    ] {
        let Some(lifetime) = lifetime else { continue };
        match counter_step(previous, lifetime.value()) {
            CounterStep::Increase(delta) if interval.is_none() => {
                *interval = Some(EnergyReading::Interval(WattHours::new(delta)));
                flags = flags | QualityFlags::DERIVED;
            }
            CounterStep::Reset => flags = flags | QualityFlags::COUNTER_RESET,
            CounterStep::Increase(_) | CounterStep::NoPredecessor => {}
        }
    }
    observation.quality = flags;
}
