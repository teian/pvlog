//! Typed, provider-neutral runtime configuration.

use std::{fmt, net::SocketAddr, path::PathBuf};

use figment::{
    Figment,
    providers::{Env, Format, Toml},
};
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;
use thiserror::Error;
use url::Url;

/// Complete process configuration loaded from a file and environment overrides.
#[derive(Deserialize)]
#[serde(default)]
pub struct RuntimeConfig {
    /// Runtime environment and production-safety mode.
    pub environment: Environment,
    /// Public and listening HTTP settings.
    pub http: HttpConfig,
    /// Database backend and storage paths.
    pub database: DatabaseConfig,
    /// Session and encryption secrets.
    pub security: SecurityConfig,
    /// Local and external interactive authentication settings.
    pub auth: AuthConfig,
    /// OpenTelemetry export settings.
    pub telemetry: TelemetryConfig,
}

impl RuntimeConfig {
    /// Loads `pvlog.toml` when present and applies `PVLOG_*` environment overrides.
    ///
    /// Nested environment keys use a double underscore, for example
    /// `PVLOG_HTTP__PUBLIC_BASE_URL`.
    ///
    /// # Errors
    ///
    /// Returns an error when configuration cannot be decoded or fails validation.
    pub fn load() -> Result<Self, ConfigError> {
        let figment = Figment::new()
            .merge(Toml::file("pvlog.toml"))
            .merge(Env::prefixed("PVLOG_").split("__"));
        Self::from_figment(&figment)
    }

    /// Extracts and validates configuration from a prepared `Figment` source.
    ///
    /// # Errors
    ///
    /// Returns an error when the source cannot be decoded or production safeguards fail.
    pub fn from_figment(figment: &Figment) -> Result<Self, ConfigError> {
        let config: Self = figment
            .extract()
            .map_err(|error| ConfigError::Load(Box::new(error)))?;
        config.validate()?;
        Ok(config)
    }

    /// Validates cross-field invariants and fail-closed production defaults.
    ///
    /// # Errors
    ///
    /// Returns all detected configuration issues in a deterministic order.
    pub fn validate(&self) -> Result<(), ConfigError> {
        let mut issues = Vec::new();

        if self.security.session_secret.expose_secret().len() < 32 {
            issues.push("security.session_secret must contain at least 32 bytes".to_owned());
        }
        if self
            .security
            .credential_encryption_key
            .expose_secret()
            .len()
            < 32
        {
            issues.push(
                "security.credential_encryption_key must contain at least 32 bytes".to_owned(),
            );
        }
        if !self.auth.local.enabled && !self.auth.connectors.iter().any(|item| item.enabled) {
            issues
                .push("at least one interactive authentication method must be enabled".to_owned());
        }
        if self.auth.local.password_minimum_length < 8
            || self.auth.local.password_minimum_length > self.auth.local.password_maximum_length
        {
            issues.push(
                "auth.local password length bounds must satisfy 8 <= minimum <= maximum".to_owned(),
            );
        }
        if self.auth.local.maximum_failed_attempts == 0 || self.auth.local.lockout_seconds == 0 {
            issues.push(
                "auth.local brute-force controls must use non-zero attempts and lockout".to_owned(),
            );
        }
        if self.auth.local.argon2_memory_kib < 8_192
            || self.auth.local.argon2_time_cost == 0
            || self.auth.local.argon2_parallelism == 0
        {
            issues.push("auth.local Argon2id parameters are below safe minimums".to_owned());
        }

        match self.database.backend {
            DatabaseBackend::Sqlite => {
                if self.database.sqlite.management_path == self.database.sqlite.accounts_dir {
                    issues.push(
                        "database.sqlite.management_path and accounts_dir must differ".to_owned(),
                    );
                }
            }
            DatabaseBackend::Postgres => {
                let url = self.database.postgres.url.expose_secret();
                if !(url.starts_with("postgres://") || url.starts_with("postgresql://")) {
                    issues.push(
                        "database.postgres.url must use postgres:// or postgresql://".to_owned(),
                    );
                }
            }
        }

        for connector in self.auth.connectors.iter().filter(|item| item.enabled) {
            connector.validate(&mut issues);
        }

        if self.environment == Environment::Production {
            if self.http.public_base_url.scheme() != "https" {
                issues.push("http.public_base_url must use HTTPS in production".to_owned());
            }
            if !self.http.secure_cookies {
                issues.push("http.secure_cookies must be enabled in production".to_owned());
            }
            if self.auth.local.enabled
                && self.auth.local.allow_self_registration
                && !self.auth.local.require_verified_email
            {
                issues.push(
                    "local self-registration requires verified email in production".to_owned(),
                );
            }
        }

        if issues.is_empty() {
            Ok(())
        } else {
            Err(ConfigError::Validation(issues))
        }
    }
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            environment: Environment::Development,
            http: HttpConfig::default(),
            database: DatabaseConfig::default(),
            security: SecurityConfig::default(),
            auth: AuthConfig::default(),
            telemetry: TelemetryConfig::default(),
        }
    }
}

