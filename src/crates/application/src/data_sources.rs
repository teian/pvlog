//! Source profiles, uploader configuration and connection health of data sources.
//!
//! A data source is one uploader or import feeding a system. Its profile says
//! which logger it is, and from that this module renders a configuration the
//! owner can paste without editing, and judges whether readings still arrive.

use serde::Serialize;

use crate::channel_values::{DEVICE_CHANNEL_KEYS, DeviceChannelKey, SBFSPOT_CHANNEL_KEYS};

/// Kind of uploader a data source represents.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceProfile {
    /// Stable identifier stored with the data source.
    pub id: &'static str,
    /// Name shown when the catalog cannot be localized.
    pub display_name: &'static str,
    /// Whether creating a source of this profile issues an upload credential.
    pub issues_credential: bool,
    /// Kinds of configuration snippets rendered for this profile.
    pub snippet_kinds: &'static [&'static str],
    /// Channel keys a source of this profile sends, each registered with the
    /// definition given here instead of being guessed from its first value.
    pub channel_keys: &'static [DeviceChannelKey],
}

/// Profiles this instance provides, in the order they are offered.
pub const SOURCE_PROFILES: &[SourceProfile] = &[
    SourceProfile {
        id: "smalog",
        display_name: "smalog",
        issues_credential: true,
        snippet_kinds: &["smalog-config", "environment"],
        channel_keys: DEVICE_CHANNEL_KEYS,
    },
    SourceProfile {
        id: "sbfspot",
        display_name: "SBFspot",
        issues_credential: true,
        snippet_kinds: &["sbfspot-environment"],
        channel_keys: SBFSPOT_CHANNEL_KEYS,
    },
    SourceProfile {
        id: "generic",
        display_name: "Generic uploader",
        issues_credential: true,
        snippet_kinds: &["curl", "http"],
        channel_keys: &[],
    },
    SourceProfile {
        id: "import",
        display_name: "Import",
        issues_credential: false,
        snippet_kinds: &[],
        channel_keys: &[],
    },
];

/// Looks up a profile by its stored identifier.
#[must_use]
pub fn source_profile(id: &str) -> Option<&'static SourceProfile> {
    SOURCE_PROFILES.iter().find(|profile| profile.id == id)
}

/// One ready-to-paste piece of uploader configuration.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigurationSnippet {
    /// Kind of the snippet, one of the profile's `snippet_kinds`.
    pub id: &'static str,
    /// File or tool the snippet is meant for.
    pub title: &'static str,
    /// Syntax of the content, for highlighting.
    pub language: &'static str,
    pub content: String,
}

/// Renders the configuration for a profile with every value filled in.
///
/// `server_url` must not end with a slash. The key is placed only where the
/// uploader reads it; a configuration file refers to it through an environment
/// variable so the file itself can be shared.
#[must_use]
pub fn render_configuration(
    profile: &SourceProfile,
    server_url: &str,
    system_id: &str,
    api_key: &str,
) -> Vec<ConfigurationSnippet> {
    match profile.id {
        "smalog" => vec![
            ConfigurationSnippet {
                id: "smalog-config",
                title: "config.toml",
                language: "toml",
                content: format!(
                    "[pvlog]\nenabled = true\nurl = \"{server_url}\"\n\
                     system_id = \"{system_id}\"\napi_key = \"${{PVLOG_API_KEY}}\"\n"
                ),
            },
            ConfigurationSnippet {
                id: "environment",
                title: "Environment",
                language: "dotenv",
                content: format!("PVLOG_API_KEY={api_key}\n"),
            },
        ],
        "sbfspot" => vec![ConfigurationSnippet {
            id: "sbfspot-environment",
            title: "sbfspot-push environment",
            language: "dotenv",
            content: format!(
                "PVLOG_URL={server_url}\nPVLOG_SYSTEM_ID={system_id}\n\
                 PVLOG_API_KEY={api_key}\nSBFSPOT_DATABASE=/var/lib/sbfspot/SBFspot.db\n"
            ),
        }],
        "generic" => vec![
            ConfigurationSnippet {
                id: "curl",
                title: "curl",
                language: "shell",
                content: format!(
                    "curl -X POST '{server_url}/api/v1/systems/{system_id}/observations' \\\n  \
                     -H 'Authorization: Bearer {api_key}' \\\n  \
                     -H 'Content-Type: application/json' \\\n  \
                     -H \"Idempotency-Key: $(date +%s)\" \\\n  \
                     -d \"{{\\\"observedAtEpochMillis\\\": $(date +%s)000, \
                     \\\"generationPowerWatts\\\": 1200}}\"\n"
                ),
            },
            ConfigurationSnippet {
                id: "http",
                title: "HTTP request",
                language: "http",
                content: format!(
                    "POST {server_url}/api/v1/systems/{system_id}/observations\n\
                     Authorization: Bearer {api_key}\n\
                     Content-Type: application/json\n\
                     Idempotency-Key: <unique per reading>\n\n\
                     {{\"observedAtEpochMillis\": 1767268800000, \"generationPowerWatts\": 1200}}\n"
                ),
            },
        ],
        _ => Vec::new(),
    }
}

/// Whether a data source still delivers readings.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionState {
    /// No reading was ever accepted from this source.
    NeverConnected,
    /// The latest reading arrived within three status intervals.
    Receiving,
    /// Readings arrived before, but none within three status intervals.
    Stale,
    /// The source's credential was revoked or has expired.
    Disconnected,
}

/// Status intervals without a reading after which a source counts as stale.
pub const STALE_AFTER_INTERVALS: i64 = 3;

/// Derives the connection state of a data source at `now`.
///
/// A source without a usable credential is disconnected whatever it delivered
/// before, because nothing more can arrive. Times are epoch milliseconds.
#[must_use]
pub fn connection_state(
    credential_usable: bool,
    last_received_at: Option<i64>,
    status_interval_seconds: u32,
    now: i64,
) -> ConnectionState {
    if !credential_usable {
        return ConnectionState::Disconnected;
    }
    let Some(last_received_at) = last_received_at else {
        return ConnectionState::NeverConnected;
    };
    let allowed = i64::from(status_interval_seconds)
        .saturating_mul(1_000)
        .saturating_mul(STALE_AFTER_INTERVALS);
    if now.saturating_sub(last_received_at) > allowed {
        ConnectionState::Stale
    } else {
        ConnectionState::Receiving
    }
}
