use std::{error::Error, fs};

use pvlog::operator_bundle::{
    export_account_bundle, export_bundle, export_postgres_bundle, import_bundle, verify_bundle,
};
use pvlog_storage::DatabaseTarget;

#[tokio::test]
async fn bundle_round_trip_and_corruption_detection() -> Result<(), Box<dyn Error>> {
    let source = tempfile::tempdir()?;
    let accounts = source.path().join("accounts");
    fs::create_dir(&accounts)?;
    create_database(&source.path().join("management.sqlite3")).await?;
    create_database(&accounts.join("opaque-account.sqlite3")).await?;
    let source_target = DatabaseTarget::Sqlite {
        management_path: source.path().join("management.sqlite3"),
        accounts_dir: accounts,
    };
    let bundle = source.path().join("bundle");
    let manifest = export_bundle(&source_target, &bundle).await?;
    assert_eq!(manifest.format_version, 1);
    assert_eq!(manifest.files.len(), 2);
    verify_bundle(&bundle)?;

    let destination = tempfile::tempdir()?;
    let destination_target = DatabaseTarget::Sqlite {
        management_path: destination.path().join("restored/management.sqlite3"),
        accounts_dir: destination.path().join("restored/accounts"),
    };
    import_bundle(&destination_target, &bundle, true)?;
    import_bundle(&destination_target, &bundle, false)?;
    assert!(
        destination
            .path()
            .join("restored/management.sqlite3")
            .is_file()
    );

    fs::write(bundle.join("accounts/opaque-account.sqlite3"), b"corrupt")?;
    assert!(verify_bundle(&bundle).is_err());

    let account_bundle = source.path().join("account-bundle");
    let account_manifest =
        export_account_bundle(&source_target, &account_bundle, "opaque-account.sqlite3").await?;
    assert_eq!(account_manifest.scope, "account:opaque-account.sqlite3");
    assert_eq!(account_manifest.files.len(), 1);
    let account_destination = tempfile::tempdir()?;
    let account_target = DatabaseTarget::Sqlite {
        management_path: account_destination.path().join("management.sqlite3"),
        accounts_dir: account_destination.path().join("accounts"),
    };
    import_bundle(&account_target, &account_bundle, false)?;
    assert!(
        account_destination
            .path()
            .join("accounts/opaque-account.sqlite3")
            .is_file()
    );
    let postgres_target = DatabaseTarget::Postgres {
        url: "postgres://verification.invalid/pvlog".to_owned(),
    };
    import_bundle(&postgres_target, &account_bundle, true)?;

    let archive = source.path().join("postgres-source.dump");
    fs::write(&archive, b"consistent pg_dump archive")?;
    let postgres_bundle = source.path().join("postgres-bundle");
    let postgres_manifest = export_postgres_bundle(&postgres_target, &postgres_bundle, &archive)?;
    assert_eq!(postgres_manifest.source_backend, "postgres");
    verify_bundle(&postgres_bundle)?;
    Ok(())
}

async fn create_database(path: &std::path::Path) -> Result<(), sqlx::Error> {
    use sqlx::{Connection as _, Executor as _};
    let options = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true);
    let mut connection = sqlx::SqliteConnection::connect_with(&options).await?;
    connection
        .execute("CREATE TABLE marker (value TEXT NOT NULL)")
        .await?;
    connection
        .execute("INSERT INTO marker VALUES ('ok')")
        .await?;
    connection.close().await
}
