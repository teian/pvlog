## ADDED Requirements

### Requirement: Exact multi-decade retention
The system SHALL preserve every accepted canonical raw observation and accepted correction for at least 25 years unless an authorized retention or erasure policy explicitly removes it. Rollup creation SHALL NOT replace the authoritative raw values.

#### Scenario: Old raw interval is queried
- **WHEN** an authorized user requests exact observations from a system-day 25 years in the past
- **THEN** the system reconstructs the accepted canonical observations, including corrections and quality metadata

### Requirement: Tiered time-series representation
The system SHALL keep recent mutable observations in indexed hot rows, compact stable system-day data into immutable versioned compressed segments, preserve later corrections as immediately visible overlays, and maintain reproducible aggregate rollups.

#### Scenario: Stable day is compacted
- **WHEN** a system-day is older than the configured lateness window and has no active ingestion transaction
- **THEN** the worker writes and verifies a checksummed segment and required rollups before removing redundant hot rows

#### Scenario: Compaction is interrupted
- **WHEN** a worker stops between segment creation and hot-row cleanup
- **THEN** the next worker run resumes idempotently without losing or double-returning observations

### Requirement: Portable measurement encoding
Archived segments SHALL use a documented, versioned, deterministic encoding with UTC timestamp deltas, typed nullable columns, unit scales, provenance and quality data, Zstandard compression, uncompressed length, and a cryptographic content hash. Readers SHALL retain support or an explicit migration path for every released segment version.

#### Scenario: Segment corruption is detected
- **WHEN** a stored segment's bytes do not match its length or content hash
- **THEN** verification marks the segment unhealthy, excludes silent use of corrupt values, and raises an operator-visible integrity failure

### Requirement: SQLite and PostgreSQL parity
The system SHALL implement SQLite and PostgreSQL repositories that pass the same behavioral contract for transactions within an account boundary, uniqueness, authorization-relevant reads, ingestion, corrections, rollups, export, and restore.

#### Scenario: Storage contract suite runs
- **WHEN** the shared storage conformance suite executes against a fresh SQLite management database with multiple account databases and against a fresh PostgreSQL database
- **THEN** both adapters produce equivalent canonical outcomes for every required case

### Requirement: SQLite management and account database separation
The SQLite backend SHALL store instance identity, accounts, memberships, credential routing, global configuration, database registry/state, and privacy-safe cross-account projections in one management database, and SHALL store each account's systems, telemetry, rollups, alerts, integrations, and local jobs in a separate opaque-path account database. Raw telemetry from one account SHALL NOT be stored in the management database or another account's database.

#### Scenario: Two accounts ingest concurrently
- **WHEN** independent clients ingest valid readings for systems in two different accounts
- **THEN** each write is routed to its owning account database and the accounts do not contend on one shared SQLite writer lock

#### Scenario: Account database is unavailable
- **WHEN** one account database cannot be opened or passes neither schema nor integrity checks
- **THEN** that account is marked unavailable while the management plane and healthy accounts continue according to readiness policy

### Requirement: Recoverable cross-database coordination
Operations spanning the SQLite management and account databases SHALL use explicit state machines, transactional outbox/inbox events, idempotent projections, sequence checkpoints, and reconciliation instead of assuming cross-database atomic transactions.

#### Scenario: Process stops during account provisioning
- **WHEN** the process stops after reserving an account but before activating its verified data database
- **THEN** startup reconciliation resumes or safely rolls back provisioning without routing requests to a partial database

#### Scenario: Projection worker repeats an event
- **WHEN** a committed account projection event is delivered to the management database more than once
- **THEN** sequence/idempotency checks apply it exactly once to the privacy-safe projection

### Requirement: Certified scale profile
The PostgreSQL deployment profile SHALL be load-tested for at least 5,000 systems reporting at five-minute intervals, a modeled 25-year history of 13.14 billion observations, ingestion bursts of at least 250 observations per second, and concurrent chart/statistics traffic. Published results SHALL identify hardware, configuration, generated data distribution, compression, database size, and latency percentiles.

#### Scenario: Scale certification is published
- **WHEN** a release is declared suitable for the reference scale
- **THEN** its reproducible benchmark report demonstrates no data loss, bounded queue lag, integrity reconciliation, and the specified query service objectives

### Requirement: Database maintenance and portability
The system SHALL provide independent management/account schema migrations, integrity verification, compaction/rebuild, manifest-driven database-native backup guidance, account-scoped restore, and a database-independent checksummed export/import path between supported storage engines.

#### Scenario: SQLite data is migrated to PostgreSQL
- **WHEN** an operator exports a verified SQLite instance and imports it into a compatible PostgreSQL release
- **THEN** system metadata and authoritative observation hashes/counts match before the PostgreSQL instance is activated

#### Scenario: One SQLite account is restored
- **WHEN** an operator restores an account database and its matching management metadata from a verified backup-set manifest
- **THEN** routing activates only after account ID, schema version, projection checkpoint, file hash, and authoritative data checks pass
