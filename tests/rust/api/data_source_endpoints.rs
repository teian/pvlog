//! The data source routes hand out a credential once and guard who may manage sources.

use std::{collections::BTreeSet, error::Error, sync::Arc};

use async_trait::async_trait;
use axum::{
    Extension, Router,
    body::Body,
    http::{Method, Request, StatusCode},
};
use pvlog_api::{
    AuthorizedRequest, CreateDataSourceInput, DataSourceApiError, DataSourceApiUseCases,
    DataSourceConfiguration, DataSourceResponse, IssuedDataSource, ModernRequestAuthorizer,
    RenameDataSourceInput, RequestAuthorizationError, RequestPrincipal, data_sources_router,
};
use pvlog_application::{
    ConnectionState, SOURCE_PROFILES, SourceProfile, render_configuration, source_profile,
};
use pvlog_domain::{
    AccountId, ApiCredentialId, ApiScope, DataSourceId, Permission, PrincipalId, SystemId, UserId,
};
use secrecy::SecretString;
use tower::ServiceExt as _;

const SECRET: &str = "pvlog_one-time-secret";

fn app(principal: RequestPrincipal, may_manage: bool) -> Router {
    data_sources_router(Arc::new(Stub), Arc::new(Authorizer { may_manage }))
        .layer(Extension(principal))
}

fn user() -> RequestPrincipal {
    RequestPrincipal::User(UserId::new())
}

fn api_key(scopes: &[ApiScope], system_id: Option<SystemId>) -> RequestPrincipal {
    RequestPrincipal::ApiCredential {
        id: ApiCredentialId::new(),
        owner_user_id: UserId::new(),
        account_id: AccountId::new(),
        system_id,
        scopes: scopes.iter().copied().collect::<BTreeSet<_>>(),
    }
}

async fn send(
    app: Router,
    method: Method,
    uri: String,
    body: Option<&'static str>,
) -> Result<(StatusCode, Option<String>, serde_json::Value), Box<dyn Error>> {
    let request = Request::builder().method(method).uri(uri);
    let request = match body {
        Some(body) => request
            .header("content-type", "application/json")
            .body(Body::from(body))?,
        None => request.body(Body::empty())?,
    };
    let response = app.oneshot(request).await?;
    let status = response.status();
    let cache = response
        .headers()
        .get("cache-control")
        .and_then(|value| value.to_str().ok())
        .map(ToOwned::to_owned);
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024).await?;
    let json = if bytes.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_slice(&bytes)?
    };
    Ok((status, cache, json))
}

