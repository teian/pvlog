use std::{error::Error, sync::Arc};

use async_trait::async_trait;
use axum::{
    Extension,
    body::Body,
    http::{Request, StatusCode},
};
use pvlog_api::{
    IdentityApiError, IdentityApiUseCases, LinkedIdentityResponse, RequestPrincipal,
    identities_router,
};
use pvlog_domain::{ConnectorId, ExternalIdentityId, UserId};
use tower::ServiceExt as _;

#[tokio::test]
async fn linked_identities_require_a_browser_session() -> Result<(), Box<dyn Error>> {
    let app = identities_router(Arc::new(Identities));
    let denied = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/users/me/identities")
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(denied.status(), StatusCode::FORBIDDEN);
    let allowed = app
        .layer(Extension(RequestPrincipal::User(UserId::new())))
        .oneshot(
            Request::builder()
                .uri("/api/v1/users/me/identities")
                .body(Body::empty())?,
        )
        .await?;
    assert_eq!(allowed.status(), StatusCode::OK);
    Ok(())
}

struct Identities;
#[async_trait]
impl IdentityApiUseCases for Identities {
    async fn list_identities(
        &self,
        _user_id: UserId,
    ) -> Result<Vec<LinkedIdentityResponse>, IdentityApiError> {
        Ok(vec![LinkedIdentityResponse {
            id: ExternalIdentityId::new(),
            connector_id: ConnectorId::new(),
            subject: "subject".to_owned(),
            linked_at_epoch_millis: 1,
            last_login_at_epoch_millis: None,
        }])
    }
}