impl fmt::Debug for RuntimeConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RuntimeConfig")
            .field("environment", &self.environment)
            .field("http", &self.http)
            .field("database", &self.database)
            .field("security", &self.security)
            .field("auth", &self.auth)
            .field("telemetry", &self.telemetry)
            .finish()
    }
}

/// Deployment mode controlling safety validation.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Environment {
    /// Developer workstation defaults.
    #[default]
    Development,
    /// Automated or isolated test mode.
    Test,
    /// Fail-closed internet-facing mode.
    Production,
}

/// HTTP listener and public URL configuration.
#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct HttpConfig {
    /// Socket used by the Axum listener.
    pub bind: SocketAddr,
    /// Externally visible base URL used for callbacks and links.
    pub public_base_url: Url,
    /// Whether session cookies require HTTPS.
    pub secure_cookies: bool,
}

impl Default for HttpConfig {
    fn default() -> Self {
        let Ok(public_base_url) = Url::parse("http://localhost:8080") else {
            unreachable!("the static development URL must be valid");
        };
        Self {
            bind: SocketAddr::from(([127, 0, 0, 1], 8080)),
            public_base_url,
            secure_cookies: false,
        }
    }
}

/// Selected database engine and engine-specific settings.
#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct DatabaseConfig {
    /// Active storage adapter.
    pub backend: DatabaseBackend,
    /// `SQLite` management and account database locations.
    pub sqlite: SqliteConfig,
    /// `PostgreSQL` connection settings.
    pub postgres: PostgresConfig,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            backend: DatabaseBackend::Sqlite,
            sqlite: SqliteConfig::default(),
            postgres: PostgresConfig::default(),
        }
    }
}

/// Supported persistence adapters.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum DatabaseBackend {
    /// Management catalog plus one data file per account.
    #[default]
    Sqlite,
    /// Shared `PostgreSQL` scale profile.
    Postgres,
}

/// `SQLite` file topology.
#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct SqliteConfig {
    /// Instance-wide management database path.
    pub management_path: PathBuf,
    /// Directory containing opaque per-account database files.
    pub accounts_dir: PathBuf,
}

impl Default for SqliteConfig {
    fn default() -> Self {
        Self {
            management_path: PathBuf::from("data/management.sqlite3"),
            accounts_dir: PathBuf::from("data/accounts"),
        }
    }
}

/// `PostgreSQL` connection configuration.
#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct PostgresConfig {
    /// Secret connection URL.
    pub url: SecretString,
}

impl Default for PostgresConfig {
    fn default() -> Self {
        Self {
            url: SecretString::from(String::new()),
        }
    }
}

/// Cryptographic process secrets.
#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct SecurityConfig {
    /// Key material used to authenticate browser sessions.
    pub session_secret: SecretString,
    /// Key material used to encrypt stored connector credentials and tokens.
    pub credential_encryption_key: SecretString,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            session_secret: SecretString::from(String::new()),
            credential_encryption_key: SecretString::from(String::new()),
        }
    }
}

/// Interactive authentication methods.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct AuthConfig {
    /// Local password authentication policy.
    pub local: LocalAuthConfig,
    /// Provider-neutral external login connectors.
    pub connectors: Vec<AuthConnectorConfig>,
}

/// Local account login and registration policy.
#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct LocalAuthConfig {
    /// Enables local password authentication.
    pub enabled: bool,
    /// Allows users to register without an invitation.
    pub allow_self_registration: bool,
    /// Requires email verification before activation.
    pub require_verified_email: bool,
    /// Minimum Unicode scalar count accepted for new passwords.
    pub password_minimum_length: u16,
    /// Maximum Unicode scalar count accepted for new passwords.
    pub password_maximum_length: u16,
    /// Consecutive failed attempts before the credential is locked.
    pub maximum_failed_attempts: u16,
    /// Duration of a credential lock after the threshold is reached.
    pub lockout_seconds: u32,
    /// Lifetime of a single-use password recovery token.
    pub recovery_lifetime_seconds: u32,
    /// Argon2id memory cost in KiB.
    pub argon2_memory_kib: u32,
    /// Argon2id iteration count.
    pub argon2_time_cost: u32,
    /// Argon2id parallel lane count.
    pub argon2_parallelism: u32,
}

