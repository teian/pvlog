//! Shared account configuration repository and effective-date contracts.

use std::error::Error;

use pvlog_domain::{
    AccountId, AuditEventId, ChannelId, EquipmentId, EquipmentValueProvenance, InverterId,
    StringId, SystemId, TariffId,
};
use pvlog_storage::{
    AccountAuditRecord, AccountConfigurationRepository, AccountRepositoryError,
    ChannelDefinitionRecord, DatabaseTarget, EquipmentRecord, InverterRecord,
    PostgresAccountConfigurationRepository, PvStringRecord, SqliteAccountConfigurationRepository,
    SqliteAccountPoolConfig, SqliteAccountPoolRouter, SqliteAccountProvisioner,
    SystemConfigurationRecord, TariffRecord, apply_migrations,
};
use sqlx::{Connection as _, PgConnection, SqliteConnection, sqlite::SqliteConnectOptions};
use tempfile::TempDir;
use uuid::Uuid;

#[tokio::test]
async fn sqlite_account_configuration_repository_contract() -> Result<(), Box<dyn Error>> {
    let directory = TempDir::new()?;
    let management_path = directory.path().join("management.sqlite3");
    let accounts_dir = directory.path().join("accounts");
    apply_migrations(&DatabaseTarget::Sqlite {
        management_path: management_path.clone(),
        accounts_dir: accounts_dir.clone(),
    })
    .await?;
    let account_a = create_sqlite_account(&management_path, &accounts_dir, "a").await?;
    let account_b = create_sqlite_account(&management_path, &accounts_dir, "b").await?;
    let router = SqliteAccountPoolRouter::new(
        management_path,
        accounts_dir,
        SqliteAccountPoolConfig::default(),
    )?;
    let repository_a = SqliteAccountConfigurationRepository::new(router.route(account_a).await?);
    let repository_b = SqliteAccountConfigurationRepository::new(router.route(account_b).await?);
    verify_contract(&repository_a, &repository_b).await
}

#[tokio::test]
async fn postgres_account_configuration_repository_contract_when_configured()
-> Result<(), Box<dyn Error>> {
    let Ok(url) = std::env::var("TEST_POSTGRES_URL") else {
        return Ok(());
    };
    apply_migrations(&DatabaseTarget::Postgres { url: url.clone() }).await?;
    let account_a = create_postgres_account(&url, "a").await?;
    let account_b = create_postgres_account(&url, "b").await?;
    let repository_a = PostgresAccountConfigurationRepository::new(url.clone(), account_a);
    let repository_b = PostgresAccountConfigurationRepository::new(url, account_b);
    verify_contract(&repository_a, &repository_b).await
}

