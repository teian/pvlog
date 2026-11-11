//! Moved out of the crate: this repository keeps Rust tests in tests/.

use pvlog::tariffs::{Revenue, TariffPeriod, revenue_for};

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
