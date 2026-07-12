use std::{collections::BTreeSet, error::Error, sync::Arc};

use axum::{
    Extension,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use pvlog_api::{InverterInput, RequestPrincipal, equipment_catalog_router};
use pvlog_application::{
    EquipmentCatalog, confirm_inverter_snapshot, prefill_inverter_from_catalog,
};
use pvlog_domain::{
    AccountId, ApiCredentialId, ApiScope, CatalogEntryId, EquipmentValueProvenance, UserId,
};
use tower::ServiceExt as _;

#[tokio::test]
async fn catalog_requires_authentication_and_read_scope() -> Result<(), Box<dyn Error>> {
    let catalog = Arc::new(EquipmentCatalog::bundled()?);
    let anonymous = equipment_catalog_router(catalog.clone())
        .oneshot(
            Request::builder()
                .uri("/api/v1/equipment-catalog/inverters")
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(anonymous.status(), StatusCode::FORBIDDEN);

    let insufficient = RequestPrincipal::ApiCredential {
        id: ApiCredentialId::new(),
        owner_user_id: UserId::new(),
        account_id: AccountId::new(),
        system_id: None,
        scopes: BTreeSet::new(),
    };
    let forbidden = equipment_catalog_router(catalog.clone())
        .layer(Extension(insufficient))
        .oneshot(
            Request::builder()
                .uri("/api/v1/equipment-catalog/solar-modules")
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(forbidden.status(), StatusCode::FORBIDDEN);

    let credential = RequestPrincipal::ApiCredential {
        id: ApiCredentialId::new(),
        owner_user_id: UserId::new(),
        account_id: AccountId::new(),
        system_id: None,
        scopes: BTreeSet::from([ApiScope::SystemsRead]),
    };
    let allowed = equipment_catalog_router(catalog)
        .layer(Extension(credential))
        .oneshot(
            Request::builder()
                .uri("/api/v1/equipment-catalog/inverters")
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(allowed.status(), StatusCode::OK);
    Ok(())
}

#[tokio::test]
async fn catalog_filters_pages_and_returns_typed_details() -> Result<(), Box<dyn Error>> {
    let app = equipment_catalog_router(Arc::new(EquipmentCatalog::bundled()?))
        .layer(Extension(RequestPrincipal::User(UserId::new())));
    let filtered = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(
                    "/api/v1/equipment-catalog/inverters?manufacturer=GoodWe&search=gw10&limit=500",
                )
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(filtered.status(), StatusCode::OK);
    let document: serde_json::Value =
        serde_json::from_slice(&to_bytes(filtered.into_body(), usize::MAX).await?)?;
    assert_eq!(document["revision"], "2026.07.11.1");
    assert_eq!(document["total"], 1);
    assert_eq!(document["limit"], 100);
    assert_eq!(document["items"][0]["model"], "GW10K-ET");

    let detail = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/equipment-catalog/solar-modules/ja-solar-jam54d40-450-lb")
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(detail.status(), StatusCode::OK);
    let module: serde_json::Value =
        serde_json::from_slice(&to_bytes(detail.into_body(), usize::MAX).await?)?;
    assert_eq!(module["specification"]["peakPowerWatts"], 450);
    assert_eq!(
        module["specification"]["openCircuitVoltageMillivolts"],
        39_300
    );

    let missing = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/equipment-catalog/inverters/not-listed")
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);
    Ok(())
}

#[test]
fn equipment_write_schema_accepts_manual_and_edited_prefills() -> Result<(), Box<dyn Error>> {
    let manual: InverterInput = serde_json::from_value(serde_json::json!({
        "name": "Manual inverter",
        "manufacturer": "Unlisted manufacturer",
        "model": "Site values",
        "ratedPowerWatts": 8000,
        "valueProvenance": "manual",
        "effectiveFrom": 1,
        "strings": [{
            "name": "South roof",
            "panelCount": 18,
            "panelManufacturer": "Manual modules",
            "panelModel": "M-450",
            "ratedPowerWatts": 8100,
            "effectiveFrom": 1
        }]
    }))?;
    assert!(manual.specification_snapshot.is_none());

    let catalog = EquipmentCatalog::bundled()?;
    let mut snapshot = prefill_inverter_from_catalog(
        &catalog,
        &CatalogEntryId("fronius-symo-gen24-10-0".to_owned()),
    )?;
    snapshot.model.push_str(" site setting");
    let snapshot = confirm_inverter_snapshot(&catalog, snapshot)?;
    let edited: InverterInput = serde_json::from_value(serde_json::json!({
        "name": "Catalog-assisted inverter",
        "manufacturer": snapshot.manufacturer,
        "model": snapshot.model,
        "ratedPowerWatts": 10000,
        "valueProvenance": "catalog_customized",
        "specificationSnapshot": snapshot,
        "effectiveFrom": 1,
        "strings": [{"name": "South roof", "panelCount": 18, "ratedPowerWatts": 8100, "effectiveFrom": 1}]
    }))?;
    assert_eq!(
        edited.value_provenance,
        Some(EquipmentValueProvenance::CatalogCustomized)
    );
    Ok(())
}
