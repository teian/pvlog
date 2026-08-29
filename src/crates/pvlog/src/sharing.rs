//! Storage-backed public sharing.
//!
//! The token lives in the management registry next to the routing entry, so an
//! anonymous request can be resolved to an account without first knowing one.
//! What comes back is deliberately narrow: the response types in the API layer
//! have no place for serial numbers, channel keys, or people.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate};
use chrono_tz::Tz;
use pvlog_api::{
    PublicDirectoryEntry, PublicSystem, SharingApiError, SharingApiUseCases, SharingInput,
    SharingState,
};
use pvlog_application::Clock;
use pvlog_domain::{AccountId, SystemId};
use pvlog_storage::DatabaseTarget;
#[cfg(feature = "sqlite")]
use sqlx::{ConnectOptions as _, Row};

/// Bytes of randomness behind a sharing token.
const TOKEN_BYTES: usize = 16;

/// Serves sharing administration and the two anonymous read endpoints.
pub struct StorageSharingApi {
    target: DatabaseTarget,
    clock: Arc<dyn Clock>,
}

impl StorageSharingApi {
    #[must_use]
    pub const fn new(target: DatabaseTarget, clock: Arc<dyn Clock>) -> Self {
        Self { target, clock }
    }

    #[cfg(feature = "sqlite")]
    fn paths(&self) -> Result<(&std::path::Path, &std::path::Path), SharingApiError> {
        match &self.target {
            DatabaseTarget::Sqlite {
                management_path,
                accounts_dir,
            } => Ok((management_path, accounts_dir)),
            DatabaseTarget::Postgres { .. } => Err(SharingApiError::Unavailable),
        }
    }

    #[cfg(feature = "sqlite")]
    async fn management(&self) -> Result<sqlx::SqliteConnection, SharingApiError> {
        let (management_path, _) = self.paths()?;
        sqlx::sqlite::SqliteConnectOptions::new()
            .filename(management_path)
            .create_if_missing(false)
            .connect()
            .await
            .map_err(|_| SharingApiError::Unavailable)
    }

    #[cfg(feature = "sqlite")]
    async fn account(
        &self,
        account_id: AccountId,
    ) -> Result<pvlog_storage::RoutedSqliteAccount, SharingApiError> {
        let (management_path, accounts_dir) = self.paths()?;
        pvlog_storage::SqliteAccountPoolRouter::new(
            management_path.to_path_buf(),
            accounts_dir.to_path_buf(),
            pvlog_storage::SqliteAccountPoolConfig::default(),
        )
        .map_err(|_| SharingApiError::Unavailable)?
        .route(account_id)
        .await
        .map_err(|_| SharingApiError::Unavailable)
    }
}

#[async_trait]
impl SharingApiUseCases for StorageSharingApi {
    async fn state(&self, system_id: SystemId) -> Result<SharingState, SharingApiError> {
        #[cfg(feature = "sqlite")]
        {
            let mut management = self.management().await?;
            let registry = read_registry(&mut management, system_id).await?;
            let account = self.account(registry.account_id).await?;
            let mut connection = account
                .acquire()
                .await
                .map_err(|_| SharingApiError::Unavailable)?;
            let visibility: String =
                sqlx::query_scalar("SELECT visibility FROM systems WHERE id=?")
                    .bind(id_blob(system_id))
                    .fetch_optional(&mut *connection)
                    .await
                    .map_err(|_| SharingApiError::Unavailable)?
                    .unwrap_or_else(|| "private".to_owned());
            return Ok(SharingState {
                system_id,
                link_enabled: registry.link_enabled,
                public_token: registry.public_token,
                visibility,
            });
        }
        #[cfg(not(feature = "sqlite"))]
        {
            let _ = system_id;
            Err(SharingApiError::Unavailable)
        }
    }

