//! A system's setup status follows from what it has, and a working system is
//! never presented as unfinished.

use pvlog_application::{SetupFacts, SetupItem, SetupItemKey, SetupItemState, setup_status};

fn item(items: &[SetupItem], key: SetupItemKey) -> SetupItem {
    items
        .iter()
        .copied()
        .find(|entry| entry.key == key)
        .unwrap_or_else(|| panic!("{key:?} missing"))
}

fn states(items: &[SetupItem], state: SetupItemState) -> Vec<SetupItemKey> {
    items
        .iter()
        .filter(|entry| entry.state == state)
        .map(|entry| entry.key)
        .collect()
}

#[test]
fn the_items_come_in_a_fixed_order() {
    let keys: Vec<SetupItemKey> = setup_status(&SetupFacts::default())
        .iter()
        .map(|entry| entry.key)
        .collect();
    assert_eq!(
        keys,
        [
            SetupItemKey::Location,
            SetupItemKey::Capacity,
            SetupItemKey::DataSource,
            SetupItemKey::FirstData,
            SetupItemKey::Channels,
            SetupItemKey::Equipment,
            SetupItemKey::Tariff,
            SetupItemKey::Forecast,
        ]
    );
}

#[test]
fn a_new_system_is_asked_for_a_data_source_and_nothing_else() {
    let items = setup_status(&SetupFacts::default());
    assert_eq!(
        states(&items, SetupItemState::Recommended),
        [SetupItemKey::DataSource]
    );
    let first_data = item(&items, SetupItemKey::FirstData);
    assert_eq!(first_data.state, SetupItemState::Blocked);
    assert_eq!(first_data.reason, Some("data_source_missing"));
    assert_eq!(
        states(&items, SetupItemState::Optional),
        [
            SetupItemKey::Location,
            SetupItemKey::Capacity,
            SetupItemKey::Equipment,
            SetupItemKey::Tariff,
            SetupItemKey::Forecast,
        ]
    );
    assert_eq!(
        states(&items, SetupItemState::Done),
        [SetupItemKey::Channels]
    );
}

#[test]
fn a_connected_source_without_data_asks_to_wait_for_the_first_reading() {
    let items = setup_status(&SetupFacts {
        data_sources: 1,
        ..SetupFacts::default()
    });
    assert_eq!(
        states(&items, SetupItemState::Recommended),
        [SetupItemKey::FirstData]
    );
    assert_eq!(
        item(&items, SetupItemKey::FirstData).reason,
        Some("waiting_for_first_reading")
    );
    assert_eq!(
        item(&items, SetupItemKey::DataSource).state,
        SetupItemState::Done
    );
}

#[test]
fn a_working_system_with_missing_details_recommends_nothing() {
    let items = setup_status(&SetupFacts {
        data_sources: 1,
        has_reading: true,
        ..SetupFacts::default()
    });
    assert_eq!(
        states(&items, SetupItemState::Recommended),
        [] as [pvlog_application::SetupItemKey; 0]
    );
    assert_eq!(
        states(&items, SetupItemState::Blocked),
        [] as [pvlog_application::SetupItemKey; 0]
    );
    for key in [SetupItemKey::Tariff, SetupItemKey::Location] {
        assert_eq!(item(&items, key).state, SetupItemState::Optional);
    }
}

#[test]
fn readings_without_a_data_source_still_count_as_connected() {
    // An uploader that predates data sources uses an account key.
    let items = setup_status(&SetupFacts {
        has_reading: true,
        ..SetupFacts::default()
    });
    assert_eq!(
        item(&items, SetupItemKey::DataSource).state,
        SetupItemState::Done
    );
    assert_eq!(
        item(&items, SetupItemKey::FirstData).state,
        SetupItemState::Done
    );
}

#[test]
fn unassigned_channels_are_counted() {
    let items = setup_status(&SetupFacts {
        unassigned_channels: 3,
        ..SetupFacts::default()
    });
    let channels = item(&items, SetupItemKey::Channels);
    assert_eq!(channels.state, SetupItemState::Optional);
    assert_eq!(channels.reason, Some("unassigned_channels"));
    assert_eq!(channels.count, Some(3));
}

#[test]
fn capacity_is_met_by_a_declared_value_or_by_strings() {
    for facts in [
        SetupFacts {
            has_declared_capacity: true,
            ..SetupFacts::default()
        },
        SetupFacts {
            strings: 2,
            ..SetupFacts::default()
        },
    ] {
        assert_eq!(
            item(&setup_status(&facts), SetupItemKey::Capacity).state,
            SetupItemState::Done
        );
    }
    let declared_only = setup_status(&SetupFacts {
        has_declared_capacity: true,
        ..SetupFacts::default()
    });
    assert_eq!(
        item(&declared_only, SetupItemKey::Equipment).reason,
        Some("equipment_missing"),
        "a declared capacity does not stand in for equipment"
    );
}

#[test]
fn a_completely_set_up_system_has_every_item_done() {
    let items = setup_status(&SetupFacts {
        has_location: true,
        has_declared_capacity: false,
        strings: 2,
        data_sources: 1,
        has_reading: true,
        unassigned_channels: 0,
        has_effective_tariff: true,
        forecast_ready: Some(true),
    });
    assert_eq!(states(&items, SetupItemState::Done).len(), items.len());
    assert!(items.iter().all(|entry| entry.reason.is_none()));
}

#[test]
fn forecast_readiness_names_why_it_is_open() {
    let reason = |forecast_ready| {
        item(
            &setup_status(&SetupFacts {
                forecast_ready,
                ..SetupFacts::default()
            }),
            SetupItemKey::Forecast,
        )
        .reason
    };
    assert_eq!(reason(Some(false)), Some("forecast_inputs_missing"));
    assert_eq!(reason(None), Some("forecast_readiness_unknown"));
}