#[allow(clippy::too_many_lines)]
async fn verify_contract(
    repository: &dyn AccountConfigurationRepository,
    other_account: &dyn AccountConfigurationRepository,
) -> Result<(), Box<dyn Error>> {
    assert_ne!(repository.account_id(), other_account.account_id());
    let system_id = SystemId::new();
    let system = SystemConfigurationRecord {
        id: system_id,
        name: "Roof PV".to_owned(),
        description: "Contract fixture".to_owned(),
        timezone: "Europe/Berlin".to_owned(),
        visibility: "private".to_owned(),
        lifecycle: "active".to_owned(),
        status_interval_seconds: 300,
        power_calculation_mode: "reported".to_owned(),
        net_calculation_mode: "separate_flows".to_owned(),
        created_at: 1,
        updated_at: 1,
    };
    repository.save_system(&system).await?;
    assert_eq!(repository.system(system_id).await?, Some(system));
    assert!(other_account.system(system_id).await?.is_none());

    let inverter = inverter(system_id, 0, None);
    repository.save_inverter_aggregate(&inverter).await?;
    assert_eq!(
        repository.effective_inverters(system_id, 0).await?,
        vec![inverter.clone()]
    );
    assert!(
        other_account
            .effective_inverters(system_id, 0)
            .await?
            .is_empty()
    );
    let mut invalid_inverter = inverter;
    invalid_inverter.strings[0].inverter_id = InverterId::new();
    assert!(matches!(
        repository.save_inverter_aggregate(&invalid_inverter).await,
        Err(AccountRepositoryError::InvalidRecord("PV string"))
    ));

    let first_equipment = equipment(system_id, "Original", 0, Some(10));
    let second_equipment = equipment(system_id, "Replacement", 10, None);
    repository.save_equipment(&first_equipment).await?;
    repository.save_equipment(&second_equipment).await?;
    assert_eq!(
        repository.effective_equipment(system_id, 9).await?,
        vec![first_equipment]
    );
    assert_eq!(
        repository.effective_equipment(system_id, 10).await?,
        vec![second_equipment]
    );

    let old_tariff = tariff(system_id, "Old", 0, Some(10));
    let new_tariff = tariff(system_id, "New", 10, None);
    repository.save_tariff(&old_tariff).await?;
    repository.save_tariff(&new_tariff).await?;
    assert_eq!(
        repository.effective_tariff(system_id, "export", 9).await?,
        Some(old_tariff)
    );
    assert_eq!(
        repository.effective_tariff(system_id, "export", 10).await?,
        Some(new_tariff)
    );

    let channel = ChannelDefinitionRecord {
        id: ChannelId::new(),
        system_id,
        channel_key: "inverter_efficiency".to_owned(),
        display_name: "Inverter efficiency".to_owned(),
        data_type: "decimal".to_owned(),
        unit: "percent".to_owned(),
        scale: 2,
        minimum_value: Some(0),
        maximum_value: Some(10_000),
        lifecycle: "active".to_owned(),
        effective_from: 10,
        effective_to: Some(20),
        display: serde_json::json!({"decimals": 2}),
        created_at: 1,
        updated_at: 1,
    };
    repository.save_channel(&channel).await?;
    assert!(
        repository
            .effective_channel(system_id, &channel.channel_key, 9)
            .await?
            .is_none()
    );
    assert_eq!(
        repository
            .effective_channel(system_id, &channel.channel_key, 10)
            .await?,
        Some(channel.clone())
    );
    assert!(
        repository
            .effective_channel(system_id, &channel.channel_key, 20)
            .await?
            .is_none()
    );

    let mut invalid = channel;
    invalid.id = ChannelId::new();
    "invalid_range".clone_into(&mut invalid.channel_key);
    invalid.effective_from = 20;
    invalid.effective_to = Some(20);
    assert!(matches!(
        repository.save_channel(&invalid).await,
        Err(AccountRepositoryError::InvalidEffectiveRange)
    ));

    let audit = audit_record(system_id);
    repository.append_audit(&audit).await?;
    assert_eq!(repository.audit(10).await?, vec![audit]);
    assert!(other_account.audit(10).await?.is_empty());
    Ok(())
}

fn equipment(
    system_id: SystemId,
    name: &str,
    effective_from: i64,
    effective_to: Option<i64>,
) -> EquipmentRecord {
    EquipmentRecord {
        id: EquipmentId::new(),
        system_id,
        equipment_kind: "battery".to_owned(),
        name: name.to_owned(),
        capacity_watts: Some(8_000),
        effective_from,
        effective_to,
        configuration: serde_json::json!({}),
        created_at: 1,
        updated_at: 1,
    }
}

