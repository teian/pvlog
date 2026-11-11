//! Equipment is checked completely before anything is stored, so a request that
//! creates a system together with its inverters can be refused as a whole.

use std::{error::Error, sync::Arc};

use pvlog::inverters::ManagementInverterApi;
use pvlog_api::{InverterApiError, InverterApiUseCases as _, InverterInput};
use pvlog_application::EquipmentCatalog;
use pvlog_storage::DatabaseTarget;

fn api() -> Result<ManagementInverterApi, Box<dyn Error>> {
    // Validation never opens the database, so the paths need not exist.
    Ok(ManagementInverterApi::new(
        DatabaseTarget::Sqlite {
            management_path: "unused-management.sqlite3".into(),
            accounts_dir: "unused-accounts".into(),
        },
        Arc::new(EquipmentCatalog::bundled()?),
    ))
}

fn inverter(string: &str) -> Result<InverterInput, serde_json::Error> {
    serde_json::from_str(&format!(
        r#"{{"name":"East","effectiveFrom":0,"strings":[{string}]}}"#
    ))
}

fn rejected_field(result: &Result<(), InverterApiError>) -> Option<&'static str> {
    match *result {
        Err(InverterApiError::InvalidInput(field)) => Some(field),
        _ => None,
    }
}

#[test]
fn a_complete_inverter_passes() -> Result<(), Box<dyn Error>> {
    let input = inverter(
        r#"{"name":"S1","panelCount":10,"modulePeakPowerWatts":400,"effectiveFrom":0,
            "temperatureCoefficientPpmPerCelsius":-3500,
            "shading":[{"fromMinuteOfDay":420,"toMinuteOfDay":540,"lossBasisPoints":3000}]}"#,
    )?;
    assert!(api()?.validate(&input).is_ok());
    Ok(())
}

#[test]
fn each_invalid_string_input_is_named() -> Result<(), Box<dyn Error>> {
    let api = api()?;
    let cases = [
        (
            r#"{"name":"S1","panelCount":0,"modulePeakPowerWatts":400,"effectiveFrom":0}"#,
            "equipment",
        ),
        (
            r#"{"name":"S1","panelCount":10,"modulePeakPowerWatts":400,"effectiveFrom":0,
                "temperatureCoefficientPpmPerCelsius":10}"#,
            "temperatureCoefficientPpmPerCelsius",
        ),
        (
            r#"{"name":"S1","panelCount":10,"modulePeakPowerWatts":400,"effectiveFrom":0,
                "shading":[{"fromMinuteOfDay":540,"toMinuteOfDay":420,"lossBasisPoints":3000}]}"#,
            "shading",
        ),
        (
            r#"{"name":"S1","panelCount":10,"modulePeakPowerWatts":400,"effectiveFrom":0,
                "shading":[{"fromMinuteOfDay":0,"toMinuteOfDay":60,"lossBasisPoints":10001}]}"#,
            "shading",
        ),
    ];
    for (string, expected) in cases {
        assert_eq!(
            rejected_field(&api.validate(&inverter(string)?)),
            Some(expected),
            "{string}"
        );
    }
    Ok(())
}
