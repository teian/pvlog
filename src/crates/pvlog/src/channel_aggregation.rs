//! Aggregates of bound measurement channels.
//!
//! A channel that is bound to the system, an inverter or a string is reduced
//! to one row per local-calendar bucket. Quarter-hour, hour and day buckets
//! are built from the stored values of the days that received something new;
//! month and year buckets are merged from the day buckets, so a year of
//! history is never read again because one reading arrived.
//!
//! Which values are new is tracked per channel, and a changed binding resets
//! that mark, so rebinding rebuilds a channel's aggregates under its new
//! target on the next cycle.

use std::collections::BTreeMap;

use chrono_tz::Tz;
use pvlog_domain::{ChannelId, SystemId};
use pvlog_storage::{
    ChannelRecord, ChannelRepository, ChannelRollupRecord, ChannelValueRecord, DatabaseTarget,
    ManagementRepository,
};

use crate::aggregation::{
    AggregationError, Resolution, local_time, next_year_start, window, year_start,
};

/// Resolutions built from stored values.
const FINE: [Resolution; 3] = [
    Resolution::FifteenMinutes,
    Resolution::Hour,
    Resolution::Day,
];
/// Resolutions merged from day buckets.
const COARSE: [Resolution; 2] = [Resolution::Month, Resolution::Year];

/// Rebuilds the stale channel aggregates of every registered system once and
/// returns how many aggregate rows were written.
///
/// # Errors
///
/// Returns an error when routing, account storage or a system's timezone is
/// unavailable.
pub async fn run_cycle(
    target: &DatabaseTarget,
    management: &dyn ManagementRepository,
) -> Result<u64, AggregationError> {
    let mut written = 0;
    for system in management
        .registered_systems()
        .await
        .map_err(|_| AggregationError::Unavailable)?
    {
        let Some(repository) =
            crate::channels::channel_repository(target, management, system.system_id).await
        else {
            // An account whose storage is not provisioned yet has no channels.
            continue;
        };
        written += rebuild_channel_rollups(
            repository.as_ref(),
            system.system_id,
            chrono::Utc::now().timestamp_millis(),
        )
        .await?;
    }
    Ok(written)
}

/// Brings the aggregates of one system's bound channels up to date and returns
/// how many aggregate rows were written.
///
/// # Errors
///
/// Returns an error when account storage or the system's timezone is unavailable.
pub async fn rebuild_channel_rollups(
    repository: &dyn ChannelRepository,
    system_id: SystemId,
    calculated_at: i64,
) -> Result<u64, AggregationError> {
    let unavailable = |_| AggregationError::Unavailable;
    let pending = repository
        .pending_channel_rollups(system_id)
        .await
        .map_err(unavailable)?;
    if pending.is_empty() {
        return Ok(0);
    }
    let zone: Tz = repository
        .system_timezone(system_id)
        .await
        .map_err(unavailable)?
        .and_then(|timezone| timezone.parse().ok())
        .ok_or(AggregationError::Timezone)?;
    let mut written = 0_u64;
    for pending in pending {
        let channel = &pending.channel;
        // Whole local days: every finer bucket lies within one of them.
        let (_, days_from, _) = window(
            zone,
            Resolution::Day,
            local_time(zone, pending.first_measured_at)?,
        )?;
        let (_, _, days_to) = window(
            zone,
            Resolution::Day,
            local_time(zone, pending.last_measured_at)?,
        )?;
        let values = repository
            .channel_values(channel.id, days_from, days_to)
            .await
            .map_err(unavailable)?;
        let counter = channel.aggregation == "counter_delta";
        let predecessor = if counter {
            repository
                .channel_value_before(channel.id, days_from)
                .await
                .map_err(unavailable)?
                .map(|value| value.value)
        } else {
            None
        };
        let mut fine = Vec::new();
        for resolution in FINE {
            fine.extend(bucket_values(
                zone,
                resolution,
                channel.id,
                &values,
                counter,
                predecessor,
            )?);
        }
        replace(
            repository,
            channel,
            FINE,
            days_from,
            days_to,
            &fine,
            calculated_at,
        )
        .await?;

        // Whole local years: the months and the year the touched days belong to.
        let years_from = year_start(zone, pending.first_measured_at)?;
        let years_to = next_year_start(zone, pending.last_measured_at)?;
        let days = repository
            .channel_rollup_buckets(channel.id, Resolution::Day.name(), years_from, years_to)
            .await
            .map_err(unavailable)?;
        let mut coarse = Vec::new();
        for resolution in COARSE {
            coarse.extend(merge_days(zone, resolution, &days)?);
        }
        replace(
            repository,
            channel,
            COARSE,
            years_from,
            years_to,
            &coarse,
            calculated_at,
        )
        .await?;
        repository
            .mark_channel_rollups_built(
                channel.id,
                pending.last_received_at,
                pending.last_received_values,
            )
            .await
            .map_err(unavailable)?;
        written += u64::try_from(fine.len() + coarse.len()).unwrap_or(u64::MAX);
    }
    Ok(written)
}

