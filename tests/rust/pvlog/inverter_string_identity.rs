//! A string identifier in a save must name a string the inverter already has.

use std::{error::Error, sync::Arc};

use pvlog::inverters::ManagementInverterApi;
use pvlog_api::{InverterApiError, InverterApiUseCases as _, InverterInput};
use pvlog_application::EquipmentCatalog;
use pvlog_domain::{AccountId, StringId, SystemId, UserId};
use pvlog_storage::{DatabaseTarget, SqliteAccountProvisioner, apply_migrations};
use sqlx::{ConnectOptions as _, Connection as _, sqlite::SqliteConnectOptions};
use tempfile::TempDir;

fn input(strings: &str) -> Result<InverterInput, serde_json::Error> {
    serde_json::from_str(&format!(
        r#"{{"name":"East","effectiveFrom":0,"strings":[{strings}]}}"#
    ))
}

fn string(id: Option<StringId>, name: &str) -> String {
    let id = id.map_or_else(String::new, |id| format!(r#""id":"{}","#, id.as_uuid()));
    format!(
        r#"{{{id}"name":"{name}","panelCount":10,"modulePeakPowerWatts":400,
            "panelManufacturer":"Example","panelModel":"P400","effectiveFrom":0}}"#
    )
}

#[tokio::test]
async fn known_strings_keep_their_identifier_and_invented_ones_are_refused()
-> Result<(), Box<dyn Error>> {
    let directory = TempDir::new()?;
    let management_path = directory.path().join("management.sqlite3");
    let accounts_dir = directory.path().join("accounts");
    let target = DatabaseTarget::Sqlite {
        management_path: management_path.clone(),
        accounts_dir: accounts_dir.clone(),
    };
    apply_migrations(&target).await?;
    let account_id = AccountId::new();
    let mut management = SqliteConnectOptions::new()
        .filename(&management_path)
        .create_if_missing(false)
        .connect()
        .await?;
    sqlx::query(
        "INSERT INTO accounts (id, slug, display_name, status, created_at, updated_at) \
         VALUES (?, 'identity', 'Identity', 'provisioning', 1, 1)",
    )
    .bind(account_id.as_uuid().as_bytes().as_slice())
    .execute(&mut management)
    .await?;
    management.close().await?;
    let database = SqliteAccountProvisioner::new(management_path, accounts_dir.clone())
        .provision(account_id)
        .await?;
    let system_id = SystemId::new();
    let mut account = SqliteConnectOptions::new()
        .filename(accounts_dir.join(database.opaque_locator))
        .create_if_missing(false)
        .connect()
        .await?;
    sqlx::query(
        "INSERT INTO systems (id, name, timezone, status_interval_seconds, \
         power_calculation_mode, net_calculation_mode, created_at, updated_at) \
         VALUES (?, 'Identity', 'UTC', 300, 'reported', 'separate_flows', 1, 1)",
    )
    .bind(system_id.as_uuid().as_bytes().as_slice())
    .execute(&mut account)
    .await?;
    account.close().await?;

    let api = ManagementInverterApi::new(target, Arc::new(EquipmentCatalog::bundled()?));
    let actor = UserId::new();

    let invented = input(&string(Some(StringId::new()), "S1"))?;
    assert!(
        matches!(
            api.create(actor, account_id, system_id, invented).await,
            Err(InverterApiError::InvalidInput("strings.id"))
        ),
        "a new inverter has no strings to refer to"
    );

    let created = api
        .create(actor, account_id, system_id, input(&string(None, "S1"))?)
        .await
        .map_err(|error| format!("create failed: {error:?}"))?;
    let first = created.strings[0].id;

    let updated = api
        .update(
            actor,
            account_id,
            system_id,
            created.id,
            input(&format!(
                "{},{}",
                string(Some(first), "S1 renamed"),
                string(None, "S2")
            ))?,
        )
        .await
        .map_err(|error| format!("update failed: {error:?}"))?;
    assert_eq!(updated.strings[0].id, first);
    assert_ne!(updated.strings[1].id, first);

    let twice = input(&format!(
        "{},{}",
        string(Some(first), "a"),
        string(Some(first), "b")
    ))?;
    assert!(matches!(
        api.update(actor, account_id, system_id, created.id, twice)
            .await,
        Err(InverterApiError::InvalidInput("strings.id"))
    ));
    let unknown = input(&string(Some(StringId::new()), "S9"))?;
    assert!(matches!(
        api.update(actor, account_id, system_id, created.id, unknown)
            .await,
        Err(InverterApiError::InvalidInput("strings.id"))
    ));
    Ok(())
}
