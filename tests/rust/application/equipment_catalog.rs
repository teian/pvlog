use std::error::Error;

use pvlog_application::{
    EquipmentCatalog, EquipmentCatalogError, EquipmentCatalogQuery, EquipmentConfigurationError,
    confirm_module_snapshot, confirm_string_composition, prefill_inverter_from_catalog,
    prefill_module_from_catalog,
};
use pvlog_domain::{CatalogEntryId, EquipmentValueProvenance};

const CATALOG: &str = include_str!("../../../assets/equipment-catalog/catalog-v1.json");

#[test]
fn bundled_catalog_preserves_module_and_asymmetric_mppt_data() -> Result<(), Box<dyn Error>> {
    let catalog = EquipmentCatalog::bundled()?;
    assert_eq!(catalog.revision().0, "2026.07.11.1");

    let module = catalog
        .solar_module(&CatalogEntryId("ja-solar-jam54d40-450-lb".to_owned()))
        .ok_or("module missing")?;
    let specification = &module.specification;
    assert_eq!(specification.peak_power_watts, 450);
    assert_eq!(specification.open_circuit_voltage_millivolts, 39_300);
    assert_eq!(specification.maximum_power_voltage_millivolts, 32_820);
    assert_eq!(specification.short_circuit_current_milliamperes, 14_480);
    assert_eq!(specification.maximum_power_current_milliamperes, 13_710);
    assert_eq!(specification.efficiency_basis_points, 2_250);
    assert_eq!(specification.bifaciality_factor_basis_points, Some(8_000));
    assert_eq!(
        specification.bifaciality_tolerance_basis_points,
        Some(1_000)
    );
    assert_eq!(specification.dimensions_millimetres.length, 1_762);
    assert_eq!(specification.weight_grams, 22_000);

    let inverter = catalog
        .inverter(&CatalogEntryId("fronius-symo-gen24-10-0".to_owned()))
        .ok_or("inverter missing")?;
    assert_eq!(inverter.dc.total_string_input_count, 3);
    assert_eq!(inverter.dc.mppt_inputs.len(), 2);
    assert_eq!(inverter.dc.mppt_inputs[0].string_input_count, 2);
    assert_eq!(inverter.dc.mppt_inputs[1].string_input_count, 1);
    assert_eq!(
        inverter.dc.mppt_inputs[0].maximum_operating_current_milliamperes,
        Some(25_000)
    );
    assert_eq!(
        inverter.dc.mppt_inputs[1].maximum_short_circuit_current_milliamperes,
        Some(20_000)
    );
    Ok(())
}

#[test]
fn catalog_search_is_filtered_bounded_and_deterministic() -> Result<(), Box<dyn Error>> {
    let catalog = EquipmentCatalog::bundled()?;
    let goodwe = catalog.inverters(&EquipmentCatalogQuery {
        search: Some("gw10".to_owned()),
        manufacturer: Some(" goodwe ".to_owned()),
        offset: 0,
        limit: 25,
    });
    assert_eq!(goodwe.total, 1);
    assert_eq!(goodwe.items[0].model, "GW10K-ET");

    let first_page = catalog.inverters(&EquipmentCatalogQuery {
        search: None,
        manufacturer: None,
        offset: 1,
        limit: 2,
    });
    assert_eq!(first_page.total, 4);
    assert_eq!(first_page.items.len(), 2);
    assert_eq!(first_page.items[0].id.0, "goodwe-gw10k-et");
    assert_eq!(first_page.items[1].id.0, "huawei-sun2000-10ktl-m1");
    Ok(())
}

#[test]
fn parser_rejects_duplicate_ids_invalid_voltage_and_schema() -> Result<(), Box<dyn Error>> {
    let mut duplicate: serde_json::Value = serde_json::from_str(CATALOG)?;
    let first = duplicate["inverters"][0].clone();
    duplicate["inverters"]
        .as_array_mut()
        .ok_or("array")?
        .insert(1, first);
    assert!(matches!(
        EquipmentCatalog::parse(&serde_json::to_string(&duplicate)?),
        Err(EquipmentCatalogError::InvalidEntry(message)) if message.contains("duplicate")
    ));

    let mut invalid_voltage: serde_json::Value = serde_json::from_str(CATALOG)?;
    invalid_voltage["solarModules"][0]["specification"]["maximumPowerVoltageMillivolts"] =
        serde_json::json!(40_000);
    assert!(matches!(
        EquipmentCatalog::parse(&serde_json::to_string(&invalid_voltage)?),
        Err(EquipmentCatalogError::InvalidEntry(message)) if message.contains("electrical")
    ));

    let mut unsupported: serde_json::Value = serde_json::from_str(CATALOG)?;
    unsupported["schemaVersion"] = serde_json::json!(2);
    assert!(matches!(
        EquipmentCatalog::parse(&serde_json::to_string(&unsupported)?),
        Err(EquipmentCatalogError::UnsupportedSchema)
    ));
    Ok(())
}

#[test]
fn prefills_are_editable_and_reapplication_is_explicit() -> Result<(), Box<dyn Error>> {
    let catalog = EquipmentCatalog::bundled()?;
    let inverter_id = CatalogEntryId("fronius-symo-gen24-10-0".to_owned());
    let original = prefill_inverter_from_catalog(&catalog, &inverter_id)?;
    let reapplied = prefill_inverter_from_catalog(&catalog, &inverter_id)?;
    assert_eq!(original, reapplied);
    assert_eq!(
        reapplied
            .template
            .as_ref()
            .map(|template| template.value_provenance),
        Some(EquipmentValueProvenance::CatalogCopied)
    );

    let module_id = CatalogEntryId("ja-solar-jam54d40-450-lb".to_owned());
    let unchanged =
        confirm_module_snapshot(&catalog, prefill_module_from_catalog(&catalog, &module_id)?)?;
    assert_eq!(
        unchanged
            .template
            .as_ref()
            .map(|template| template.value_provenance),
        Some(EquipmentValueProvenance::CatalogCopied)
    );

    let mut edited = prefill_module_from_catalog(&catalog, &module_id)?;
    edited.specification.peak_power_watts = 455;
    let edited = confirm_module_snapshot(&catalog, edited)?;
    assert_eq!(
        edited
            .template
            .as_ref()
            .map(|template| template.value_provenance),
        Some(EquipmentValueProvenance::CatalogCustomized)
    );
    Ok(())
}

#[test]
fn string_composition_rejects_bounds_and_derives_capacity() -> Result<(), Box<dyn Error>> {
    let catalog = EquipmentCatalog::bundled()?;
    let module = prefill_module_from_catalog(
        &catalog,
        &CatalogEntryId("ja-solar-jam54d40-450-lb".to_owned()),
    )?;
    let string = confirm_string_composition(&catalog, 18, module.clone())?;
    assert_eq!(string.total_peak_power_watts, 8_100);
    assert_eq!(
        confirm_string_composition(&catalog, 0, module.clone()),
        Err(EquipmentConfigurationError::InvalidModuleCount)
    );
    assert_eq!(
        confirm_string_composition(&catalog, 10_001, module),
        Err(EquipmentConfigurationError::InvalidModuleCount)
    );
    Ok(())
}
