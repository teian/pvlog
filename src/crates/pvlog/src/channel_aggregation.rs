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

use std::collections::{BTreeMap, HashMap};

use chrono_tz::Tz;
use pvlog_domain::{ChannelId, SystemId};
use pvlog_storage::{
    ChannelRecord, ChannelRepository, ChannelRollupRecord, ChannelValueRecord, DatabaseTarget,
    ManagementRepository, PendingChannelRollup,
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
    // The whole local days of each channel to rebuild: every finer bucket lies
    // within one of them. A counter starts from the value measured before.
    let mut runs = Vec::with_capacity(pending.len());
    for pending in pending {
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
        let counter = pending.channel.aggregation == "counter_delta";
        let previous = if counter {
            repository
                .channel_value_before(pending.channel.id, days_from)
                .await
                .map_err(unavailable)?
                .map(|value| value.value)
        } else {
            None
        };
        runs.push(ChannelRun {
            pending,
            days_from,
            days_to,
            counter,
            previous,
        });
    }
    let (Some(start), Some(end)) = (
        runs.iter().map(|run| run.days_from).min(),
        runs.iter().map(|run| run.days_to).max(),
    ) else {
        return Ok(0);
    };

    // One local day at a time, with the values of every pending channel read
    // in one pass: years of history are neither read channel by channel nor
    // held in memory at once, and every finer bucket lies within a day. The
    // aggregates of a month are written together, while the next day is read.
    let mut chunks = Vec::new();
    let mut chunk_from = start;
    while chunk_from < end {
        let (_, _, day_end) = window(zone, Resolution::Day, local_time(zone, chunk_from)?)?;
        let chunk_to = day_end.min(end);
        chunks.push((chunk_from, chunk_to));
        chunk_from = chunk_to;
    }
    let read = |(from, to): (i64, i64), channels: Vec<ChannelId>| async move {
        repository
            .system_channel_values(system_id, &channels, from, to)
            .await
            .map_err(|_| AggregationError::Unavailable)
    };
    let mut written = 0_u64;
    let mut staged: BTreeMap<usize, Staged> = BTreeMap::new();
    let mut next = match chunks.first() {
        Some(&chunk) => Some(read(chunk, channels_within(&runs, chunk)).await?),
        None => None,
    };
    for (index, &chunk) in chunks.iter().enumerate() {
        build_chunk(
            zone,
            &mut runs,
            chunk,
            next.take().unwrap_or_default(),
            &mut staged,
        )?;
        let (_, _, month_end) = window(zone, Resolution::Month, local_time(zone, chunk.0)?)?;
        let ready = if chunk.1 >= month_end || index + 1 == chunks.len() {
            std::mem::take(&mut staged)
        } else {
            BTreeMap::new()
        };
        let following = chunks
            .get(index + 1)
            .map(|&chunk| read(chunk, channels_within(&runs, chunk)));
        let (stored, following) = tokio::join!(
            write_staged(repository, &runs, ready, calculated_at),
            async {
                match following {
                    Some(reading) => Some(reading.await),
                    None => None,
                }
            }
        );
        written += stored?;
        next = following.transpose()?;
    }

    for run in runs {
        let pending = &run.pending;
        let channel = &pending.channel;
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
        written += u64::try_from(coarse.len()).unwrap_or(u64::MAX);
    }
    Ok(written)
}

/// The pending channels with days to rebuild in `[from, to)`.
fn channels_within(runs: &[ChannelRun], (from, to): (i64, i64)) -> Vec<ChannelId> {
    runs.iter()
        .filter(|run| run.days_from < to && run.days_to > from)
        .map(|run| run.pending.channel.id)
        .collect()
}

/// The quarter-hour, hour and day aggregates of one channel built for
/// consecutive days and not yet written.
struct Staged {
    /// Start of the first day they cover.
    from: i64,
    /// End of the last day they cover.
    to: i64,
    rollups: Vec<ChannelRollupRecord>,
}

/// Builds the quarter-hour, hour and day aggregates of the pending channels
/// within `[from, to)` from their values there, oldest first, and stages them
/// under each channel's run.
fn build_chunk(
    zone: Tz,
    runs: &mut [ChannelRun],
    (from, to): (i64, i64),
    values: Vec<(ChannelId, ChannelValueRecord)>,
    staged: &mut BTreeMap<usize, Staged>,
) -> Result<(), AggregationError> {
    let mut by_channel: HashMap<ChannelId, Vec<ChannelValueRecord>> = HashMap::new();
    for (channel_id, value) in values {
        by_channel.entry(channel_id).or_default().push(value);
    }
    for (index, run) in runs
        .iter_mut()
        .enumerate()
        .filter(|(_, run)| run.days_from < to && run.days_to > from)
    {
        let channel = &run.pending.channel;
        let (low, high) = (run.days_from.max(from), run.days_to.min(to));
        let values: Vec<ChannelValueRecord> = by_channel
            .remove(&channel.id)
            .unwrap_or_default()
            .into_iter()
            .filter(|value| (low..high).contains(&value.measured_at))
            .collect();
        let stage = staged.entry(index).or_insert(Staged {
            from: low,
            to: high,
            rollups: Vec::new(),
        });
        stage.to = high;
        for resolution in FINE {
            stage.rollups.extend(bucket_values(
                zone,
                resolution,
                channel.id,
                &values,
                run.counter,
                run.previous,
            )?);
        }
        if run.counter
            && let Some(last) = values.last()
        {
            run.previous = Some(last.value);
        }
    }
    Ok(())
}

/// Writes staged aggregates in place of the stored ones of the days they
/// cover and returns how many were written.
async fn write_staged(
    repository: &dyn ChannelRepository,
    runs: &[ChannelRun],
    staged: BTreeMap<usize, Staged>,
    calculated_at: i64,
) -> Result<u64, AggregationError> {
    let mut written = 0_u64;
    for (index, stage) in staged {
        replace(
            repository,
            &runs[index].pending.channel,
            FINE,
            stage.from,
            stage.to,
            &stage.rollups,
            calculated_at,
        )
        .await?;
        written += u64::try_from(stage.rollups.len()).unwrap_or(u64::MAX);
    }
    Ok(written)
}

/// What a cycle rebuilds of one channel.
struct ChannelRun {
    pending: PendingChannelRollup,
    /// Start of the first local day to rebuild.
    days_from: i64,
    /// End of the last local day to rebuild.
    days_to: i64,
    counter: bool,
    /// For a counter, the value measured before the next one to aggregate.
    previous: Option<i64>,
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
    // The values come oldest first, so the bucket of the previous value is
    // usually the bucket of this one too; it is looked up only when left.
    let mut current: Option<(i64, i64)> = None;
    for value in values {
        let (start, end) = match current {
            Some((start, end)) if (start..end).contains(&value.measured_at) => (start, end),
            _ => {
                let (_, start, end) =
                    window(zone, resolution, local_time(zone, value.measured_at)?)?;
                current = Some((start, end));
                (start, end)
            }
        };
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
