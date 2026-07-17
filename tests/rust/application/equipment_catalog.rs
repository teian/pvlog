use std::error::Error;

use pvlog_application::{
    EquipmentCatalog, EquipmentCatalogError, EquipmentCatalogQuery, EquipmentConfigurationError,
    confirm_module_snapshot, confirm_string_composition, prefill_inverter_from_catalog,
    prefill_module_from_catalog,
};
use pvlog_domain::{CatalogEntryId, EquipmentValueProvenance};

const INVERTER_CATALOG: &str =
    include_str!("../../../assets/equipment-catalog/inverter-catalog-v1.json");
const MODULE_CATALOG: &str =
    include_str!("../../../assets/equipment-catalog/pv-module-catalog-v1.json");

#[test]
fn bundled_catalog_preserves_module_and_asymmetric_mppt_data() -> Result<(), Box<dyn Error>> {
    let catalog = EquipmentCatalog::bundled()?;
    assert_eq!(catalog.inverter_revision().0, "pvlog-2026.07.17.2");
    assert_eq!(
        catalog.module_revision().0,
        "open-pv-module-database-2026.07.12"
    );

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
    let dimensions = specification
        .dimensions_millimetres
        .ok_or("catalog dimensions missing")?;
    assert_eq!(dimensions.length, 1_762);
    assert_eq!(specification.weight_grams, Some(22_000));

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

    let sunny_boy = catalog
        .inverter(&CatalogEntryId("sma-sunny-boy-5000tl-20".to_owned()))
        .ok_or("SMA Sunny Boy 5000TL missing")?;
    assert_eq!(sunny_boy.model, "Sunny Boy 5000TL (SB 5000TL-20)");
    assert_eq!(sunny_boy.dc.total_string_input_count, 4);
    assert_eq!(sunny_boy.dc.mppt_inputs.len(), 2);
    assert_eq!(sunny_boy.dc.mppt_inputs[0].string_input_count, 2);
    assert_eq!(sunny_boy.dc.mppt_inputs[1].string_input_count, 2);
    assert_eq!(sunny_boy.dc.maximum_input_voltage_millivolts, Some(550_000));
    assert_eq!(sunny_boy.dc.minimum_mppt_voltage_millivolts, Some(175_000));
    assert_eq!(sunny_boy.dc.maximum_mppt_voltage_millivolts, Some(440_000));
    assert_eq!(sunny_boy.ac.phase_count, 1);
    assert_eq!(sunny_boy.ac.rated_active_power_watts, 4_600);
    assert_eq!(
        sunny_boy.operational.maximum_efficiency_basis_points,
        Some(9_700)
    );
    assert_eq!(
        sunny_boy.operational.european_efficiency_basis_points,
        Some(9_650)
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

    let sma = catalog.inverters(&EquipmentCatalogQuery {
        search: Some("sunny boy 5000tl".to_owned()),
        manufacturer: Some("SMA".to_owned()),
        offset: 0,
        limit: 25,
    });
    assert_eq!(sma.total, 1);
    assert_eq!(sma.items[0].id.0, "sma-sunny-boy-5000tl-20");

    let imported_modules = catalog.solar_modules(&EquipmentCatalogQuery {
        search: Some("6MN6A270".to_owned()),
        manufacturer: Some("Ablytek".to_owned()),
        offset: 0,
        limit: 25,
    });
    assert_eq!(imported_modules.total, 1);
    assert_eq!(
        imported_modules.items[0].specification.peak_power_watts,
        270
    );

    let first_page = catalog.inverters(&EquipmentCatalogQuery {
        search: None,
        manufacturer: None,
        offset: 1,
        limit: 2,
    });
    assert_eq!(first_page.total, 9);
    assert_eq!(first_page.items.len(), 2);
    assert_eq!(first_page.items[0].id.0, "goodwe-gw10k-et");
    assert_eq!(first_page.items[1].id.0, "huawei-sun2000-10ktl-m1");
    Ok(())
}

#[test]
fn bundled_catalog_includes_legacy_sunny_boy_family() -> Result<(), Box<dyn Error>> {
    let catalog = EquipmentCatalog::bundled()?;
    let expected = [
        ("1200", 2, 400_000, 100_000, 320_000, 1_200, 9_210, 9_090),
        ("1700", 2, 400_000, 147_000, 320_000, 1_550, 9_350, 9_180),
        ("2500", 3, 600_000, 224_000, 480_000, 2_300, 9_410, 9_320),
        ("3000", 3, 600_000, 268_000, 480_000, 2_750, 9_500, 9_360),
    ];
    for (
        model,
        strings,
        maximum_voltage,
        minimum_mppt_voltage,
        maximum_mppt_voltage,
        rated_power,
        maximum_efficiency,
        european_efficiency,
    ) in expected
    {
        let inverter = catalog
            .inverter(&CatalogEntryId(format!("sma-sunny-boy-{model}")))
            .ok_or("legacy SMA Sunny Boy missing")?;
        assert_eq!(inverter.dc.total_string_input_count, strings);
        assert_eq!(inverter.dc.mppt_inputs.len(), 1);
        assert_eq!(
            inverter.dc.maximum_input_voltage_millivolts,
            Some(maximum_voltage)
        );
        assert_eq!(
            inverter.dc.minimum_mppt_voltage_millivolts,
            Some(minimum_mppt_voltage)
        );
        assert_eq!(
            inverter.dc.maximum_mppt_voltage_millivolts,
            Some(maximum_mppt_voltage)
        );
        assert_eq!(inverter.ac.phase_count, 1);
        assert_eq!(inverter.ac.rated_active_power_watts, rated_power);
        assert_eq!(
            inverter.operational.maximum_efficiency_basis_points,
            Some(maximum_efficiency)
        );
        assert_eq!(
            inverter.operational.european_efficiency_basis_points,
            Some(european_efficiency)
        );
    }
    Ok(())
}

#[test]
fn parser_rejects_duplicate_ids_invalid_voltage_and_schema() -> Result<(), Box<dyn Error>> {
    let mut duplicate: serde_json::Value = serde_json::from_str(INVERTER_CATALOG)?;
    let first = duplicate["inverters"][0].clone();
    duplicate["inverters"]
        .as_array_mut()
        .ok_or("array")?
        .insert(1, first);
    assert!(matches!(
        EquipmentCatalog::parse(&serde_json::to_string(&duplicate)?, MODULE_CATALOG),
        Err(EquipmentCatalogError::InvalidEntry(message)) if message.contains("duplicate")
    ));

    let mut invalid_voltage: serde_json::Value = serde_json::from_str(MODULE_CATALOG)?;
    invalid_voltage["solarModules"][0]["specification"]["maximumPowerVoltageMillivolts"] =
        serde_json::json!(40_000);
    assert!(matches!(
        EquipmentCatalog::parse(INVERTER_CATALOG, &serde_json::to_string(&invalid_voltage)?),
        Err(EquipmentCatalogError::InvalidEntry(message)) if message.contains("electrical")
    ));

    let mut unsupported: serde_json::Value = serde_json::from_str(INVERTER_CATALOG)?;
    unsupported["schemaVersion"] = serde_json::json!(2);
    assert!(matches!(
        EquipmentCatalog::parse(&serde_json::to_string(&unsupported)?, MODULE_CATALOG),
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
