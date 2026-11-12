//! Typed channel values: exact decimals, the device key scheme, contradictions.

use pvlog_application::{
    ChannelAggregation, ChannelReading, ChannelTyping, ChannelValueError, ChannelValueType,
    DecimalValue, DeviceChannelTarget, channel_typing_for, device_channel, scaled_channel_value,
};

fn decimal(text: &str) -> Option<DecimalValue> {
    DecimalValue::parse(text)
}

fn value(text: &str, unit: Option<&str>) -> ChannelReading {
    ChannelReading {
        value: decimal(text),
        unit: unit.map(ToOwned::to_owned),
        ..ChannelReading::default()
    }
}

fn typing(value_type: ChannelValueType, unit: &str, scale: u8) -> ChannelTyping {
    ChannelTyping {
        value_type,
        unit: unit.to_owned(),
        scale,
        aggregation: ChannelAggregation::Mean,
    }
}

#[test]
fn decimals_are_parsed_and_rescaled_without_rounding() {
    assert_eq!(
        decimal("12.34"),
        Some(DecimalValue {
            mantissa: 1_234,
            scale: 2
        })
    );
    // Trailing zeros carry no information.
    assert_eq!(decimal("12.340"), decimal("12.34"));
    assert_eq!(decimal("-0.5").map(|value| value.mantissa), Some(-5));
    assert_eq!(decimal("230"), Some(DecimalValue::integer(230)));
    for malformed in ["", "-", ".5", "1e-7", "1.2.3", "12,5", "0.0000000001"] {
        assert_eq!(decimal(malformed), None, "{malformed}");
    }
    assert_eq!(decimal("99999999999999999999"), None, "does not fit");

    let parsed = decimal("12.34").unwrap_or(DecimalValue::integer(0));
    assert_eq!(parsed.at_scale(2), Some(1_234));
    assert_eq!(parsed.at_scale(4), Some(123_400));
    assert_eq!(parsed.at_scale(1), None, "a digit would be dropped");
    assert_eq!(DecimalValue::integer(i64::MAX).at_scale(1), None);
}

#[test]
#[allow(clippy::too_many_lines)]
fn device_keys_take_the_definition_of_the_key_scheme() {
    let cases = [
        ("inv.2000123456.ac.l1.power", "W", ChannelAggregation::Mean),
        (
            "inv.2000123456.ac.l3.current",
            "mA",
            ChannelAggregation::Mean,
        ),
        (
            "inv.2000123456.ac.l2.voltage",
            "mV",
            ChannelAggregation::Mean,
        ),
        (
            "inv.2000123456.grid.frequency",
            "mHz",
            ChannelAggregation::Mean,
        ),
        (
            "inv.2000123456.grid.import_power",
            "W",
            ChannelAggregation::Mean,
        ),
        (
            "inv.2000123456.grid.export_power",
            "W",
            ChannelAggregation::Mean,
        ),
        (
            "inv.2000123456.energy.today",
            "Wh",
            ChannelAggregation::Last,
        ),
        (
            "inv.2000123456.energy.lifetime",
            "Wh",
            ChannelAggregation::CounterDelta,
        ),
        (
            "inv.2000123456.operating_time",
            "s",
            ChannelAggregation::Last,
        ),
        ("inv.2000123456.feed_in_time", "s", ChannelAggregation::Last),
        ("inv.2000123456.status", "code", ChannelAggregation::Last),
        (
            "inv.2000123456.grid_relay",
            "code",
            ChannelAggregation::Last,
        ),
        ("inv.2000123456.temperature", "m°C", ChannelAggregation::Max),
        ("inv.2000123456.signal", "‰", ChannelAggregation::Mean),
        ("inv.2000123456.mppt.1.power", "W", ChannelAggregation::Mean),
        (
            "inv.2000123456.mppt.255.current",
            "mA",
            ChannelAggregation::Mean,
        ),
        (
            "inv.2000123456.mppt.5.voltage",
            "mV",
            ChannelAggregation::Mean,
        ),
        ("inv.2000123456.battery.soc", "‰", ChannelAggregation::Last),
        (
            "inv.2000123456.battery.voltage",
            "mV",
            ChannelAggregation::Mean,
        ),
        (
            "inv.2000123456.battery.current",
            "mA",
            ChannelAggregation::Mean,
        ),
        (
            "inv.2000123456.battery.temperature",
            "m°C",
            ChannelAggregation::Mean,
        ),
    ];
    for (key, unit, aggregation) in cases {
        let Some(channel) = device_channel(key) else {
            panic!("{key} belongs to the key scheme");
        };
        assert_eq!(channel.typing.unit, unit, "{key}");
        assert_eq!(channel.typing.aggregation, aggregation, "{key}");
        assert_eq!(channel.typing.scale, 0, "{key}");
        assert_eq!(channel.target.serial(), "2000123456", "{key}");
        assert_eq!(
            matches!(channel.target, DeviceChannelTarget::Tracker { .. }),
            key.contains(".mppt."),
            "{key}"
        );
        assert_eq!(
            channel.tracker_power,
            key.ends_with("mppt.1.power"),
            "{key}"
        );
    }
    assert_eq!(
        device_channel("inv.2000123456.energy.lifetime").map(|channel| channel.typing.value_type),
        Some(ChannelValueType::Counter)
    );
    assert_eq!(
        device_channel("inv.A-1_b.mppt.5.power").map(|channel| channel.target),
        Some(DeviceChannelTarget::Tracker {
            serial: "A-1_b",
            tracker: 5
        })
    );
    for other in [
        "mppt-1",
        "inv..status",
        "inv.2000123456",
        "inv.2000123456.mppt.0.power",
        "inv.2000123456.mppt.01.power",
        "inv.2000123456.mppt.256.power",
        "inv.2000123456.ac.l4.power",
        "inv.2000123456.ac.l1.frequency",
        "inv.20 00.status",
        "inv.2000123456.unknown",
    ] {
        assert_eq!(device_channel(other), None, "{other}");
    }
}

