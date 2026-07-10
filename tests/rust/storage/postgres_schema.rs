//! `PostgreSQL` management and account-ownership schema contracts.

use std::{collections::BTreeSet, error::Error};

use pvlog_storage::{DatabaseTarget, apply_migrations};
use sqlx::{Connection as _, PgConnection, Row as _};
use uuid::Uuid;

#[tokio::test]
async fn postgres_separates_management_account_community_integrations_and_jobs()
-> Result<(), Box<dyn Error>> {
    let Some((url, mut connection)) = postgres().await? else {
        return Ok(());
    };
    apply_migrations(&DatabaseTarget::Postgres { url }).await?;
    let schemas = sqlx::query_scalar::<_, String>(
        "SELECT schema_name FROM information_schema.schemata \
         WHERE schema_name IN ('management', 'account_data', 'community', 'integrations', 'jobs')",
    )
    .fetch_all(&mut connection)
    .await?
    .into_iter()
    .collect::<BTreeSet<_>>();
    assert_eq!(schemas.len(), 5);

    let tables = sqlx::query(
        "SELECT table_schema, table_name FROM information_schema.tables \
         WHERE table_schema IN ('management', 'account_data', 'community', 'integrations', 'jobs')",
    )
    .fetch_all(&mut connection)
    .await?
    .into_iter()
    .map(|row| {
        (
            row.get::<String, _>("table_schema"),
            row.get::<String, _>("table_name"),
        )
    })
    .collect::<BTreeSet<_>>();
    for required in [
        ("management", "users"),
        ("management", "local_credentials"),
        ("management", "auth_connectors"),
        ("management", "external_identities"),
        ("management", "sessions"),
        ("management", "accounts"),
        ("management", "memberships"),
        ("management", "api_credentials"),
        ("management", "rbac_roles"),
        ("management", "quota_policies"),
        ("management", "global_audit_events"),
        ("account_data", "systems"),
        ("account_data", "equipment"),
        ("account_data", "tariffs"),
        ("account_data", "channel_definitions"),
        ("community", "teams"),
        ("community", "team_memberships"),
        ("community", "favourites"),
        ("community", "system_projections"),
        ("integrations", "webhook_subscriptions"),
        ("integrations", "webhook_deliveries"),
        ("integrations", "webhook_delivery_attempts"),
        ("integrations", "providers"),
        ("jobs", "account_jobs"),
    ] {
        assert!(
            tables.contains(&(required.0.to_owned(), required.1.to_owned())),
            "missing PostgreSQL table {}.{}",
            required.0,
            required.1
        );
    }
    connection.close().await?;
    Ok(())
}

#[tokio::test]
async fn postgres_account_owned_primary_keys_and_foreign_keys_include_account_id()
-> Result<(), Box<dyn Error>> {
    let Some((url, mut connection)) = postgres().await? else {
        return Ok(());
    };
    apply_migrations(&DatabaseTarget::Postgres { url }).await?;
    let rows = sqlx::query(
        "SELECT namespace.nspname AS schema_name, relation.relname AS table_name, \
                bool_or(attribute.attname = 'account_id') AS account_in_primary_key \
         FROM pg_index AS index \
         JOIN pg_class AS relation ON relation.oid = index.indrelid \
         JOIN pg_namespace AS namespace ON namespace.oid = relation.relnamespace \
         JOIN pg_attribute AS attribute ON attribute.attrelid = relation.oid \
              AND attribute.attnum = ANY(index.indkey) \
         WHERE namespace.nspname IN ('account_data', 'community', 'integrations', 'jobs') \
               AND index.indisprimary \
         GROUP BY namespace.nspname, relation.relname \
         ORDER BY namespace.nspname, relation.relname",
    )
    .fetch_all(&mut connection)
    .await?;
    assert!(!rows.is_empty());
    for row in rows {
        let schema: String = row.get("schema_name");
        let table: String = row.get("table_name");
        let account_in_primary_key: bool = row.get("account_in_primary_key");
        assert!(
            account_in_primary_key,
            "account_id is absent from primary key {schema}.{table}"
        );
    }

    let account_a = Uuid::now_v7();
    let account_b = Uuid::now_v7();
    for (id, slug) in [
        (account_a, format!("account-a-{account_a}")),
        (account_b, format!("account-b-{account_b}")),
    ] {
        sqlx::query(
            "INSERT INTO management.accounts \
             (id, slug, display_name, status, created_at, updated_at) \
             VALUES ($1, $2, $2, 'active', 1, 1)",
        )
        .bind(id)
        .bind(&slug)
        .execute(&mut connection)
        .await?;
    }
    let system_id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO account_data.systems \
         (account_id, id, name, timezone, status_interval_seconds, power_calculation_mode, \
          net_calculation_mode, created_at, updated_at) \
         VALUES ($1, $2, 'PV', 'Europe/Berlin', 300, 'reported', 'separate_flows', 1, 1)",
    )
    .bind(account_a)
    .bind(system_id)
    .execute(&mut connection)
    .await?;
    assert!(
        sqlx::query(
            "INSERT INTO account_data.equipment \
             (account_id, id, system_id, equipment_kind, name, effective_from, created_at, updated_at) \
             VALUES ($1, $2, $3, 'array', 'Cross account', 1, 1, 1)",
        )
        .bind(account_b)
        .bind(Uuid::now_v7())
        .bind(system_id)
        .execute(&mut connection)
        .await
        .is_err()
    );
    connection.close().await?;
    Ok(())
}

#[tokio::test]
async fn postgres_auth_and_audit_constraints_are_provider_neutral_and_append_only()
-> Result<(), Box<dyn Error>> {
    let Some((url, mut connection)) = postgres().await? else {
        return Ok(());
    };
    apply_migrations(&DatabaseTarget::Postgres { url }).await?;
    assert!(
        sqlx::query(
            "INSERT INTO management.auth_connectors \
             (id, slug, display_name, protocol, client_id, client_secret_ref, scopes, \
              claim_mapping, created_at, updated_at) \
             VALUES ($1, 'vendor', 'Vendor', 'vendor', 'client', 'secret/ref', '[]', '{}', 1, 1)",
        )
        .bind(Uuid::now_v7())
        .execute(&mut connection)
        .await
        .is_err()
    );
    let audit_id = Uuid::now_v7();
    let mut event_hash = audit_id.as_bytes().to_vec();
    event_hash.extend_from_slice(audit_id.as_bytes());
    sqlx::query(
        "INSERT INTO management.global_audit_events \
         (id, occurred_at, actor_type, action, target_type, outcome, event_hash) \
         VALUES ($1, 1, 'worker', 'schema.test', 'instance', 'succeeded', $2)",
    )
    .bind(audit_id)
    .bind(event_hash)
    .execute(&mut connection)
    .await?;
    assert!(
        sqlx::query("DELETE FROM management.global_audit_events WHERE id = $1")
            .bind(audit_id)
            .execute(&mut connection)
            .await
            .is_err()
    );
    connection.close().await?;
    Ok(())
}

async fn postgres() -> Result<Option<(String, PgConnection)>, sqlx::Error> {
    let Ok(url) = std::env::var("TEST_POSTGRES_URL") else {
        return Ok(None);
    };
    let connection = PgConnection::connect(&url).await?;
    Ok(Some((url, connection)))
}
