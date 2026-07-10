use std::{error::Error, str::FromStr as _};

use pvlog_domain::{
    AccountId, CalculationSettings, ChannelScale, GeographicPrecision, IanaTimezone,
    NetCalculationMode, PowerCalculationMode, PvSystem, SystemId, SystemLifecycle, SystemPrivacy,
    Visibility,
};
use time::macros::date;

#[test]
fn effective_periods_are_half_open_and_nonempty() -> Result<(), Box<dyn Error>> {
    let period =
        pvlog_domain::EffectivePeriod::new(date!(2025 - 01 - 01), Some(date!(2026 - 01 - 01)))?;

    assert!(period.contains(date!(2025 - 01 - 01)));
    assert!(period.contains(date!(2025 - 12 - 31)));
    assert!(!period.contains(date!(2026 - 01 - 01)));
    assert!(
        pvlog_domain::EffectivePeriod::new(date!(2025 - 01 - 01), Some(date!(2025 - 01 - 01)))
            .is_err()
    );
    Ok(())
}

#[test]
fn new_system_shape_can_apply_privacy_first_defaults() -> Result<(), Box<dyn Error>> {
    let system = PvSystem {
        id: SystemId::new(),
        account_id: AccountId::new(),
        name: "Roof array".to_owned(),
        description: None,
        timezone: IanaTimezone::from_str("Europe/Berlin")?,
        commissioning_date: date!(2025 - 01 - 01),
        country_code: Some("DE".to_owned()),
        latitude_microdegrees: None,
        longitude_microdegrees: None,
        status_interval_seconds: 300,
        lifecycle: SystemLifecycle::Active,
        privacy: SystemPrivacy::default(),
        calculation: CalculationSettings {
            power: PowerCalculationMode::MeasuredOnly,
            net: NetCalculationMode::Measured,
            derive_interval_energy: false,
            currency: None,
        },
    };

    assert_eq!(system.privacy.visibility, Visibility::Private);
    assert_eq!(
        system.privacy.location_precision,
        GeographicPrecision::Hidden
    );
    assert!(!system.privacy.discoverable);
    Ok(())
}

#[test]
fn extended_channel_scale_is_bounded() -> Result<(), Box<dyn Error>> {
    assert_eq!(ChannelScale::new(-3)?.exponent(), -3);
    assert!(ChannelScale::new(10).is_err());
    Ok(())
}
