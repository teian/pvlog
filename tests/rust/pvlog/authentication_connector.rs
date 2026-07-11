use pvlog::authentication::ManagementConnectorApi;
use pvlog::config::{AuthConnectorConfig, AuthProtocol, ClaimMappings};
use pvlog_api::ConnectorAdminUseCases as _;
use url::Url;

#[tokio::test]
async fn connector_catalog_never_serializes_client_credentials_or_secret_references()
-> Result<(), Box<dyn std::error::Error>> {
    let connector = AuthConnectorConfig {
        id: "company-sso".to_owned(),
        display_name: "Company SSO".to_owned(),
        protocol: AuthProtocol::Oidc,
        enabled: true,
        client_id: "client-id-must-not-leak".to_owned(),
        client_secret_ref: "secret-ref-must-not-leak".to_owned(),
        discovery_url: Some(Url::parse(
            "https://identity.example/.well-known/openid-configuration",
        )?),
        issuer: Some(Url::parse("https://identity.example")?),
        authorization_endpoint: Some(Url::parse("https://identity.example/authorize")?),
        token_endpoint: Some(Url::parse("https://identity.example/token")?),
        userinfo_endpoint: Some(Url::parse("https://identity.example/userinfo")?),
        scopes: vec!["openid".to_owned()],
        claims: ClaimMappings {
            subject: "sub".to_owned(),
            name: None,
            email: None,
            email_verified: None,
            avatar: None,
        },
    };
    let response = ManagementConnectorApi::new(&[connector]).connectors().await;
    assert!(response.is_ok());
    let serialized = serde_json::to_string(&response.unwrap_or_default())?;
    assert!(!serialized.contains("client-id-must-not-leak"));
    assert!(!serialized.contains("secret-ref-must-not-leak"));
    assert!(!serialized.contains("https://identity.example/token"));
    assert!(!serialized.contains("https://identity.example/userinfo"));
    assert!(serialized.contains("https://identity.example/authorize"));
    Ok(())
}
