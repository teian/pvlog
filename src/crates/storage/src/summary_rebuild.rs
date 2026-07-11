//! Idempotent daily and lifetime summary rebuild planning.

use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct SummaryDay(pub i32);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DailyAggregate {
    pub system_id: Uuid,
    pub day: SummaryDay,
    pub generation_wh: i64,
    pub consumption_wh: i64,
    pub quality_flags: u32,
    pub source_revision: u64,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SummaryProjection {
    pub daily: BTreeMap<(Uuid, SummaryDay), DailyAggregate>,
    pub lifetime: BTreeMap<Uuid, LifetimeAggregate>,
    invalidated: BTreeSet<(Uuid, SummaryDay)>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LifetimeAggregate {
    pub generation_wh: i128,
    pub consumption_wh: i128,
    pub quality_flags: u32,
    pub through_day: Option<SummaryDay>,
}

impl SummaryProjection {
    /// Invalidates a changed day and its dependent lifetime summary.
    pub fn invalidate(&mut self, system_id: Uuid, day: SummaryDay) {
        self.invalidated.insert((system_id, day));
        self.lifetime.remove(&system_id);
    }

    /// Reconciles one authoritative daily aggregate and rebuilds its lifetime dependency.
    /// Replaying the same source revision and values is a no-op.
    pub fn reconcile(&mut self, aggregate: &DailyAggregate) -> bool {
        let key = (aggregate.system_id, aggregate.day);
        let changed = self.daily.get(&key) != Some(aggregate);
        self.daily.insert(key, aggregate.clone());
        self.invalidated.remove(&key);
        self.rebuild_lifetime(aggregate.system_id);
        changed
    }

    pub fn invalidated_days(&self) -> impl Iterator<Item = (Uuid, SummaryDay)> + '_ {
        self.invalidated.iter().copied()
    }

    fn rebuild_lifetime(&mut self, system_id: Uuid) {
        let mut lifetime = LifetimeAggregate::default();
        for aggregate in self
            .daily
            .values()
            .filter(|aggregate| aggregate.system_id == system_id)
        {
            lifetime.generation_wh += i128::from(aggregate.generation_wh);
            lifetime.consumption_wh += i128::from(aggregate.consumption_wh);
            lifetime.quality_flags |= aggregate.quality_flags;
            lifetime.through_day = Some(
                lifetime
                    .through_day
                    .map_or(aggregate.day, |day| day.max(aggregate.day)),
            );
        }
        self.lifetime.insert(system_id, lifetime);
    }
}
