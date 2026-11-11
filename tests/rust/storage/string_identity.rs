//! Saving an inverter keeps the identity of its strings.
//!
//! Measurement channels and forecast settings refer to a string by identifier.
//! A save that recreated every string would silently detach them, so a string
//! that is resubmitted keeps its identifier, and only one left out is removed.

use std::error::Error;

use pvlog_domain::{AccountId, EquipmentValueProvenance, InverterId, StringId, SystemId};
use pvlog_storage::{
    AccountConfigurationRepository, AccountRepositoryError, DatabaseTarget, InverterRecord,
    PostgresAccountConfigurationRepository, PvStringRecord, SqliteAccountConfigurationRepository,
    SqliteAccountPoolConfig, SqliteAccountPoolRouter, SqliteAccountProvisioner,
    SystemConfigurationRecord, apply_migrations,
};
use sqlx::{
    Connection as _, PgConnection, Row as _, SqliteConnection, sqlite::SqliteConnectOptions,
};
use tempfile::TempDir;
use uuid::Uuid;

#[tokio::test]
async fn sqlite_strings_keep_their_identity_and_channel_bindings() -> Result<(), Box<dyn Error>> {
    let directory = TempDir::new()?;
    let management_path = directory.path().join("management.sqlite3");
    let accounts_dir = directory.path().join("accounts");
    apply_migrations(&DatabaseTarget::Sqlite {
        management_path: management_path.clone(),
        accounts_dir: accounts_dir.clone(),
    })
    .await?;
    let account_id = AccountId::new();
    let mut management = SqliteConnection::connect_with(
        &SqliteConnectOptions::new()
            .filename(&management_path)
            .create_if_missing(false),
    )
    .await?;
    sqlx::query(
        "INSERT INTO accounts (id,slug,display_name,status,created_at,updated_at) \
         VALUES (?,'identity','Identity','provisioning',1,1)",
    )
    .bind(account_id.as_uuid().as_bytes().as_slice())
    .execute(&mut management)
    .await?;
    management.close().await?;
    SqliteAccountProvisioner::new(management_path.clone(), accounts_dir.clone())
        .provision(account_id)
        .await?;
    let router = SqliteAccountPoolRouter::new(
        management_path,
        accounts_dir,
        SqliteAccountPoolConfig::default(),
    )?;
    let account = router.route(account_id).await?;
    let repository = SqliteAccountConfigurationRepository::new(account.clone());

    let fixture = Fixture::save(&repository).await?;
    {
        let mut writer = account.acquire_writer().await?;
        for (key, string) in [("kept", fixture.kept), ("dropped", fixture.dropped)] {
            sqlx::query(
                "INSERT INTO channel_definitions (id,system_id,channel_key,display_name,\
                 data_type,unit,scale,effective_from,created_at,updated_at,target_kind,target_id) \
                 VALUES (?,?,?,?,'integer','W',0,0,1,1,'string',?)",
            )
            .bind(Uuid::now_v7().as_bytes().as_slice())
            .bind(fixture.system_id.as_uuid().as_bytes().as_slice())
            .bind(key)
            .bind(key)
            .bind(string.as_uuid().as_bytes().as_slice())
            .execute(writer.connection())
            .await?;
        }
    }

    fixture.update_and_verify(&repository).await?;

    let mut connection = account.acquire().await?;
    let rows = sqlx::query(
        "SELECT channel_key,target_kind,target_id FROM channel_definitions ORDER BY channel_key",
    )
    .fetch_all(&mut *connection)
    .await?;
    let bindings: Vec<(String, String, Option<Vec<u8>>)> = rows
        .iter()
        .map(|row| (row.get(0), row.get(1), row.get(2)))
        .collect();
    assert_eq!(
        bindings,
        vec![
            ("dropped".to_owned(), "unassigned".to_owned(), None),
            (
                "kept".to_owned(),
                "string".to_owned(),
                Some(fixture.kept.as_uuid().as_bytes().to_vec())
            ),
        ],
        "a kept string stays bound; a removed one releases its channel"
    );
    Ok(())
}

#[tokio::test]
async fn postgres_strings_keep_their_identity_and_channel_bindings_when_configured()
-> Result<(), Box<dyn Error>> {
    let Ok(url) = std::env::var("TEST_POSTGRES_URL") else {
        return Ok(());
    };
    apply_migrations(&DatabaseTarget::Postgres { url: url.clone() }).await?;
    let account_id = AccountId::new();
    let mut connection = PgConnection::connect(&url).await?;
    sqlx::query(
        "INSERT INTO management.accounts (id,slug,display_name,status,created_at,updated_at) \
         VALUES ($1,$2,'Identity','active',1,1)",
    )
    .bind(account_id.as_uuid())
    .bind(format!("identity-{account_id}"))
    .execute(&mut connection)
    .await?;
    let repository = PostgresAccountConfigurationRepository::new(url, account_id);

    let fixture = Fixture::save(&repository).await?;
    for (key, string) in [("kept", fixture.kept), ("dropped", fixture.dropped)] {
        sqlx::query(
            "INSERT INTO account_data.channel_definitions (account_id,id,system_id,channel_key,\
             display_name,data_type,unit,scale,effective_from,created_at,updated_at,\
             target_kind,target_id) VALUES ($1,$2,$3,$4,$4,'integer','W',0,0,1,1,'string',$5)",
        )
        .bind(account_id.as_uuid())
        .bind(Uuid::now_v7())
        .bind(fixture.system_id.as_uuid())
        .bind(key)
        .bind(string.as_uuid())
        .execute(&mut connection)
        .await?;
    }

    fixture.update_and_verify(&repository).await?;

    let rows = sqlx::query(
        "SELECT channel_key,target_kind,target_id FROM account_data.channel_definitions \
         WHERE account_id=$1 ORDER BY channel_key",
    )
    .bind(account_id.as_uuid())
    .fetch_all(&mut connection)
    .await?;
    connection.close().await?;
    let bindings: Vec<(String, String, Option<Uuid>)> = rows
        .iter()
        .map(|row| (row.get(0), row.get(1), row.get(2)))
        .collect();
    assert_eq!(
        bindings,
        vec![
            ("dropped".to_owned(), "unassigned".to_owned(), None),
            (
                "kept".to_owned(),
                "string".to_owned(),
                Some(fixture.kept.as_uuid())
            ),
        ]
    );
    Ok(())
}