#[tokio::test]
async fn the_credential_is_returned_once_and_never_cached() -> Result<(), Box<dyn Error>> {
    let system = SystemId::new();
    let collection = format!("/api/v1/systems/{system}/data-sources");

    let (status, cache, body) = send(
        app(user(), true),
        Method::POST,
        collection.clone(),
        Some(r#"{"name":"Roof","profile":"smalog"}"#),
    )
    .await?;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(cache.as_deref(), Some("no-store"));
    assert_eq!(body["configuration"]["apiKey"], SECRET);
    assert_eq!(body["configuration"]["systemId"], system.to_string());
    assert_eq!(body["configuration"]["serverUrl"], "https://pvlog.example");
    assert_eq!(body["configuration"]["snippets"][0]["id"], "smalog-config");
    assert_eq!(body["dataSource"]["state"], "never_connected");
    let id = body["dataSource"]["id"]
        .as_str()
        .ok_or("data source id missing")?
        .to_owned();

    // Reading the source again shows where to send, not the key.
    for uri in [collection.clone(), format!("{collection}/{id}")] {
        let (status, _, body) = send(app(user(), false), Method::GET, uri, None).await?;
        assert_eq!(status, StatusCode::OK);
        let text = body.to_string();
        assert!(!text.contains(SECRET), "{text}");
        assert!(!text.contains("apiKey"), "{text}");
        assert!(text.contains("https://pvlog.example"));
    }

    let (status, cache, body) = send(
        app(user(), true),
        Method::POST,
        format!("{collection}/{id}/credential"),
        None,
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(cache.as_deref(), Some("no-store"));
    assert_eq!(body["configuration"]["apiKey"], SECRET);

    let (status, _, body) = send(
        app(user(), true),
        Method::POST,
        collection,
        Some(r#"{"name":"Roof","profile":"solar-wizard"}"#),
    )
    .await?;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["detail"], "profile");
    Ok(())
}

#[tokio::test]
async fn only_principals_that_manage_the_system_change_data_sources() -> Result<(), Box<dyn Error>>
{
    let system = SystemId::new();
    let collection = format!("/api/v1/systems/{system}/data-sources");
    let create = Some(r#"{"name":"Roof","profile":"generic"}"#);
    let one = format!("{collection}/{}", DataSourceId::new());

    let refused = [
        // A user who may only read the system.
        (user(), false),
        // An upload key, restricted to this system or not, manages nothing.
        (api_key(&[ApiScope::TelemetryWrite], Some(system)), true),
        (api_key(&[ApiScope::TelemetryWrite], None), true),
        // A key bound to one system must not mint further keys for it.
        (api_key(&[ApiScope::SystemsWrite], Some(system)), true),
        // Reading systems does not include changing them.
        (api_key(&[ApiScope::SystemsRead], None), true),
    ];
    for (principal, may_manage) in refused {
        for (method, uri, body) in [
            (Method::POST, collection.clone(), create),
            (Method::POST, format!("{one}/credential"), None),
            (Method::PUT, one.clone(), Some(r#"{"name":"Other"}"#)),
            (Method::DELETE, one.clone(), None),
        ] {
            let (status, _, _) = send(
                app(principal.clone(), may_manage),
                method.clone(),
                uri,
                body,
            )
            .await?;
            assert_eq!(status, StatusCode::FORBIDDEN, "{method} as {principal:?}");
        }
    }

    // An upload key cannot even look at the sources.
    let (status, _, _) = send(
        app(api_key(&[ApiScope::TelemetryWrite], Some(system)), true),
        Method::GET,
        collection.clone(),
        None,
    )
    .await?;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // An unrestricted management key may, and a reader may list.
    let (status, _, _) = send(
        app(api_key(&[ApiScope::SystemsWrite], None), true),
        Method::POST,
        collection.clone(),
        create,
    )
    .await?;
    assert_eq!(status, StatusCode::CREATED);
    let (status, _, _) = send(
        app(api_key(&[ApiScope::SystemsRead], None), false),
        Method::GET,
        collection,
        None,
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    Ok(())
}

#[tokio::test]
async fn the_profile_catalog_needs_a_signed_in_client() -> Result<(), Box<dyn Error>> {
    let (status, _, body) = send(
        app(user(), false),
        Method::GET,
        "/api/v1/data-source-profiles".to_owned(),
        None,
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body[0]["id"], "smalog");
    assert_eq!(body[0]["issuesCredential"], true);
    assert_eq!(body[0]["snippetKinds"][0], "smalog-config");

    let anonymous = data_sources_router(Arc::new(Stub), Arc::new(Authorizer { may_manage: true }));
    let (status, _, _) = send(
        anonymous,
        Method::GET,
        "/api/v1/data-source-profiles".to_owned(),
        None,
    )
    .await?;
    assert_eq!(status, StatusCode::FORBIDDEN);
    Ok(())
}

/// Grants reading to everyone and management only when told to.
struct Authorizer {
    may_manage: bool,
}

#[async_trait]
impl ModernRequestAuthorizer for Authorizer {
    async fn authorize_instance(
        &self,
        _principal: PrincipalId,
        _permission: Permission,
        _action: &'static str,
    ) -> Result<UserId, RequestAuthorizationError> {
        Err(RequestAuthorizationError::Forbidden)
    }

    async fn authorize_account(
        &self,
        _principal: PrincipalId,
        _account_id: AccountId,
        _permission: Permission,
        _action: &'static str,
    ) -> Result<AuthorizedRequest, RequestAuthorizationError> {
        Err(RequestAuthorizationError::Forbidden)
    }

    async fn authorize_system(
        &self,
        _principal: PrincipalId,
        _system_id: SystemId,
        permission: Permission,
        _action: &'static str,
    ) -> Result<AuthorizedRequest, RequestAuthorizationError> {
        if permission == Permission::SystemManage && !self.may_manage {
            return Err(RequestAuthorizationError::Forbidden);
        }
        Ok(AuthorizedRequest {
            actor_user_id: UserId::new(),
            account_id: AccountId::new(),
        })
    }
}

struct Stub;

fn source(system_id: SystemId, id: DataSourceId, name: &str, profile: &str) -> DataSourceResponse {
    DataSourceResponse {
        id,
        system_id,
        name: name.to_owned(),
        profile: profile.to_owned(),
        state: ConnectionState::NeverConnected,
        server_url: "https://pvlog.example".to_owned(),
        credential_id: Some(ApiCredentialId::new()),
        last_reading_at_epoch_millis: None,
        last_received_at_epoch_millis: None,
        readings_last_day: 0,
        created_at_epoch_millis: 1,
        updated_at_epoch_millis: 1,
    }
}

fn issued(data_source: DataSourceResponse, profile: &SourceProfile) -> IssuedDataSource {
    let system_id = data_source.system_id;
    IssuedDataSource {
        data_source,
        configuration: Some(DataSourceConfiguration {
            server_url: "https://pvlog.example".to_owned(),
            system_id,
            api_key: SecretString::from(SECRET),
            snippets: render_configuration(
                profile,
                "https://pvlog.example",
                &system_id.to_string(),
                SECRET,
            ),
        }),
    }
}

#[async_trait]
impl DataSourceApiUseCases for Stub {
    fn profiles(&self) -> Vec<SourceProfile> {
        SOURCE_PROFILES.to_vec()
    }

    async fn list(
        &self,
        _account_id: AccountId,
        system_id: SystemId,
    ) -> Result<Vec<DataSourceResponse>, DataSourceApiError> {
        Ok(vec![source(
            system_id,
            DataSourceId::new(),
            "Roof",
            "smalog",
        )])
    }

    async fn read(
        &self,
        _account_id: AccountId,
        system_id: SystemId,
        id: DataSourceId,
    ) -> Result<DataSourceResponse, DataSourceApiError> {
        Ok(source(system_id, id, "Roof", "smalog"))
    }

    async fn create(
        &self,
        _actor: UserId,
        _account_id: AccountId,
        system_id: SystemId,
        input: CreateDataSourceInput,
    ) -> Result<IssuedDataSource, DataSourceApiError> {
        let profile =
            source_profile(&input.profile).ok_or(DataSourceApiError::InvalidInput("profile"))?;
        Ok(issued(
            source(system_id, DataSourceId::new(), &input.name, profile.id),
            profile,
        ))
    }

    async fn rename(
        &self,
        _actor: UserId,
        _account_id: AccountId,
        system_id: SystemId,
        id: DataSourceId,
        input: RenameDataSourceInput,
    ) -> Result<DataSourceResponse, DataSourceApiError> {
        Ok(source(system_id, id, &input.name, "smalog"))
    }

    async fn regenerate(
        &self,
        _actor: UserId,
        _account_id: AccountId,
        system_id: SystemId,
        id: DataSourceId,
    ) -> Result<IssuedDataSource, DataSourceApiError> {
        Ok(issued(
            source(system_id, id, "Roof", "smalog"),
            &SOURCE_PROFILES[0],
        ))
    }

    async fn delete(
        &self,
        _actor: UserId,
        _account_id: AccountId,
        _system_id: SystemId,
        _id: DataSourceId,
    ) -> Result<(), DataSourceApiError> {
        Ok(())
    }
}
