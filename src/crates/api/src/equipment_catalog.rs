//! Authenticated read-only access to the bundled equipment catalog.

use std::sync::Arc;

use axum::{
    Extension, Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
};
use pvlog_application::{EquipmentCatalog, EquipmentCatalogQuery};
use pvlog_domain::{
    ApiScope, CatalogEntryId, CatalogRevision, InverterCatalogEntry, SolarModuleCatalogEntry,
};
use serde::{Deserialize, Serialize};

use crate::RequestPrincipal;

#[derive(Clone)]
struct EquipmentCatalogState {
    catalog: Arc<EquipmentCatalog>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CatalogListQuery {
    search: Option<String>,
    manufacturer: Option<String>,
    #[serde(default)]
    offset: usize,
    #[serde(default = "default_limit")]
    limit: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogPageResponse<T> {
    pub revision: CatalogRevision,
    pub total: usize,
    pub offset: usize,
    pub limit: usize,
    pub items: Vec<T>,
}

pub fn equipment_catalog_router(catalog: Arc<EquipmentCatalog>) -> Router {
    Router::new()
        .route("/api/v1/equipment-catalog/inverters", get(list_inverters))
        .route(
            "/api/v1/equipment-catalog/inverters/{entry_id}",
            get(inverter_detail),
        )
        .route(
            "/api/v1/equipment-catalog/solar-modules",
            get(list_solar_modules),
        )
        .route(
            "/api/v1/equipment-catalog/solar-modules/{entry_id}",
            get(solar_module_detail),
        )
        .with_state(EquipmentCatalogState { catalog })
}

async fn list_inverters(
    State(state): State<EquipmentCatalogState>,
    principal: Option<Extension<RequestPrincipal>>,
    Query(query): Query<CatalogListQuery>,
) -> Result<Json<CatalogPageResponse<InverterCatalogEntry>>, EquipmentCatalogApiError> {
    authorize(principal)?;
    let page = state.catalog.inverters(&query.into());
    Ok(Json(CatalogPageResponse {
        revision: page.revision,
        total: page.total,
        offset: page.offset,
        limit: page.limit,
        items: page.items,
    }))
}

async fn inverter_detail(
    State(state): State<EquipmentCatalogState>,
    principal: Option<Extension<RequestPrincipal>>,
    Path(entry_id): Path<String>,
) -> Result<Json<InverterCatalogEntry>, EquipmentCatalogApiError> {
    authorize(principal)?;
    state
        .catalog
        .inverter(&CatalogEntryId(entry_id))
        .cloned()
        .map(Json)
        .ok_or(EquipmentCatalogApiError::NotFound)
}

async fn list_solar_modules(
    State(state): State<EquipmentCatalogState>,
    principal: Option<Extension<RequestPrincipal>>,
    Query(query): Query<CatalogListQuery>,
) -> Result<Json<CatalogPageResponse<SolarModuleCatalogEntry>>, EquipmentCatalogApiError> {
    authorize(principal)?;
    let page = state.catalog.solar_modules(&query.into());
    Ok(Json(CatalogPageResponse {
        revision: page.revision,
        total: page.total,
        offset: page.offset,
        limit: page.limit,
        items: page.items,
    }))
}

async fn solar_module_detail(
    State(state): State<EquipmentCatalogState>,
    principal: Option<Extension<RequestPrincipal>>,
    Path(entry_id): Path<String>,
) -> Result<Json<SolarModuleCatalogEntry>, EquipmentCatalogApiError> {
    authorize(principal)?;
    state
        .catalog
        .solar_module(&CatalogEntryId(entry_id))
        .cloned()
        .map(Json)
        .ok_or(EquipmentCatalogApiError::NotFound)
}

fn authorize(
    principal: Option<Extension<RequestPrincipal>>,
) -> Result<(), EquipmentCatalogApiError> {
    let Extension(principal) = principal.ok_or(EquipmentCatalogApiError::Forbidden)?;
    match principal {
        RequestPrincipal::User(_) => Ok(()),
        RequestPrincipal::ApiCredential { scopes, .. }
            if scopes.contains(&ApiScope::SystemsRead) =>
        {
            Ok(())
        }
        RequestPrincipal::ApiCredential { .. } => Err(EquipmentCatalogApiError::Forbidden),
    }
}

impl From<CatalogListQuery> for EquipmentCatalogQuery {
    fn from(value: CatalogListQuery) -> Self {
        Self {
            search: value.search,
            manufacturer: value.manufacturer,
            offset: value.offset,
            limit: value.limit,
        }
    }
}

const fn default_limit() -> usize {
    25
}

#[derive(Debug)]
pub enum EquipmentCatalogApiError {
    Forbidden,
    NotFound,
}

impl IntoResponse for EquipmentCatalogApiError {
    fn into_response(self) -> Response {
        match self {
            Self::Forbidden => StatusCode::FORBIDDEN,
            Self::NotFound => StatusCode::NOT_FOUND,
        }
        .into_response()
    }
}
