//! The history import routes take a JSON upload, plain or gzip-compressed,
//! name the limit of a file that is too large, and let only principals that
//! manage the system upload and commit.

use std::{collections::BTreeSet, error::Error, io::Write as _, sync::Arc};

use async_trait::async_trait;
use axum::{
    Extension, Router,
    body::Body,
    http::{Method, Request, StatusCode},
};
use flate2::{Compression, write::GzEncoder};
use pvlog_api::{
    AuthorizedRequest, HistoryImportApiError, HistoryImportApiUseCases, HistoryImportReport,
    HistoryImportResponse, HistoryImportUpload, ImportOverlap, MAXIMUM_IMPORT_FILE_BYTES,
    ModernRequestAuthorizer, RequestAuthorizationError, RequestPrincipal, history_imports_router,
};
use pvlog_domain::{
    AccountId, ApiCredentialId, ApiScope, DataSourceId, Permission, PrincipalId, SystemId, UserId,
};
use serde_json::json;
use tower::ServiceExt as _;
use uuid::Uuid;

fn app(principal: RequestPrincipal, may_manage: bool) -> Router {
    history_imports_router(Arc::new(Stub), Arc::new(Authorizer { may_manage }))
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

fn collection() -> String {
    format!(
        "/api/v1/systems/{}/data-sources/{}/imports",
        SystemId::new(),
        DataSourceId::new()
    )
}

fn upload(content: &str) -> Result<Vec<u8>, Box<dyn Error>> {
    Ok(serde_json::to_vec(
        &json!({ "fileName": " status.csv ", "content": content }),
    )?)
}

fn gzip(bytes: &[u8]) -> Result<Vec<u8>, Box<dyn Error>> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(bytes)?;
    Ok(encoder.finish()?)
}

async fn send(
    app: Router,
    method: Method,
    uri: String,
    body: Option<(Vec<u8>, Option<&str>)>,
) -> Result<(StatusCode, serde_json::Value), Box<dyn Error>> {
    let request = Request::builder().method(method).uri(uri);
    let request = match body {
        Some((body, encoding)) => {
            let request = request.header("content-type", "application/json");
            let request = match encoding {
                Some(encoding) => request.header("content-encoding", encoding),
                None => request,
            };
            request.body(Body::from(body))?
        }
        None => request.body(Body::empty())?,
    };
    let response = app.oneshot(request).await?;
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024).await?;
    let json = if bytes.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_slice(&bytes)?
    };
    Ok((status, json))
}

#[tokio::test]
async fn a_file_is_uploaded_plain_or_compressed() -> Result<(), Box<dyn Error>> {
    let content = "20270115,08:00,5,0,100\n20270115,08:05,10,0,120";
    for (body, encoding) in [
        (upload(content)?, None),
        (gzip(&upload(content)?)?, Some("gzip")),
    ] {
        let (status, body) = send(
            app(user(), true),
            Method::POST,
            collection(),
            Some((body, encoding)),
        )
        .await?;
        assert_eq!(status, StatusCode::CREATED);
        // The stub reports the trimmed file name and counts the lines.
        assert_eq!(body["fileName"], "status.csv");
        assert_eq!(body["report"]["rows"], 2);
        assert_eq!(body["state"], "ready");
        assert!(body.get("result").is_none());
    }

    let (status, body) = send(
        app(user(), true),
        Method::POST,
        collection(),
        Some((upload(content)?, Some("br"))),
    )
    .await?;
    assert_eq!(status, StatusCode::UNSUPPORTED_MEDIA_TYPE);
    assert_eq!(body["title"], "unsupported_content_encoding");

    for body in [
        b"not json".to_vec(),
        serde_json::to_vec(&json!({ "fileName": "  ", "content": content }))?,
    ] {
        let (status, _) = send(
            app(user(), true),
            Method::POST,
            collection(),
            Some((body, None)),
        )
        .await?;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    }
    Ok(())
}

