use std::{error::Error, str::FromStr as _};

use pvlog_domain::{
    AccountId, CalculationSettings, ChannelScale, GeographicPrecision, IanaTimezone, Inverter,
    InverterId, NetCalculationMode, PowerCalculationMode, PvString, PvSystem, StringId, SystemId,
    SystemLifecycle, SystemPrivacy, Visibility, Watts,
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
    let system_id = SystemId::new();
    let inverter_id = InverterId::new();
    let system = PvSystem {
        id: system_id,
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
        inverters: vec![Inverter {
            id: inverter_id,
            system_id,
            name: "Roof inverter".to_owned(),
            manufacturer: None,
            model: None,
            serial_reference: None,
            rated_power: Some(Watts::new(8_000)),
            period: pvlog_domain::EffectivePeriod::new(date!(2025 - 01 - 01), None)?,
            strings: vec![PvString {
                id: StringId::new(),
                inverter_id,
                name: "South roof".to_owned(),
                panel_count: 20,
                panel_manufacturer: None,
                panel_model: None,
                rated_power: Watts::new(8_000),
                orientation_degrees: Some(180),
                tilt_degrees: Some(35),
                period: pvlog_domain::EffectivePeriod::new(date!(2025 - 01 - 01), None)?,
            }],
        }],
    };

    assert_eq!(system.privacy.visibility, Visibility::Private);
    assert_eq!(
        system.privacy.location_precision,
        GeographicPrecision::Hidden
    );
    assert!(!system.privacy.discoverable);
    system.validate_hierarchy()?;

    let mut invalid = system.clone();
    invalid.inverters[0].strings[0].inverter_id = InverterId::new();
    assert!(invalid.validate_hierarchy().is_err());
    Ok(())
}

#[test]
fn extended_channel_scale_is_bounded() -> Result<(), Box<dyn Error>> {
    assert_eq!(ChannelScale::new(-3)?.exponent(), -3);
    assert!(ChannelScale::new(10).is_err());
    Ok(())
}
