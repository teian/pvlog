use figment::{Figment, providers::Serialized};
use pvlog::config::{ConfigError, DatabaseBackend, Environment, RuntimeConfig};
use secrecy::ExposeSecret;
use serde_json::json;

fn config_from_json(value: serde_json::Value) -> RuntimeConfig {
    let figment = Figment::from(Serialized::defaults(value));
    RuntimeConfig::from_figment(&figment)
        .unwrap_or_else(|error| panic!("test configuration must be valid: {error}"))
}

#[test]
fn secrets_are_redacted_from_debug_output() {
    let config = config_from_json(json!({
        "security": {
            "session_secret": "session-secret-that-is-at-least-32-bytes",
            "credential_encryption_key": "encryption-key-that-is-at-least-32-bytes"
        }
    }));

    let debug = format!("{config:?}");

    assert!(!debug.contains("session-secret-that-is-at-least-32-bytes"));
    assert!(!debug.contains("encryption-key-that-is-at-least-32-bytes"));
    assert_eq!(
        config.security.session_secret.expose_secret(),
        "session-secret-that-is-at-least-32-bytes"
    );
}

#[test]
fn production_rejects_insecure_http_and_weak_secrets() {
    let config: RuntimeConfig = Figment::from(Serialized::defaults(json!({
        "environment": "production",
        "http": {
            "public_base_url": "http://pvlog.example",
            "secure_cookies": false
        },
        "security": {
            "session_secret": "short",
            "credential_encryption_key": "short"
        }
    })))
    .extract()
    .unwrap_or_else(|error| panic!("configuration shape must decode: {error}"));

    let issues = match config.validate() {
        Err(ConfigError::Validation(issues)) => issues,
        Ok(()) => panic!("expected validation error"),
        Err(error) => panic!("expected validation error, received: {error}"),
    };

    assert!(issues.iter().any(|issue| issue.contains("session_secret")));
    assert!(
        issues
            .iter()
            .any(|issue| issue.contains("credential_encryption_key"))
    );
    assert!(issues.iter().any(|issue| issue.contains("HTTPS")));
    assert!(issues.iter().any(|issue| issue.contains("secure_cookies")));
}

#[test]
fn production_accepts_secure_local_auth_configuration() {
    let config = config_from_json(json!({
        "environment": "production",
        "http": {
            "public_base_url": "https://pvlog.example",
            "secure_cookies": true
        },
        "security": {
            "session_secret": "session-secret-that-is-at-least-32-bytes",
            "credential_encryption_key": "encryption-key-that-is-at-least-32-bytes"
        },
        "auth": {
            "local": {
                "enabled": true,
                "allow_self_registration": false,
                "require_verified_email": true
            }
        }
    }));

    assert_eq!(config.environment, Environment::Production);
    assert_eq!(config.database.backend, DatabaseBackend::Sqlite);
}

#[test]
fn generic_oidc_connector_is_validated_without_vendor_fields() {
    let config = config_from_json(json!({
        "security": {
            "session_secret": "session-secret-that-is-at-least-32-bytes",
            "credential_encryption_key": "encryption-key-that-is-at-least-32-bytes"
        },
        "auth": {
            "local": { "enabled": false },
            "connectors": [{
                "id": "company-login",
                "display_name": "Company login",
                "protocol": "oidc",
                "enabled": true,
                "client_id": "pvlog",
                "client_secret": "connector-secret",
                "discovery_url": "https://id.example/.well-known/openid-configuration",
                "authorization_endpoint": null,
                "token_endpoint": null,
                "userinfo_endpoint": null,
                "scopes": ["openid", "profile", "email"],
                "claims": {
                    "subject": "sub",
                    "name": "name",
                    "email": "email",
                    "email_verified": "email_verified",
                    "avatar": "picture"
                }
            }]
        }
    }));

    assert_eq!(config.auth.connectors.len(), 1);
    assert_eq!(config.auth.connectors[0].id, "company-login");
}

#[test]
fn local_password_security_parameters_are_validated() {
    let config: RuntimeConfig = Figment::from(Serialized::defaults(json!({
        "security": {
            "session_secret": "session-secret-that-is-at-least-32-bytes",
            "credential_encryption_key": "encryption-key-that-is-at-least-32-bytes"
        },
        "auth": {
            "local": {
                "password_minimum_length": 7,
                "password_maximum_length": 6,
                "maximum_failed_attempts": 0,
                "lockout_seconds": 0,
                "argon2_memory_kib": 4096,
                "argon2_time_cost": 0,
                "argon2_parallelism": 0
            }
        }
    })))
    .extract()
    .unwrap_or_else(|error| panic!("configuration shape must decode: {error}"));

    let issues = match config.validate() {
        Err(ConfigError::Validation(issues)) => issues,
        Ok(()) => panic!("expected password policy validation errors"),
        Err(error) => panic!("expected validation error, received: {error}"),
    };
    assert!(issues.iter().any(|issue| issue.contains("length bounds")));
    assert!(issues.iter().any(|issue| issue.contains("brute-force")));
    assert!(issues.iter().any(|issue| issue.contains("Argon2id")));
}