    async fn update(
        &self,
        _actor: pvlog_domain::UserId,
        system_id: SystemId,
        input: SharingInput,
    ) -> Result<SharingState, SharingApiError> {
        #[cfg(feature = "sqlite")]
        {
            let mut management = self.management().await?;
            let registry = read_registry(&mut management, system_id).await?;
            // A link that has never been issued gets one the moment it is
            // enabled; rotating replaces it, which revokes every link handed out.
            let token =
                if input.rotate_token || (input.link_enabled && registry.public_token.is_none()) {
                    Some(generate_token())
                } else {
                    registry.public_token.clone()
                };
            let now = i64::try_from(self.clock.now().epoch_millis()).unwrap_or_default();
            sqlx::query(
                "UPDATE system_registry SET public_token=?,link_enabled=?,updated_at=? \
                 WHERE system_id=?",
            )
            .bind(token.as_deref())
            .bind(i64::from(input.link_enabled))
            .bind(now)
            .bind(id_blob(system_id))
            .execute(&mut management)
            .await
            .map_err(|_| SharingApiError::Unavailable)?;

            if let Some(visibility) = input.visibility.as_deref() {
                let account = self.account(registry.account_id).await?;
                let mut writer = account
                    .acquire_writer()
                    .await
                    .map_err(|_| SharingApiError::Unavailable)?;
                sqlx::query(
                    "UPDATE systems SET visibility=?,updated_at=?,version=version+1 WHERE id=?",
                )
                .bind(visibility)
                .bind(now)
                .bind(id_blob(system_id))
                .execute(writer.connection())
                .await
                .map_err(|_| SharingApiError::Unavailable)?;
            }
            return self.state(system_id).await;
        }
        #[cfg(not(feature = "sqlite"))]
        {
            let _ = (system_id, input);
            Err(SharingApiError::Unavailable)
        }
    }

    async fn public_system(&self, token: &str) -> Result<PublicSystem, SharingApiError> {
        #[cfg(feature = "sqlite")]
        {
            let mut management = self.management().await?;
            let row = sqlx::query(
                "SELECT system_id,account_id FROM system_registry \
                 WHERE public_token=? AND link_enabled=1",
            )
            .bind(token)
            .fetch_optional(&mut management)
            .await
            .map_err(|_| SharingApiError::Unavailable)?
            .ok_or(SharingApiError::NotFound)?;
            let system_id = system_uuid(&row, "system_id")?;
            let account_id = account_uuid(&row, "account_id")?;
            return self.public_view(account_id, system_id).await;
        }
        #[cfg(not(feature = "sqlite"))]
        {
            let _ = token;
            Err(SharingApiError::Unavailable)
        }
    }

    async fn public_directory(
        &self,
        limit: u32,
    ) -> Result<Vec<PublicDirectoryEntry>, SharingApiError> {
        #[cfg(feature = "sqlite")]
        {
            let mut management = self.management().await?;
            let rows = sqlx::query(
                "SELECT system_id,account_id,public_token FROM system_registry \
                 WHERE link_enabled=1 AND public_token IS NOT NULL ORDER BY system_id LIMIT ?",
            )
            .bind(i64::from(limit))
            .fetch_all(&mut management)
            .await
            .map_err(|_| SharingApiError::Unavailable)?;
            let mut entries = Vec::new();
            for row in &rows {
                let system_id = system_uuid(row, "system_id")?;
                let account_id = account_uuid(row, "account_id")?;
                let view = self.public_view(account_id, system_id).await?;
                // The directory lists only what the owner marked public;
                // an unlisted system stays reachable by link alone.
                if !self.is_public(account_id, system_id).await? {
                    continue;
                }
                entries.push(PublicDirectoryEntry {
                    name: view.name,
                    location: view.location,
                    capacity_watts: view.capacity_watts,
                    public_token: row.get("public_token"),
                });
            }
            return Ok(entries);
        }
        #[cfg(not(feature = "sqlite"))]
        {
            let _ = limit;
            Err(SharingApiError::Unavailable)
        }
    }
}

impl StorageSharingApi {
    #[cfg(feature = "sqlite")]
    async fn is_public(
        &self,
        account_id: AccountId,
        system_id: SystemId,
    ) -> Result<bool, SharingApiError> {
        let account = self.account(account_id).await?;
        let mut connection = account
            .acquire()
            .await
            .map_err(|_| SharingApiError::Unavailable)?;
        let visibility: Option<String> =
            sqlx::query_scalar("SELECT visibility FROM systems WHERE id=?")
                .bind(id_blob(system_id))
                .fetch_optional(&mut *connection)
                .await
                .map_err(|_| SharingApiError::Unavailable)?;
        Ok(visibility.as_deref() == Some("public"))
    }

