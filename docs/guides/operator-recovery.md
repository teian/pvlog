# Operator, backup, and recovery guide

## Deployment topology

SQLite uses `management.sqlite3` for instance state and opaque per-account files
for account data. PostgreSQL keeps account ownership in every key. Run migrations
under the built-in lock before starting upgraded servers; never copy live SQLite
files directly. Monitor readiness, job lag, storage integrity, WAL/checkpoints,
disk capacity, provider freshness, and dead letters.

## Backup and restore

An account backup includes a versioned manifest, checksums, configuration,
telemetry segments, correction overlays, and audit metadata. A full SQLite set
coordinates the management database and every active account checkpoint.
PostgreSQL deployments integrate logical/native backups with the same manifest.
Use encryption and retention hooks supplied by the deployment environment.

For SQLite, `pvlog export OUTPUT` takes online `VACUUM INTO` snapshots and writes
a coordinated checksum manifest; `--account-database OPAQUE_FILE` limits the set
for account transfer. For PostgreSQL, create a transactionally consistent custom
archive with the deployment's pinned `pg_dump`, then package and checksum it with
`pvlog export OUTPUT --postgres-archive ARCHIVE`. Run `pvlog verify BUNDLE`, an
isolated `pvlog import BUNDLE --dry-run`, and a test restore before retention.
Encryption-at-rest and retention are deliberate post-export/pre-delete hooks:
encrypt the entire immutable bundle, record the key identifier in the external
backup catalog, verify the encrypted copy, and delete only after policy approval.

Restore into an isolated destination, verify every checksum and schema version,
run `pvlog doctor`, storage integrity and projection reconciliation, then perform
sample raw/rollup queries before activation. Account transfer and
SQLite-to-PostgreSQL imports begin with dry run and are resumable. Drill both one
account and full-instance recovery on a schedule.

## Upgrade, rollback, and maintenance

Before upgrade, record free space, estimated migration duration, current build,
backup identifier, and rollback window. Migration lock contention or failure is
fatal and must leave the prior schema usable. After upgrade, verify health,
version, storage, queues, compaction, reconciliation, indexes/partitions, provider
caches, and credential rotation schedules. Defer destructive cleanup until the
rollback window closes.

Maintenance ownership is explicit: `pvlog migrate plan/status/apply` owns schema,
partition-horizon, and index changes; the continuous worker owns leased
compaction, rollup/projection reconciliation, provider-cache refresh, and bounded
dead-letter retries; `pvlog doctor` owns read-only schema/storage integrity; and
the account router owns bounded WAL checkpoints and idle-pool eviction. Inspect
dead letters before administrative replay. Rotate API credentials and connector
secret references through their scoped administration resources, never by
editing hashes. Capacity maintenance uses the health surfaces and the certified
profile thresholds; a failing integrity result produces a repair plan rather
than silent mutation.

Troubleshooting order: readiness problem details, correlated request/job ID,
secret-redacted logs and traces, database reachability, disk/WAL state, queue
lag/dead letters, corrupt-segment repair plan, then external provider/webhook
circuit state. Repair commands must not silently mutate unverifiable data.

## Observability baseline

Production logs are structured JSON and authorization/cookie headers are marked
sensitive. Preserve `x-request-id` through the reverse proxy and include the job
ID in worker events. When telemetry is enabled, PVLog exports HTTP request spans,
request counts, and duration histograms through provider-neutral OTLP/HTTP. The
browser exporter is independently disabled by default and must target the same
trusted collector without embedding collector credentials in source.

Dashboards should chart request/error latency, ingestion rejection/backpressure,
job and projection lag, webhook/provider circuit state, compaction throughput,
WAL/checkpoint age, database/disk capacity, and integrity results. Alert on
readiness loss, sustained p95 regression, exhausted capacity, dead-letter growth,
stale backups, and failed integrity checks; avoid labels containing users,
tokens, request bodies, or high-cardinality observation IDs.
