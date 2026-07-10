## ADDED Requirements

### Requirement: Repository source and test layout
The project SHALL place all production Rust crates and backend source beneath `src/crates/`, use `src/ui/` itself as the production frontend source root without `src/ui/src/`, and place all unit, integration, contract, compatibility, end-to-end, performance, fixture, mock, and test-support code beneath the repository-root `tests/` directory. Production source SHALL NOT depend on code beneath `tests/`.

#### Scenario: Repository layout is validated
- **WHEN** the repository structure quality check runs
- **THEN** it fails for production backend code outside `src/crates/`, a nested `src/ui/src/` root, production UI code outside `src/ui/`, or test-only code outside root `tests/`

#### Scenario: Rust tests execute from centralized harnesses
- **WHEN** the Cargo workspace test command runs
- **THEN** test harness packages beneath `tests/rust/` exercise production crates through their public interfaces without crate-local or inline test modules

### Requirement: Reproducible deployment profiles
The project SHALL provide versioned unprivileged container images and documented Compose profiles for SQLite and PostgreSQL, with immutable application images, persistent data volumes, an `.env.example`, and provider-neutral OIDC/OAuth2 connector configuration. The SQLite profile SHALL place the management database and opaque per-account database files beneath separately identifiable managed paths in the persistent data volume.

#### Scenario: Fresh SQLite deployment starts
- **WHEN** an operator follows the documented SQLite Compose quickstart with generated secrets
- **THEN** the server, worker, migration, and readiness checks succeed with a management database, a provisioned first-account database, persistent storage, and no external database dependency

### Requirement: Explicit configuration and secret handling
All runtime configuration SHALL be documented, typed, validated at startup, overridable through supported files/environment, and classified as public or secret. Authentication connector configuration SHALL use provider-neutral OIDC/OAuth2 fields and secret references while allowing multiple connector records. The system SHALL refuse unsafe missing production secrets and SHALL never log local credentials, connector secrets, provider tokens, reset tokens, or session values.

#### Scenario: Required production secret is absent
- **WHEN** production mode starts without a required session, encryption, or bootstrap secret
- **THEN** startup fails with a safe actionable configuration error before accepting traffic

### Requirement: Controlled schema migrations
The system SHALL provide explicit migration planning/status/application commands, acquire database migration locks, record checksums and versions, and prevent incompatible application startup against an unsupported schema. SQLite tooling SHALL report and migrate the management database and every registered account database independently with bounded concurrency and resumable per-account state.

#### Scenario: Two replicas attempt migration
- **WHEN** two instances attempt to migrate the same database concurrently
- **THEN** only the lock holder applies migrations and the other exits or waits without duplicating schema changes

#### Scenario: One account migration fails
- **WHEN** a SQLite account database fails migration or verification while other account migrations succeed
- **THEN** the failed account remains isolated with an actionable status and healthy migrated accounts are not rolled back or silently served through the old schema

### Requirement: Health and observability
The system SHALL expose distinct liveness, readiness, dependency, build/version, worker lag, and storage integrity signals plus structured logs, OpenTelemetry traces, and metrics with request/job correlation and secret redaction.

#### Scenario: Database becomes unavailable
- **WHEN** the server process remains alive but cannot perform its required database probe
- **THEN** liveness remains process-accurate, readiness fails, and the dependency signal identifies the database class without exposing credentials

### Requirement: Backup, restore, and verification
The project SHALL document and automate database-appropriate backups, raw/export bundle creation, encryption guidance, retention, restore to an isolated target, and post-restore verification of schema, counts, hashes, credentials, routing, and sample queries. SQLite backups SHALL include a checksummed backup-set manifest binding the management snapshot to each included account snapshot and its projection checkpoint.

#### Scenario: Backup drill completes
- **WHEN** an operator restores a backup into an isolated compatible release and runs verification
- **THEN** the command reports manifest/schema compatibility, authoritative observation integrity, and any missing external secrets before activation

### Requirement: Safe upgrades and rollback
Releases SHALL document supported source versions, migration duration/space expectations, backup prerequisites, rolling compatibility where available, post-upgrade verification, and rollback boundaries. Destructive cleanup SHALL be deferred beyond the first compatible release.

#### Scenario: Upgrade migration fails
- **WHEN** a schema or segment migration fails before completion
- **THEN** the system stops activation, records the recoverable failure state, preserves the pre-migration backup path, and provides a documented recovery command

### Requirement: Operational maintenance
The system SHALL provide administrator commands and scheduled jobs for per-file SQLite WAL checkpoints/integrity, orphaned/missing account database reconciliation, bounded pool and file-descriptor reporting, PostgreSQL partition horizon and index maintenance, compaction, rollup reconciliation, job retry/dead-letter handling, credential rotation, and storage capacity reporting.

#### Scenario: Future partition is missing
- **WHEN** PostgreSQL partition monitoring detects that the configured future horizon is below threshold
- **THEN** the system creates or reports the required partition before ingestion reaches the uncovered range

### Requirement: Scale and failure testing
The project SHALL include reproducible generators and tests for 25-year datasets, burst ingestion, concurrent chart queries, worker restarts, database interruptions, disk exhaustion, corrupt segments, webhook failures, backup/restore, and SQLite-to-PostgreSQL migration.

#### Scenario: Worker crashes during compaction
- **WHEN** a fault test terminates a worker at every compaction transition
- **THEN** recovery completes without lost raw data, duplicate query points, or unreconciled rollups
