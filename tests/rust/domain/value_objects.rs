use std::{error::Error, str::FromStr as _};

use pvlog_domain::{
    AccountId, BasisPoints, CurrencyCode, IanaTimezone, Money, QualityFlags, UtcTimestamp,
    Visibility, WattHours, Watts,
};
use time::{OffsetDateTime, UtcOffset, macros::datetime};

#[test]
fn identifiers_are_strongly_typed_uuid_v7_values() {
    let account_id = AccountId::new();

    assert_eq!(account_id.as_uuid().get_version_num(), 7);
    assert_eq!(AccountId::from_str(&account_id.to_string()), Ok(account_id));
    assert!(AccountId::from_str("not-an-id").is_err());
}

#[test]
fn timestamps_are_normalized_to_utc_and_round_trip_epoch_milliseconds() -> Result<(), Box<dyn Error>>
{
    let source = datetime!(2026-07-10 12:30:45.123 +02:00);
    let timestamp = UtcTimestamp::new(source);
    let epoch_millis = i64::try_from(timestamp.epoch_millis())?;

    assert_eq!(timestamp.as_datetime().offset(), UtcOffset::UTC);
    assert_eq!(UtcTimestamp::from_epoch_millis(epoch_millis), Ok(timestamp));
    assert_eq!(timestamp.as_datetime(), source.to_offset(UtcOffset::UTC));
    Ok(())
}

#[test]
fn iana_timezones_are_case_sensitive_and_serde_validated() -> Result<(), Box<dyn Error>> {
    let berlin = IanaTimezone::from_str("Europe/Berlin")?;

    assert_eq!(berlin.as_str(), "Europe/Berlin");
    assert_eq!(serde_json::to_string(&berlin)?, "\"Europe/Berlin\"");
    assert!(IanaTimezone::from_str("europe/berlin").is_err());
    assert!(serde_json::from_str::<IanaTimezone>("\"Mars/Olympus\"").is_err());
    Ok(())
}

#[test]
fn integer_units_do_not_mix_power_and_energy() -> Result<(), Box<dyn Error>> {
    let power = Watts::new(-750);
    let energy = WattHours::new(1_250);

    assert_eq!(power.value(), -750);
    assert_eq!(energy.value(), 1_250);
    assert_eq!(serde_json::to_string(&power)?, "-750");
    Ok(())
}

#[test]
fn basis_points_and_quality_bits_reject_invalid_boundary_values() -> Result<(), Box<dyn Error>> {
    assert_eq!(BasisPoints::new(10_000)?.value(), 10_000);
    assert!(BasisPoints::new(10_001).is_err());
    assert!(serde_json::from_str::<BasisPoints>("-10001").is_err());

    let flags = QualityFlags::ESTIMATED | QualityFlags::CORRECTED;
    assert!(flags.contains(QualityFlags::ESTIMATED));
    assert_eq!(flags.bits(), 5);
    assert!(QualityFlags::from_bits(1 << 15).is_err());
    assert!(serde_json::from_str::<QualityFlags>("32768").is_err());
    Ok(())
}

#[test]
fn money_uses_exact_minor_units_and_validated_currency() -> Result<(), Box<dyn Error>> {
    let euros = CurrencyCode::from_str("EUR")?;
    let amount = Money::new(1_099, euros);

    assert_eq!(amount.minor_units, 1_099);
    assert_eq!(amount.currency.as_str(), "EUR");
    assert!(CurrencyCode::from_str("eur").is_err());
    assert!(serde_json::from_str::<Money>(r#"{"minor_units":10,"currency":"EURO"}"#).is_err());
    Ok(())
}

#[test]
fn visibility_defaults_to_private() {
    assert_eq!(Visibility::default(), Visibility::Private);
}

#[test]
fn timestamp_range_errors_do_not_echo_rejected_values() {
    let Err(error) = UtcTimestamp::from_epoch_millis(i64::MAX) else {
        panic!("i64::MAX milliseconds must be outside the supported timestamp range");
    };

    assert_eq!(error.code, "timestamp_out_of_range");
    assert_eq!(error.field, "timestamp");
    assert!(!error.to_string().contains(&i64::MAX.to_string()));
}

#[test]
fn timestamp_constructor_accepts_known_epoch() -> Result<(), Box<dyn Error>> {
    let timestamp = UtcTimestamp::from_epoch_millis(0)?;

    assert_eq!(timestamp.as_datetime(), OffsetDateTime::UNIX_EPOCH);
    Ok(())
}
