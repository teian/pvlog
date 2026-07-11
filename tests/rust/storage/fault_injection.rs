use std::error::Error;

use sqlx::{Connection as _, Executor as _, Row as _};

#[tokio::test]
async fn interrupted_transaction_rolls_back_on_connection_loss() -> Result<(), Box<dyn Error>> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("interrupted.sqlite3");
    let options = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(&path)
        .create_if_missing(true);
    let mut connection = sqlx::SqliteConnection::connect_with(&options).await?;
    connection
        .execute("CREATE TABLE observations (value INTEGER NOT NULL)")
        .await?;
    let mut transaction = connection.begin().await?;
    transaction
        .execute("INSERT INTO observations VALUES (1)")
        .await?;
    drop(transaction);
    connection.close().await?;

    let mut reopened = sqlx::SqliteConnection::connect_with(&options).await?;
    let count: i64 = sqlx::query("SELECT COUNT(*) AS count FROM observations")
        .fetch_one(&mut reopened)
        .await?
        .get("count");
    assert_eq!(count, 0);
    Ok(())
}

#[tokio::test]
async fn disk_exhaustion_is_reported_without_partial_commit() -> Result<(), Box<dyn Error>> {
    let mut connection = sqlx::SqliteConnection::connect("sqlite::memory:").await?;
    connection
        .execute("CREATE TABLE payloads (value BLOB NOT NULL)")
        .await?;
    sqlx::query("PRAGMA max_page_count = 2")
        .execute(&mut connection)
        .await?;
    let result = sqlx::query("INSERT INTO payloads VALUES (zeroblob(1048576))")
        .execute(&mut connection)
        .await;
    let Err(error) = result else {
        return Err("bounded database unexpectedly accepted growth".into());
    };
    assert!(error.to_string().to_ascii_lowercase().contains("full"));
    let count: i64 = sqlx::query("SELECT COUNT(*) FROM payloads")
        .fetch_one(&mut connection)
        .await?
        .get(0);
    assert_eq!(count, 0);
    Ok(())
}