impl Default for LocalAuthConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            allow_self_registration: false,
            require_verified_email: true,
            password_minimum_length: 12,
            password_maximum_length: 128,
            maximum_failed_attempts: 5,
            lockout_seconds: 900,
            recovery_lifetime_seconds: 1_800,
            argon2_memory_kib: 19_456,
            argon2_time_cost: 2,
            argon2_parallelism: 1,
        }
    }
}

/// One external `OIDC` or `OAuth2` login connector.
#[derive(Debug, Deserialize)]
pub struct AuthConnectorConfig {
    /// Stable administrator-defined connector identifier.
    pub id: String,
    /// User-facing connector label.
    pub display_name: String,
    /// Standards protocol used by the connector.
    pub protocol: AuthProtocol,
    /// Whether the connector is available on the login page.
    #[serde(default)]
    pub enabled: bool,
    /// OAuth client identifier.
    pub client_id: String,
    /// Reference resolved server-side to the OAuth client secret.
    pub client_secret_ref: String,
    /// OIDC discovery URL when discovery is used.
    pub discovery_url: Option<Url>,
    /// Exact OIDC issuer identifier used for discovery and ID-token validation.
    pub issuer: Option<Url>,
    /// OAuth authorization endpoint when discovery is not used.
    pub authorization_endpoint: Option<Url>,
    /// OAuth token endpoint when discovery is not used.
    pub token_endpoint: Option<Url>,
    /// OAuth user-info endpoint for normalized identity claims.
    pub userinfo_endpoint: Option<Url>,
    /// Requested protocol scopes.
    #[serde(default)]
    pub scopes: Vec<String>,
    /// Provider-neutral claim mappings.
    pub claims: ClaimMappings,
}

impl AuthConnectorConfig {
    fn validate(&self, issues: &mut Vec<String>) {
        let prefix = format!("auth.connectors[{}]", self.id);
        if self.id.trim().is_empty() {
            issues.push("auth connector id must not be empty".to_owned());
        }
        if self.display_name.trim().is_empty() {
            issues.push(format!("{prefix}.display_name must not be empty"));
        }
        if self.client_id.trim().is_empty() {
            issues.push(format!("{prefix}.client_id must not be empty"));
        }
        if self.client_secret_ref.trim().is_empty() {
            issues.push(format!("{prefix}.client_secret_ref must not be empty"));
        }
        if self.claims.subject.trim().is_empty() {
            issues.push(format!("{prefix}.claims.subject must not be empty"));
        }

        match self.protocol {
            AuthProtocol::Oidc if self.issuer.is_none() => {
                issues.push(format!("{prefix}.issuer is required for OIDC"));
            }
            AuthProtocol::Oauth2
                if self.authorization_endpoint.is_none()
                    || self.token_endpoint.is_none()
                    || self.userinfo_endpoint.is_none() =>
            {
                issues.push(format!(
                    "{prefix} requires authorization, token, and user-info endpoints"
                ));
            }
            AuthProtocol::Oidc | AuthProtocol::Oauth2 => {}
        }
    }
}

/// Supported external authorization protocols.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum AuthProtocol {
    /// `OpenID Connect` discovery and ID token validation.
    Oidc,
    /// `OAuth2` Authorization Code flow plus user-info mapping.
    Oauth2,
}

/// Normalized external identity claim names.
#[derive(Debug, Deserialize)]
pub struct ClaimMappings {
    /// Immutable provider subject claim.
    pub subject: String,
    /// Optional display name claim.
    pub name: Option<String>,
    /// Optional email address claim.
    pub email: Option<String>,
    /// Optional verified-email boolean claim.
    pub email_verified: Option<String>,
    /// Optional avatar URL claim.
    pub avatar: Option<String>,
}

/// Browser and server telemetry export configuration.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct TelemetryConfig {
    /// Enables server-side OpenTelemetry export.
    pub enabled: bool,
    /// Provider-neutral OTLP/HTTP collector endpoint.
    pub otlp_endpoint: Option<Url>,
}

/// Runtime configuration failure.
#[derive(Debug, Error)]
pub enum ConfigError {
    /// Configuration source or decoding failure.
    #[error("failed to load configuration: {0}")]
    Load(Box<figment::Error>),
    /// One or more fail-closed validation failures.
    #[error("invalid configuration: {}", .0.join("; "))]
    Validation(Vec<String>),
}