    #[cfg(feature = "sqlite")]
    async fn public_view(
        &self,
        account_id: AccountId,
        system_id: SystemId,
    ) -> Result<PublicSystem, SharingApiError> {
        let account = self.account(account_id).await?;
        let mut connection = account
            .acquire()
            .await
            .map_err(|_| SharingApiError::Unavailable)?;
        let row = sqlx::query(
            "SELECT s.name,s.timezone,s.description,s.commissioning_date,s.location_precision,\
             s.country_code,(SELECT SUM(p.rated_power_watts) FROM pv_strings p \
              JOIN inverters i ON i.id=p.inverter_id WHERE i.system_id=s.id) AS capacity_watts,\
             (SELECT generation_energy_wh FROM system_lifetime_summaries l WHERE l.system_id=s.id) \
              AS lifetime_wh,\
             (SELECT MAX(measured_at) FROM telemetry_hot h WHERE h.system_id=s.id) AS last_seen \
             FROM systems s WHERE s.id=?",
        )
        .bind(id_blob(system_id))
        .fetch_optional(&mut *connection)
        .await
        .map_err(|_| SharingApiError::Unavailable)?
        .ok_or(SharingApiError::NotFound)?;

        let timezone: String = row.get("timezone");
        let today = daily_total(&mut connection, system_id, &timezone, &self.clock).await?;
        Ok(PublicSystem {
            name: row.get("name"),
            location: public_location(
                &row.get::<String, _>("location_precision"),
                row.try_get("description").ok(),
                row.try_get("country_code").ok().flatten(),
            ),
            commissioning_date: row.try_get("commissioning_date").ok().flatten(),
            capacity_watts: row.try_get("capacity_watts").ok().flatten(),
            timezone,
            generation_today_wh: today,
            generation_total_wh: row.try_get("lifetime_wh").ok().flatten(),
            last_seen_epoch_millis: row.try_get("last_seen").ok().flatten(),
        })
    }
}

/// Reduces a placement to the precision its owner allowed.
///
/// `hidden` yields nothing at all, `country` yields only the country code, and
/// the coarser settings pass the free-text description through — that text is
/// what the owner chose to write, so it is theirs to expose.
fn public_location(
    precision: &str,
    description: Option<String>,
    country_code: Option<String>,
) -> Option<String> {
    match precision {
        "hidden" => None,
        "country" => country_code,
        _ => description.filter(|value| !value.trim().is_empty()),
    }
}

#[cfg(feature = "sqlite")]
async fn daily_total(
    connection: &mut sqlx::SqliteConnection,
    system_id: SystemId,
    timezone: &str,
    clock: &Arc<dyn Clock>,
) -> Result<Option<i64>, SharingApiError> {
    let Ok(zone) = timezone.parse::<Tz>() else {
        return Ok(None);
    };
    let now = i64::try_from(clock.now().epoch_millis()).unwrap_or_default();
    let Some(local) = DateTime::from_timestamp_millis(now).map(|value| value.with_timezone(&zone))
    else {
        return Ok(None);
    };
    let date: NaiveDate = local.date_naive();
    let total: Option<i64> = sqlx::query_scalar(
        "SELECT generation_energy_wh FROM system_daily_summaries \
         WHERE system_id=? AND local_date=?",
    )
    .bind(id_blob(system_id))
    .bind(date.format("%Y-%m-%d").to_string())
    .fetch_optional(&mut *connection)
    .await
    .map_err(|_| SharingApiError::Unavailable)?
    .flatten();
    Ok(total)
}

/// Registry row backing one system's sharing state.
#[cfg(feature = "sqlite")]
struct RegistryEntry {
    account_id: AccountId,
    public_token: Option<String>,
    link_enabled: bool,
}

