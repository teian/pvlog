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

#[tokio::test]
async fn postgres_telemetry_partitions_have_managed_horizons_and_expected_indexes()
-> Result<(), Box<dyn Error>> {
    let Some((url, mut connection)) = postgres().await? else {
        return Ok(());
    };
    apply_migrations(&DatabaseTarget::Postgres { url }).await?;

    let partitioned_tables = sqlx::query_scalar::<_, String>(
        "SELECT relation.relname \
         FROM pg_partitioned_table AS partitioned \
         JOIN pg_class AS relation ON relation.oid = partitioned.partrelid \
         JOIN pg_namespace AS namespace ON namespace.oid = relation.relnamespace \
         WHERE namespace.nspname = 'telemetry' \
               AND relation.relname IN ('hot_observations', 'rollups') \
         ORDER BY relation.relname",
    )
    .fetch_all(&mut connection)
    .await?
    .into_iter()
    .collect::<BTreeSet<_>>();
    assert_eq!(
        partitioned_tables,
        BTreeSet::from(["hot_observations".to_owned(), "rollups".to_owned()])
    );

    let index_definitions = sqlx::query_scalar::<_, String>(
        "SELECT indexdef FROM pg_indexes \
         WHERE schemaname = 'telemetry' \
           AND indexname IN ( \
             'hot_observations_system_time_partitioned_idx', \
             'hot_observations_measured_brin_idx', \
             'rollups_query_partitioned_idx', \
             'rollups_bucket_brin_idx' \
           )",
    )
    .fetch_all(&mut connection)
    .await?;
    assert_eq!(index_definitions.len(), 4);
    assert!(
        index_definitions
            .iter()
            .any(|definition| definition.contains("USING btree")
                && definition.contains("hot_observations"))
    );
    assert!(
        index_definitions
            .iter()
            .any(|definition| definition.contains("USING brin")
                && definition.contains("hot_observations"))
    );
    assert!(
        index_definitions
            .iter()
            .any(|definition| definition.contains("USING btree") && definition.contains("rollups"))
    );
    assert!(
        index_definitions
            .iter()
            .any(|definition| definition.contains("USING brin") && definition.contains("rollups"))
    );

    let horizons = sqlx::query(
        "SELECT parent_table::TEXT AS parent_table, created_partitions, covered_until \
         FROM telemetry.ensure_partition_horizon(now() + INTERVAL '18 months', now() + INTERVAL '18 months')",
    )
    .fetch_all(&mut connection)
    .await?;
    assert_eq!(horizons.len(), 2);
    assert!(
        horizons
            .iter()
            .all(|row| row.get::<i64, _>("covered_until") > 0)
    );

    connection.close().await?;
    Ok(())
}

#[tokio::test]
async fn postgres_chart_query_plan_fixtures_prune_partitions_and_use_indexes()
-> Result<(), Box<dyn Error>> {
    const JANUARY_2035: i64 = 2_051_222_400_000;
    const FEBRUARY_2035: i64 = 2_053_900_800_000;
    const MARCH_2035: i64 = 2_056_320_000_000;

    let Some((url, mut connection)) = postgres().await? else {
        return Ok(());
    };
    apply_migrations(&DatabaseTarget::Postgres { url }).await?;
    for (parent, prefix) in [
        ("telemetry.hot_observations", "hot_observations"),
        ("telemetry.rollups", "rollups"),
    ] {
        sqlx::query(
            "SELECT telemetry.ensure_monthly_partitions( \
                 $1::REGCLASS, $2, '2035-01-01T00:00:00Z'::TIMESTAMPTZ, \
                 '2035-03-01T00:00:00Z'::TIMESTAMPTZ)",
        )
        .bind(parent)
        .bind(prefix)
        .execute(&mut connection)
        .await?;
    }
    sqlx::query("SET enable_seqscan = off")
        .execute(&mut connection)
        .await?;

    let account_id = Uuid::now_v7();
    let system_id = Uuid::now_v7();
    let hot_fixture = include_str!("../../fixtures/postgres/query-plans/hot-chart.sql");
    let hot_plan = explain_fixture(hot_fixture, &mut connection, |query| {
        query
            .bind(account_id)
            .bind(system_id)
            .bind(JANUARY_2035)
            .bind(FEBRUARY_2035)
    })
    .await?;
    assert!(hot_plan.contains("hot_observations_y2035m01"));
    assert!(!hot_plan.contains("hot_observations_y2035m02"));
    assert!(hot_plan.contains("Index"));

    let rollup_fixture = include_str!("../../fixtures/postgres/query-plans/rollup-chart.sql");
    let rollup_plan = explain_fixture(rollup_fixture, &mut connection, |query| {
        query
            .bind(account_id)
            .bind(system_id)
            .bind("hour")
            .bind(FEBRUARY_2035)
            .bind(MARCH_2035)
    })
    .await?;
    assert!(rollup_plan.contains("rollups_y2035m02"));
    assert!(!rollup_plan.contains("rollups_y2035m01"));
    assert!(rollup_plan.contains("Index"));

    connection.close().await?;
    Ok(())
}

async fn explain_fixture<'q, F>(
    fixture: &str,
    connection: &mut PgConnection,
    bind: F,
) -> Result<String, sqlx::Error>
where
    F: FnOnce(
        sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments>,
    ) -> sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments>,
{
    let statement = format!(
        "EXPLAIN (FORMAT JSON, COSTS OFF) {}",
        fixture.trim_end_matches([';', '\n'])
    );
    // The statement is composed only from repository-owned fixture SQL and a fixed EXPLAIN prefix.
    let row = bind(sqlx::query(sqlx::AssertSqlSafe(statement)))
        .fetch_one(connection)
        .await?;
    let plan: serde_json::Value = row.get(0);
    Ok(plan.to_string())
}

async fn postgres() -> Result<Option<(String, PgConnection)>, sqlx::Error> {
    let Ok(url) = std::env::var("TEST_POSTGRES_URL") else {
        return Ok(None);
    };
    let connection = PgConnection::connect(&url).await?;
    Ok(Some((url, connection)))
}
