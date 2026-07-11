use std::{error::Error, path::PathBuf};

use pvlog_application::{ExternalIdentityLinkingRepository, LinkedIdentityRecord};
use pvlog_domain::{ConnectorId, ExternalIdentityId, UserId};
use pvlog_storage::{DatabaseTarget, SqliteExternalIdentityRepository, apply_migrations};
use sqlx::{Connection as _, SqliteConnection, sqlite::SqliteConnectOptions};
use tempfile::TempDir;

#[tokio::test]
async fn sqlite_external_identity_repository_enforces_connector_subject_identity()
-> Result<(), Box<dyn Error>> {
    let directory = TempDir::new()?;
    let management_path = directory.path().join("management.sqlite3");
    apply_migrations(&DatabaseTarget::Sqlite {
        management_path: management_path.clone(),
        accounts_dir: directory.path().join("accounts"),
    })
    .await?;
    let user_id = UserId::new();
    let connector_id = ConnectorId::new();
    seed_user_and_connector(&management_path, user_id, connector_id).await?;
    let repository = SqliteExternalIdentityRepository::new(management_path);
    let identity = LinkedIdentityRecord {
        id: ExternalIdentityId::new(),
        connector_id,
        subject: "subject-1".to_owned(),
        user_id,
        linked_at_epoch_millis: 1,
        last_login_at_epoch_millis: None,
    };
    repository.link(identity.clone()).await?;
    assert_eq!(
        repository
            .find_by_connector_subject(connector_id, "subject-1")
            .await?,
        Some(identity.clone())
    );
    repository.touch_login(identity.id, 2).await?;
    assert_eq!(
        repository
            .find_for_user(identity.id, user_id)
            .await?
            .map(|value| value.last_login_at_epoch_millis),
        Some(Some(2))
    );
    assert_eq!(repository.external_identity_count(user_id).await?, 1);
    assert!(!repository.has_local_login(user_id).await?);
    repository
        .audit(user_id, "external_identity.linked", 2)
        .await?;
    repository.unlink(identity.id).await?;
    assert_eq!(repository.external_identity_count(user_id).await?, 0);
    Ok(())
}

async fn seed_user_and_connector(
    path: &PathBuf,
    user_id: UserId,
    connector_id: ConnectorId,
) -> Result<(), Box<dyn Error>> {
    let mut connection = SqliteConnection::connect_with(
        &SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(false)
            .foreign_keys(true),
    )
    .await?;
    sqlx::query(
        "INSERT INTO users (id,email,display_name,status,created_at,updated_at) \
         VALUES (?,'identity@example.test','Identity','active',1,1)",
    )
    .bind(user_id.as_uuid().as_bytes().as_slice())
    .execute(&mut connection)
    .await?;
    sqlx::query(
        "INSERT INTO auth_connectors \
         (id,slug,display_name,protocol,enabled,discovery_url,client_id,client_secret_ref, \
          scopes_json,claim_mapping_json,created_at,updated_at) \
         VALUES (?,'example','Example','oidc',1,'https://issuer.example/.well-known/openid-configuration', \
                 'client','secret-ref','[\"openid\"]','{}',1,1)",
    )
    .bind(connector_id.as_uuid().as_bytes().as_slice())
    .execute(&mut connection)
    .await?;
    connection.close().await?;
    Ok(())
}