#[tokio::test]
async fn a_file_over_the_limit_names_the_limit() -> Result<(), Box<dyn Error>> {
    let content = "1".repeat(MAXIMUM_IMPORT_FILE_BYTES + 1);
    // Plain, the file is refused; compressed, it is refused after unpacking.
    for (body, encoding) in [
        (upload(&content)?, None),
        (gzip(&upload(&content)?)?, Some("gzip")),
        // A body far over the limit is not read to its end.
        (upload(&"1".repeat(MAXIMUM_IMPORT_FILE_BYTES * 2))?, None),
    ] {
        let (status, body) = send(
            app(user(), true),
            Method::POST,
            collection(),
            Some((body, encoding)),
        )
        .await?;
        assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
        assert_eq!(body["title"], "file_too_large");
        assert_eq!(body["limit"], MAXIMUM_IMPORT_FILE_BYTES);
        assert!(
            body["detail"]
                .as_str()
                .is_some_and(|detail| detail.contains(&MAXIMUM_IMPORT_FILE_BYTES.to_string()))
        );
    }
    Ok(())
}

#[tokio::test]
async fn only_principals_that_manage_the_system_import() -> Result<(), Box<dyn Error>> {
    let system = SystemId::new();
    let collection = format!(
        "/api/v1/systems/{system}/data-sources/{}/imports",
        DataSourceId::new()
    );
    let commit = format!("{collection}/{}/commit", Uuid::now_v7());
    let refused = [
        (user(), false),
        (api_key(&[ApiScope::TelemetryWrite], None), true),
        (api_key(&[ApiScope::SystemsWrite], Some(system)), true),
        (api_key(&[ApiScope::SystemsRead], None), true),
    ];
    for (principal, may_manage) in refused {
        for (uri, body) in [
            (collection.clone(), Some((upload("x")?, None))),
            (commit.clone(), None),
        ] {
            let (status, _) =
                send(app(principal.clone(), may_manage), Method::POST, uri, body).await?;
            assert_eq!(status, StatusCode::FORBIDDEN, "{principal:?}");
        }
    }

    // A reader may look at the imports; a manager may commit.
    let (status, body) = send(
        app(api_key(&[ApiScope::SystemsRead], None), false),
        Method::GET,
        collection.clone(),
        None,
    )
    .await?;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!([]));
    let (status, _) = send(
        app(api_key(&[ApiScope::SystemsWrite], None), true),
        Method::POST,
        commit,
        None,
    )
    .await?;
    assert_eq!(status, StatusCode::NOT_FOUND);
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

/// Answers a dry run with the file name and the number of lines; no import
/// exists to be committed or read.
struct Stub;

#[async_trait]
impl HistoryImportApiUseCases for Stub {
    async fn dry_run(
        &self,
        _system_id: SystemId,
        data_source_id: DataSourceId,
        _requested_by: UserId,
        upload: HistoryImportUpload,
    ) -> Result<HistoryImportResponse, HistoryImportApiError> {
        Ok(HistoryImportResponse {
            id: Uuid::now_v7(),
            data_source_id: data_source_id.as_uuid(),
            file_name: upload.file_name,
            kind: "status".to_owned(),
            state: "ready".to_owned(),
            created_at_epoch_millis: 0,
            expires_at_epoch_millis: Some(86_400_000),
            committed_at_epoch_millis: None,
            report: HistoryImportReport {
                rows: u64::try_from(upload.content.lines().count()).unwrap_or(u64::MAX),
                accepted: 0,
                rejected_count: 0,
                rejected: Vec::new(),
                first_date: None,
                last_date: None,
                overlap: ImportOverlap::default(),
            },
            result: None,
        })
    }

    async fn commit(
        &self,
        _system_id: SystemId,
        _data_source_id: DataSourceId,
        _import_id: Uuid,
    ) -> Result<HistoryImportResponse, HistoryImportApiError> {
        Err(HistoryImportApiError::NotFound)
    }

    async fn list(
        &self,
        _system_id: SystemId,
        _data_source_id: DataSourceId,
    ) -> Result<Vec<HistoryImportResponse>, HistoryImportApiError> {
        Ok(Vec::new())
    }

    async fn get(
        &self,
        _system_id: SystemId,
        _data_source_id: DataSourceId,
        _import_id: Uuid,
    ) -> Result<HistoryImportResponse, HistoryImportApiError> {
        Err(HistoryImportApiError::NotFound)
    }
}