#[cfg(feature = "sqlite")]
async fn read_registry(
    connection: &mut sqlx::SqliteConnection,
    system_id: SystemId,
) -> Result<RegistryEntry, SharingApiError> {
    let row = sqlx::query(
        "SELECT account_id,public_token,link_enabled FROM system_registry WHERE system_id=?",
    )
    .bind(id_blob(system_id))
    .fetch_optional(&mut *connection)
    .await
    .map_err(|_| SharingApiError::Unavailable)?
    .ok_or(SharingApiError::NotFound)?;
    Ok(RegistryEntry {
        account_id: account_uuid(&row, "account_id")?,
        public_token: row.try_get("public_token").ok().flatten(),
        link_enabled: row.try_get::<i64, _>("link_enabled").unwrap_or_default() != 0,
    })
}

/// Builds an unguessable token from fresh randomness.
///
/// A version-4 identifier carries 122 random bits, so the token is not derived
/// from the system identifier and cannot be guessed from one link to the next.
#[cfg(feature = "sqlite")]
fn generate_token() -> String {
    use std::fmt::Write as _;
    let bytes = *uuid::Uuid::new_v4().as_bytes();
    bytes.iter().take(TOKEN_BYTES).fold(
        String::with_capacity(TOKEN_BYTES * 2),
        |mut token, byte| {
            let _ = write!(token, "{byte:02x}");
            token
        },
    )
}

#[cfg(feature = "sqlite")]
fn system_uuid(row: &sqlx::sqlite::SqliteRow, column: &str) -> Result<SystemId, SharingApiError> {
    uuid::Uuid::from_slice(&row.get::<Vec<u8>, _>(column))
        .ok()
        .and_then(|value| SystemId::from_uuid(value).ok())
        .ok_or(SharingApiError::Unavailable)
}

#[cfg(feature = "sqlite")]
fn account_uuid(row: &sqlx::sqlite::SqliteRow, column: &str) -> Result<AccountId, SharingApiError> {
    uuid::Uuid::from_slice(&row.get::<Vec<u8>, _>(column))
        .ok()
        .and_then(|value| AccountId::from_uuid(value).ok())
        .ok_or(SharingApiError::Unavailable)
}

#[cfg(feature = "sqlite")]
fn id_blob(id: SystemId) -> Vec<u8> {
    id.as_uuid().as_bytes().to_vec()
}

#[cfg(test)]
mod tests {
    use super::public_location;
    use pvlog_api::PublicSystem;

    /// Everything an anonymous visitor is allowed to see, and nothing else.
    ///
    /// Pinning the field set means a later addition to the response has to pass
    /// through this test, rather than quietly widening what a public link
    /// exposes.
    #[test]
    fn the_public_view_carries_only_the_agreed_fields() {
        let view = PublicSystem {
            name: "Testanlage".to_owned(),
            location: None,
            commissioning_date: None,
            capacity_watts: Some(19_520),
            timezone: "Europe/Berlin".to_owned(),
            generation_today_wh: Some(609),
            generation_total_wh: Some(27_898_674),
            last_seen_epoch_millis: Some(1_788_022_485_000),
        };
        let Ok(encoded) = serde_json::to_value(&view) else {
            panic!("the public view must serialize");
        };
        let Some(object) = encoded.as_object() else {
            panic!("the public view must serialize to an object");
        };
        let mut fields: Vec<&str> = object.keys().map(String::as_str).collect();
        fields.sort_unstable();
        assert_eq!(
            fields,
            [
                "capacityWatts",
                "commissioningDate",
                "generationTodayWh",
                "generationTotalWh",
                "lastSeenEpochMillis",
                "location",
                "name",
                "timezone",
            ]
        );
    }

    #[test]
    fn a_hidden_placement_reveals_nothing() {
        assert_eq!(
            public_location(
                "hidden",
                Some("Wohnhaus Dach".to_owned()),
                Some("DE".to_owned())
            ),
            None
        );
    }

    #[test]
    fn a_country_placement_reveals_only_the_country() {
        assert_eq!(
            public_location(
                "country",
                Some("Wohnhaus Dach".to_owned()),
                Some("DE".to_owned())
            ),
            Some("DE".to_owned())
        );
    }

    #[test]
    fn a_blank_description_is_omitted_rather_than_sent_empty() {
        assert_eq!(
            public_location("locality", Some("   ".to_owned()), None),
            None
        );
    }
}