#[test]
fn an_unknown_key_is_registered_from_its_first_reading() {
    // A key of the scheme ignores what the reading claims.
    assert_eq!(
        channel_typing_for("inv.1.mppt.2.voltage", &value("1.5", Some("V"))),
        typing(ChannelValueType::Integer, "mV", 0)
    );
    // A number keeps room for decimals that a later reading may carry.
    assert_eq!(
        channel_typing_for("pvoutput.v7", &value("230", Some("V"))),
        typing(ChannelValueType::Decimal, "V", 3)
    );
    assert_eq!(
        channel_typing_for("fine", &value("0.12345", None)),
        typing(ChannelValueType::Decimal, "", 5)
    );
    // The earlier members imply watts and watt-hours.
    let power = ChannelReading {
        power_watts: Some(1_200),
        energy_watt_hours: Some(100),
        ..ChannelReading::default()
    };
    assert_eq!(
        channel_typing_for("mppt-1", &power),
        typing(ChannelValueType::Integer, "W", 0)
    );
    let energy = ChannelReading {
        energy_watt_hours: Some(100),
        ..ChannelReading::default()
    };
    assert_eq!(
        channel_typing_for("meter", &energy),
        ChannelTyping {
            aggregation: ChannelAggregation::Sum,
            ..typing(ChannelValueType::Integer, "Wh", 0)
        }
    );
}

#[test]
fn a_value_is_scaled_or_refused_by_the_channel_definition() {
    let volts = typing(ChannelValueType::Decimal, "V", 3);
    assert_eq!(
        scaled_channel_value(&volts, &value("12.34", Some("V"))),
        Ok(12_340)
    );
    assert_eq!(
        scaled_channel_value(&volts, &value("12.34", None)),
        Ok(12_340)
    );
    assert_eq!(
        scaled_channel_value(&volts, &value("12.34", Some("mV"))),
        Err(ChannelValueError::UnitMismatch)
    );
    assert_eq!(
        scaled_channel_value(&volts, &value("12.3456", Some("V"))),
        Err(ChannelValueError::TypeMismatch)
    );
    assert_eq!(
        scaled_channel_value(&volts, &value("9223372036854775807", Some("V"))),
        Err(ChannelValueError::OutOfRange)
    );
    assert_eq!(
        scaled_channel_value(&volts, &ChannelReading::default()),
        Err(ChannelValueError::ValueMissing)
    );

    let millivolts = typing(ChannelValueType::Integer, "mV", 0);
    assert_eq!(
        scaled_channel_value(&millivolts, &value("384120", Some("mV"))),
        Ok(384_120)
    );
    assert_eq!(
        scaled_channel_value(&millivolts, &value("0.5", Some("mV"))),
        Err(ChannelValueError::TypeMismatch)
    );
    // Watts sent to a channel that is not in watts contradict it.
    let power = ChannelReading {
        power_watts: Some(1_200),
        ..ChannelReading::default()
    };
    assert_eq!(
        scaled_channel_value(&millivolts, &power),
        Err(ChannelValueError::UnitMismatch)
    );

    // A channel in watts keeps taking the energy member when no power is sent.
    let watts = typing(ChannelValueType::Integer, "W", 0);
    let both = ChannelReading {
        power_watts: Some(1_200),
        energy_watt_hours: Some(100),
        ..ChannelReading::default()
    };
    let energy = ChannelReading {
        energy_watt_hours: Some(100),
        ..ChannelReading::default()
    };
    assert_eq!(scaled_channel_value(&watts, &both), Ok(1_200));
    assert_eq!(scaled_channel_value(&watts, &energy), Ok(100));
    let watt_hours = typing(ChannelValueType::Integer, "Wh", 0);
    assert_eq!(scaled_channel_value(&watt_hours, &both), Ok(100));

    let flag = typing(ChannelValueType::Boolean, "", 0);
    assert_eq!(scaled_channel_value(&flag, &value("1", None)), Ok(1));
    assert_eq!(
        scaled_channel_value(&flag, &value("2", None)),
        Err(ChannelValueError::TypeMismatch)
    );
}