/// A system with one inverter and two strings, as first saved.
struct Fixture {
    system_id: SystemId,
    inverter: InverterRecord,
    kept: StringId,
    dropped: StringId,
}

impl Fixture {
    async fn save(repository: &dyn AccountConfigurationRepository) -> Result<Self, Box<dyn Error>> {
        let system_id = SystemId::new();
        repository
            .save_system(&SystemConfigurationRecord {
                id: system_id,
                name: "Identity".to_owned(),
                description: String::new(),
                timezone: "UTC".to_owned(),
                visibility: "private".to_owned(),
                lifecycle: "active".to_owned(),
                status_interval_seconds: 300,
                power_calculation_mode: "reported".to_owned(),
                net_calculation_mode: "separate_flows".to_owned(),
                created_at: 1,
                updated_at: 1,
            })
            .await?;
        let inverter_id = InverterId::new();
        let inverter = InverterRecord {
            id: inverter_id,
            system_id,
            name: "Inverter".to_owned(),
            manufacturer: None,
            model: None,
            serial_reference: None,
            rated_power_watts: None,
            catalog_entry_id: None,
            catalog_revision: None,
            value_provenance: EquipmentValueProvenance::Manual,
            specification_snapshot: None,
            effective_from: 0,
            effective_to: None,
            created_at: 1,
            updated_at: 1,
            strings: vec![string(inverter_id, "East"), string(inverter_id, "West")],
        };
        repository.save_inverter_aggregate(&inverter).await?;
        Ok(Self {
            system_id,
            kept: inverter.strings[0].id,
            dropped: inverter.strings[1].id,
            inverter,
        })
    }

    /// Renames the kept string, leaves one out, adds one, and checks identities.
    async fn update_and_verify(
        &self,
        repository: &dyn AccountConfigurationRepository,
    ) -> Result<(), Box<dyn Error>> {
        let mut updated = self.inverter.clone();
        updated.name = "Inverter renamed".to_owned();
        updated.updated_at = 2;
        updated.strings.remove(1);
        updated.strings[0].name = "East renamed".to_owned();
        updated.strings[0].updated_at = 2;
        let added = string(updated.id, "South");
        updated.strings.push(added.clone());
        repository.save_inverter_aggregate(&updated).await?;

        let mut ids = repository.inverter_string_ids(updated.id).await?;
        ids.sort();
        let mut expected = vec![self.kept, added.id];
        expected.sort();
        assert_eq!(
            ids, expected,
            "kept and added remain; the omitted one is gone"
        );

        let stored = repository.effective_inverters(self.system_id, 0).await?;
        let kept = stored
            .first()
            .and_then(|inverter| {
                inverter
                    .strings
                    .iter()
                    .find(|string| string.id == self.kept)
            })
            .ok_or("kept string missing")?;
        assert_eq!(kept.name, "East renamed");
        assert_eq!(kept.created_at, 1, "an update keeps the creation time");

        // A string of this inverter cannot be claimed by another one.
        let other_id = InverterId::new();
        let mut thief = self.inverter.clone();
        thief.id = other_id;
        thief.strings = vec![PvStringRecord {
            inverter_id: other_id,
            ..updated.strings[0].clone()
        }];
        assert!(matches!(
            repository.save_inverter_aggregate(&thief).await,
            Err(AccountRepositoryError::InvalidRecord("PV string"))
        ));
        Ok(())
    }
}

fn string(inverter_id: InverterId, name: &str) -> PvStringRecord {
    PvStringRecord {
        id: StringId::new(),
        inverter_id,
        name: name.to_owned(),
        panel_count: 10,
        panel_manufacturer: Some("Example".to_owned()),
        panel_model: Some("P400".to_owned()),
        rated_power_watts: 4_000,
        module_catalog_entry_id: None,
        module_catalog_revision: None,
        value_provenance: EquipmentValueProvenance::Manual,
        module_specification_snapshot: None,
        module_peak_power_watts: Some(400),
        total_peak_power_watts: Some(4_000),
        orientation_degrees: Some(90),
        tilt_degrees: Some(30),
        temperature_coefficient_ppm_per_celsius: None,
        shading: Vec::new(),
        effective_from: 0,
        effective_to: None,
        created_at: 1,
        updated_at: 1,
    }
}
