//! Tariff periods and the revenue they imply.
//!
//! A system is paid different rates over its lifetime, and the current period
//! has no end. Energy is therefore priced with the rate in force when it was
//! produced and the pieces are summed; averaging one rate across a range would
//! move historical revenue every time a new period begins.

use pvlog_domain::SystemId;
use serde::{Deserialize, Serialize};
#[cfg(feature = "sqlite")]
use sqlx::Row;

/// One contiguous tariff period. An open `effective_to` marks the current one.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TariffPeriod {
    pub id: uuid::Uuid,
    pub name: String,
    /// `import` or `export`.
    pub direction: String,
    pub currency_code: String,
    pub minor_units_per_kwh: i64,
    pub effective_from_epoch_millis: i64,
    pub effective_to_epoch_millis: Option<i64>,
}

/// A new or replacing tariff period.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TariffInput {
    pub name: String,
    pub direction: String,
    pub currency_code: String,
    pub minor_units_per_kwh: i64,
    pub effective_from_epoch_millis: i64,
    pub effective_to_epoch_millis: Option<i64>,
}

/// Revenue for one range, with the pieces it was assembled from.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Revenue {
    pub minor_units: i64,
    pub currency_code: Option<String>,
    /// How many periods the range touched; more than one means the rate changed.
    pub periods_applied: u32,
    /// Energy no period covered, reported instead of being priced at zero.
    pub unpriced_energy_wh: i64,
}

/// Prices energy buckets with the tariff in force at each bucket's start.
///
/// Buckets are `(bucket_start_epoch_millis, energy_wh)`. A bucket outside every
/// period is counted as unpriced rather than silently dropped.
#[must_use]
pub fn revenue_for(buckets: &[(i64, i64)], periods: &[TariffPeriod]) -> Revenue {
    let mut revenue = Revenue::default();
    let mut applied: std::collections::BTreeSet<uuid::Uuid> = std::collections::BTreeSet::new();
    for (bucket_start, energy_wh) in buckets {
        let Some(period) = periods.iter().find(|period| {
            period.direction == "export"
                && period.effective_from_epoch_millis <= *bucket_start
                && period
                    .effective_to_epoch_millis
                    .is_none_or(|end| *bucket_start < end)
        }) else {
            revenue.unpriced_energy_wh += *energy_wh;
            continue;
        };
        applied.insert(period.id);
        revenue.currency_code = Some(period.currency_code.clone());
        // Rates are per kilowatt-hour; dividing after the product keeps the
        // rounding error below one minor unit per bucket.
        revenue.minor_units += energy_wh.saturating_mul(period.minor_units_per_kwh) / 1_000;
    }
    revenue.periods_applied = u32::try_from(applied.len()).unwrap_or(u32::MAX);
    revenue
}

/// Rejects a period that overlaps an existing one in the same direction.
#[must_use]
pub fn overlaps(
    candidate: &TariffInput,
    existing: &[TariffPeriod],
    replacing: Option<uuid::Uuid>,
) -> bool {
    existing.iter().any(|period| {
        if Some(period.id) == replacing || period.direction != candidate.direction {
            return false;
        }
        let candidate_end = candidate.effective_to_epoch_millis.unwrap_or(i64::MAX);
        let period_end = period.effective_to_epoch_millis.unwrap_or(i64::MAX);
        candidate.effective_from_epoch_millis < period_end
            && period.effective_from_epoch_millis < candidate_end
    })
}

/// Reads every tariff period of a system, newest first.
///
/// # Errors
///
/// Returns an error when the account database cannot be queried.
#[cfg(feature = "sqlite")]
pub async fn periods(
    connection: &mut sqlx::SqliteConnection,
    system_id: SystemId,
) -> Result<Vec<TariffPeriod>, sqlx::Error> {
    let rows = sqlx::query(
        "SELECT id,name,direction,currency_code,minor_units_per_kwh,effective_from,effective_to \
         FROM tariffs WHERE system_id=? ORDER BY effective_from DESC",
    )
    .bind(system_id.as_uuid().as_bytes().to_vec())
    .fetch_all(&mut *connection)
    .await?;
    Ok(rows
        .iter()
        .filter_map(|row| {
            Some(TariffPeriod {
                id: uuid::Uuid::from_slice(&row.get::<Vec<u8>, _>("id")).ok()?,
                name: row.get("name"),
                direction: row.get("direction"),
                currency_code: row.get("currency_code"),
                minor_units_per_kwh: row.get("minor_units_per_kwh"),
                effective_from_epoch_millis: row.get("effective_from"),
                effective_to_epoch_millis: row.try_get("effective_to").ok().flatten(),
            })
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::{Revenue, TariffPeriod, revenue_for};

    fn period(id: u128, rate: i64, from: i64, to: Option<i64>) -> TariffPeriod {
        TariffPeriod {
            id: uuid::Uuid::from_u128(id),
            name: "Feed-in".to_owned(),
            direction: "export".to_owned(),
            currency_code: "EUR".to_owned(),
            minor_units_per_kwh: rate,
            effective_from_epoch_millis: from,
            effective_to_epoch_millis: to,
        }
    }

    #[test]
    fn a_range_spanning_two_periods_prices_each_piece_separately() {
        let periods = [period(1, 8, 0, Some(100)), period(2, 12, 100, None)];
        // 1000 Wh before the switch, 1000 Wh after it.
        let revenue = revenue_for(&[(50, 1_000), (150, 1_000)], &periods);
        assert_eq!(
            revenue,
            Revenue {
                minor_units: 8 + 12,
                currency_code: Some("EUR".to_owned()),
                periods_applied: 2,
                unpriced_energy_wh: 0,
            },
            "each bucket takes the rate in force when it was produced"
        );
    }

    #[test]
    fn energy_outside_every_period_is_reported_rather_than_priced() {
        let periods = [period(1, 8, 1_000, None)];
        let revenue = revenue_for(&[(0, 500), (2_000, 1_000)], &periods);
        assert_eq!(revenue.minor_units, 8);
        assert_eq!(revenue.unpriced_energy_wh, 500);
    }
}