/// Stores the aggregates of some resolutions in place of the ones in the range.
async fn replace<const N: usize>(
    repository: &dyn ChannelRepository,
    channel: &ChannelRecord,
    resolutions: [Resolution; N],
    from: i64,
    to: i64,
    rollups: &[ChannelRollupRecord],
    calculated_at: i64,
) -> Result<(), AggregationError> {
    repository
        .replace_channel_rollups(
            channel,
            &resolutions.map(Resolution::name),
            from,
            to,
            rollups,
            calculated_at,
        )
        .await
        .map_err(|_| AggregationError::Unavailable)
}

/// Reduces values, oldest first, to one aggregate per local bucket.
///
/// For a counter channel, `predecessor` is the value measured before the
/// first one, when there is one. A counter's growth is the sum of the
/// increases from each value to the next; a decrease is a reset and adds
/// nothing.
fn bucket_values(
    zone: Tz,
    resolution: Resolution,
    channel_id: ChannelId,
    values: &[ChannelValueRecord],
    counter: bool,
    predecessor: Option<i64>,
) -> Result<Vec<ChannelRollupRecord>, AggregationError> {
    let mut buckets: BTreeMap<i64, ChannelRollupRecord> = BTreeMap::new();
    let mut previous = predecessor;
    for value in values {
        let (_, start, end) = window(zone, resolution, local_time(zone, value.measured_at)?)?;
        let growth = previous.map(|previous| (value.value - previous).max(0));
        let bucket = buckets.entry(start).or_insert(ChannelRollupRecord {
            channel_id,
            resolution: resolution.name().to_owned(),
            bucket_start: start,
            bucket_end: end,
            point_count: 0,
            value_sum: 0,
            value_min: value.value,
            value_max: value.value,
            value_first: value.value,
            value_last: value.value,
            counter_delta: None,
        });
        bucket.point_count += 1;
        bucket.value_sum = bucket.value_sum.saturating_add(value.value);
        bucket.value_min = bucket.value_min.min(value.value);
        bucket.value_max = bucket.value_max.max(value.value);
        bucket.value_last = value.value;
        if counter {
            if let Some(growth) = growth {
                bucket.counter_delta = Some(bucket.counter_delta.unwrap_or_default() + growth);
            }
            previous = Some(value.value);
        }
    }
    Ok(buckets.into_values().collect())
}

/// Merges day aggregates, oldest first, into the months or years they belong to.
fn merge_days(
    zone: Tz,
    resolution: Resolution,
    days: &[ChannelRollupRecord],
) -> Result<Vec<ChannelRollupRecord>, AggregationError> {
    let mut buckets: BTreeMap<i64, ChannelRollupRecord> = BTreeMap::new();
    for day in days {
        let (_, start, end) = window(zone, resolution, local_time(zone, day.bucket_start)?)?;
        buckets
            .entry(start)
            .and_modify(|bucket| {
                bucket.point_count += day.point_count;
                bucket.value_sum = bucket.value_sum.saturating_add(day.value_sum);
                bucket.value_min = bucket.value_min.min(day.value_min);
                bucket.value_max = bucket.value_max.max(day.value_max);
                bucket.value_last = day.value_last;
                if let Some(growth) = day.counter_delta {
                    bucket.counter_delta = Some(bucket.counter_delta.unwrap_or_default() + growth);
                }
            })
            .or_insert_with(|| ChannelRollupRecord {
                resolution: resolution.name().to_owned(),
                bucket_start: start,
                bucket_end: end,
                ..day.clone()
            });
    }
    Ok(buckets.into_values().collect())
}

/// The value a channel reports for a bucket, by its aggregation kind.
///
/// A mean is rounded to the nearest stored integer. A counter's growth is
/// absent while the bucket's first value has no predecessor.
#[must_use]
pub fn aggregate_value(aggregation: &str, rollup: &ChannelRollupRecord) -> Option<i64> {
    match aggregation {
        "mean" => {
            let count = rollup.point_count.max(1);
            let half = count / 2;
            Some(if rollup.value_sum >= 0 {
                (rollup.value_sum + half) / count
            } else {
                (rollup.value_sum - half) / count
            })
        }
        "sum" => Some(rollup.value_sum),
        "min" => Some(rollup.value_min),
        "max" => Some(rollup.value_max),
        "last" => Some(rollup.value_last),
        "counter_delta" => rollup.counter_delta,
        _ => None,
    }
}
