# Certified capacity report

## PostgreSQL reference profile

The modeled large profile is 5,000 systems at five-minute cadence over 25 years:
13.14 billion observations before compaction. The reproducible reference report
records 286 observations/second for the burst workload, exceeding the required
250 observations/second, and records p50/p95/p99 for concurrent chart/statistics
requests. Use PostgreSQL 17, time partitions, BRIN plus ownership/time B-tree
indexes, a maintained partition horizon, 4 GiB shared buffers on the 16 GiB
reference host, bounded connections, and account-local job workers. The chart
regression harness separately enforces 30-day p95 below 500 ms and 25-year daily
p95 below 1,000 ms.

These are reference-fixture certification values, not a promise for arbitrary
hardware. Re-run both performance harnesses with production PostgreSQL settings,
storage, TLS, and representative channel density before setting an SLO.

## SQLite profile

SQLite is certified for self-hosted isolation, not parity with the 5,000-system
PostgreSQL profile. Benchmark account count, concurrent account writers,
per-account file size, open pool/file-descriptor ceiling, WAL checkpoint age,
projection lag, backup duration, and integrity/compaction maintenance. The
recommended reference ceiling is 100 simultaneously active account pools, one
serialized writer per account, 64 idle pools, and 80% disk-capacity alerting.

Plan migration to PostgreSQL when active pools approach the file-descriptor
budget, cross-account projections or checkpoints remain behind their objectives,
one account requires sustained concurrent writers, maintenance exceeds its
window, or fleet-wide analytics becomes operationally important. Export, dry-run
import, checksum verification, and reconciliation are mandatory migration gates.

## Measurement record

The machine-readable report records hardware, PostgreSQL settings, bytes per
system-day, compression ratio, queue lag, concurrency, throughput, and latency
samples in `tests/fixtures/performance/scale-workload-report.json`. Results must
be replaced, not hand-edited, when the schema, codec, planner, or reference
hardware changes.
