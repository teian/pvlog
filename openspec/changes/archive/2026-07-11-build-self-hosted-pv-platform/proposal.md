## Why

PV system owners and operators need a self-hosted, long-lived photovoltaic monitoring platform with a safe, clear modern API and complete operational functionality. The platform must remain practical from a single-system SQLite installation through a PostgreSQL deployment serving thousands of systems and decades of high-frequency telemetry.

## What Changes

- Create a Rust/Axum backend and web application for registering, configuring, monitoring, comparing, and administering photovoltaic systems.
- Organize all backend crates and backend source beneath `src/crates/`, all frontend source beneath `src/ui/`, and all unit, integration, contract, end-to-end, performance, fixture, and test-support code beneath the repository-root `tests/` directory.
- Provide complete modern API functionality for output/status ingestion, queries and statistics, system management, insolation and supply data, and notification registration without emulating a third-party wire protocol.
- Add a versioned, resource-oriented JSON REST API with consistent authentication, pagination, filtering, timestamps, idempotency, bulk operations, validation errors, and rate-limit metadata.
- Publish a complete OpenAPI 3.1 specification plus rendered reference documentation, guides, examples, feature coverage notes, and executable API conformance checks.
- Support SQLite and PostgreSQL through one storage contract, with SQLite split into an instance-wide management database and one isolated data database per account, plus a tiered time-series model that retains exact raw measurements for at least 25 years while serving chart queries from precomputed rollups.
- Add secure multi-user and multi-system access, scoped API credentials, auditability, configurable quotas, and privacy controls suitable for self-hosting.
- Provide first-class local users with password authentication and hierarchical RBAC, while allowing multiple administrator-configured external OIDC/OAuth2 login connectors including ready-to-configure Google, GitHub, Facebook, and X presets.
- Provide background processing for aggregation, compaction, data quality, notifications, weather/insolation integration, retention verification, and operational maintenance.
- Provide container-first deployment, migrations, backup/restore, health and readiness checks, structured telemetry, and documented scaling paths.

## Capabilities

### New Capabilities

- `identity-and-access`: Local users and password authentication, multiple external OIDC/OAuth2 login connectors including Google/GitHub/Facebook/X presets, identity linking, hierarchical RBAC, accounts, sessions, scoped API credentials, quotas, and audit records.
- `pv-system-management`: PV system aggregates containing inverters and their PV strings, plus tariffs, privacy, lifecycle operations, and bulk import/export.
- `telemetry-ingestion`: Validated single and batch ingestion of generation, consumption, export/import, battery, temperature, voltage, and extended measurements with idempotent correction semantics.
- `time-series-storage`: SQLite management/account database separation, durable hot/raw/rollup storage, compaction, retention, database portability, backup integrity, and the capacity envelope for multi-decade data.
- `analytics-and-charting`: Fast resolution-aware time-series queries, daily and lifetime statistics, missing-data and quality signals, and interactive charts.
- `modern-rest-api`: A versioned JSON API with modern resource naming, filtering, pagination, bulk operations, concurrency controls, error documents, and stable evolution rules.
- `notifications-and-integrations`: Alert rules, webhook registration and delivery, retries, signing, uploader integration, insolation, and regional supply data providers.
- `web-application`: Responsive self-hosted administration and monitoring UI for onboarding, live status, historical charts, data quality, alerts, and settings.
- `documentation-and-openapi`: A committed OpenAPI 3.1 contract, generated API reference, task-oriented guides, examples, feature coverage documentation, changelog, and automated documentation checks.
- `self-hosting-operations`: Reproducible deployment, configuration, migrations, workers, observability, backup/restore, upgrades, and SQLite-to-PostgreSQL migration.

### Modified Capabilities

None. This repository does not yet contain baseline capability specifications.

## Impact

- Introduces a Rust workspace rooted in `src/crates/` for Axum HTTP services, domain/application layers, background workers, database adapters, and migrations.
- Introduces a TypeScript web frontend rooted in `src/ui/`, a generated/documented API client, and charting workflows.
- Introduces a repository-root `tests/` hierarchy containing all Rust and frontend unit, integration, contract, end-to-end, performance, fixture, and test-support code.
- Adds a SQLite management catalog and per-account data database topology, a PostgreSQL runtime profile, container packaging, deployment examples, and operational documentation.
- Establishes a public modern API contract; subsequent breaking changes require explicit versioning and migration guidance.
- Adds dependencies for async HTTP, serialization/validation, SQL access and migrations, authentication/cryptography, OpenAPI tooling, compression, background jobs, tracing/metrics, and frontend delivery.