fn inverter(system_id: SystemId, effective_from: i64, effective_to: Option<i64>) -> InverterRecord {
    let inverter_id = InverterId::new();
    InverterRecord {
        id: inverter_id,
        system_id,
        name: "Roof inverter".to_owned(),
        manufacturer: Some("Example".to_owned()),
        model: Some("INV-8K".to_owned()),
        serial_reference: None,
        rated_power_watts: Some(8_000),
        catalog_entry_id: None,
        catalog_revision: None,
        value_provenance: EquipmentValueProvenance::Manual,
        specification_snapshot: None,
        effective_from,
        effective_to,
        created_at: 1,
        updated_at: 1,
        strings: vec![PvStringRecord {
            id: StringId::new(),
            inverter_id,
            name: "South roof".to_owned(),
            panel_count: 20,
            panel_manufacturer: Some("Example".to_owned()),
            panel_model: Some("P400".to_owned()),
            rated_power_watts: 8_000,
            module_catalog_entry_id: None,
            module_catalog_revision: None,
            value_provenance: EquipmentValueProvenance::Manual,
            module_specification_snapshot: None,
            module_peak_power_watts: None,
            total_peak_power_watts: None,
            orientation_degrees: Some(180),
            tilt_degrees: Some(35),
            effective_from,
            effective_to,
            created_at: 1,
            updated_at: 1,
        }],
    }
}

fn tariff(
    system_id: SystemId,
    name: &str,
    effective_from: i64,
    effective_to: Option<i64>,
) -> TariffRecord {
    TariffRecord {
        id: TariffId::new(),
        system_id,
        name: name.to_owned(),
        direction: "export".to_owned(),
        currency_code: "EUR".to_owned(),
        minor_units_per_kwh: 8,
        schedule: serde_json::json!({}),
        effective_from,
        effective_to,
        created_at: 1,
        updated_at: 1,
    }
}

fn audit_record(system_id: SystemId) -> AccountAuditRecord {
    let id = AuditEventId::new();
    let mut event_hash = [0_u8; 32];
    event_hash[..16].copy_from_slice(id.as_uuid().as_bytes());
    event_hash[16..].copy_from_slice(id.as_uuid().as_bytes());
    AccountAuditRecord {
        id,
        occurred_at: 1,
        request_id: Some(Uuid::now_v7()),
        actor_type: "user".to_owned(),
        actor_id: Some(Uuid::now_v7()),
        action: "system.configure".to_owned(),
        target_type: "system".to_owned(),
        target_id: Some(system_id.as_uuid()),
        outcome: "succeeded".to_owned(),
        previous_event_hash: None,
        event_hash,
        safe_metadata: serde_json::json!({"source": "contract"}),
    }
}

async fn create_sqlite_account(
    management_path: &std::path::Path,
    accounts_dir: &std::path::Path,
    label: &str,
) -> Result<AccountId, Box<dyn Error>> {
    let account_id = AccountId::new();
    let mut management = SqliteConnection::connect_with(
        &SqliteConnectOptions::new()
            .filename(management_path)
            .create_if_missing(false)
            .foreign_keys(true),
    )
    .await?;
    sqlx::query(
        "INSERT INTO accounts (id,slug,display_name,status,created_at,updated_at) \
         VALUES (?,?,?,'provisioning',1,1)",
    )
    .bind(account_id.as_uuid().as_bytes().as_slice())
    .bind(format!("config-{label}-{account_id}"))
    .bind(format!("Account {label}"))
    .execute(&mut management)
    .await?;
    management.close().await?;
    SqliteAccountProvisioner::new(management_path.to_owned(), accounts_dir.to_owned())
        .provision(account_id)
        .await?;
    Ok(account_id)
}

async fn create_postgres_account(url: &str, label: &str) -> Result<AccountId, Box<dyn Error>> {
    let account_id = AccountId::new();
    let mut connection = PgConnection::connect(url).await?;
    sqlx::query(
        "INSERT INTO management.accounts \
         (id,slug,display_name,status,created_at,updated_at) \
         VALUES ($1,$2,$3,'active',1,1)",
    )
    .bind(account_id.as_uuid())
    .bind(format!("config-{label}-{account_id}"))
    .bind(format!("Account {label}"))
    .execute(&mut connection)
    .await?;
    connection.close().await?;
    Ok(account_id)
}
