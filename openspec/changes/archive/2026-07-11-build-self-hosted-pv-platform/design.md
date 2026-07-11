## Context

The repository currently contains project guidance and an OpenSpec configuration but no application implementation or baseline specs. The new system serves operators and API clients through a versioned JSON API with precise schemas, strong errors, and discoverable documentation.

The largest technical constraint is time-series longevity. A reference deployment of 5,000 systems reporting every five minutes produces about 525.6 million observations per year and 13.14 billion observations over 25 years. Bursty uploads, corrections, extended values, daily statistics, and indexes add substantial overhead. Keeping every observation as a permanently indexed relational row would make storage, vacuuming, backups, and range scans unnecessarily expensive, especially on SQLite.

The implementation must comply with the repository rules: Rust changes build with zero `cargo check` warnings; backend identity integration remains OIDC/OAuth-provider-neutral; frontend code follows Feature-Sliced Design with React/Vite, native `fetch`, TanStack Query, shadcn/ui, strict TypeScript, local assets, English and German localization, and WCAG 2.1 AA. The repository layout is fixed: production backend code lives beneath `src/crates/`, production UI code beneath `src/ui/`, and all test code and fixtures beneath root `tests/`.

## Goals / Non-Goals

**Goals:**

- Provide complete photovoltaic ingestion, query, administration, provider, and notification functionality through a modern, consistent `/api/v1` contract.
- Provide a useful web product rather than an API-only data sink.
- Retain exact accepted measurements for at least 25 years and make common chart/statistics queries independent of total raw history size.
- Certify a SQLite profile with a management catalog and isolated per-account data databases, plus a PostgreSQL scale profile behind the same domain contract and export format.
- Make operations, security, and API behavior observable and testable.
- Keep the first implementation deployable as a modular monolith with an independently runnable worker, avoiding distributed-systems overhead.

**Non-Goals:**

- Wire compatibility with PVOutput or any other hosted service, including legacy routes, credentials, request formats, and response formats.
- Forecasting, automated energy trading, inverter control, or safety-critical plant control in the initial release.
- Transparent active-active multi-region writes or unlimited horizontal scale.
- Identical performance from SQLite and PostgreSQL; PostgreSQL is the certified profile for thousands of systems.

## Decisions

### 1. Use a modular Rust monolith with explicit boundaries

The backend will be a Cargo workspace with domain, application, storage, HTTP API, and worker modules/crates. All workspace crates and their production Rust source live under `src/crates/`. The React/Vite project treats `src/ui/` itself as its source root: Feature-Sliced Design layers, entrypoints, TypeScript, CSS, and UI assets live directly beneath it, with no nested `src/ui/src/` directory. Axum/Tokio provides HTTP and async execution; Tower middleware owns request IDs, limits, timeouts, compression, tracing, CORS, and security headers. A server process and worker process may run from the same versioned binary using separate subcommands.

The initial structure is:

```text
src/
├── crates/
│   ├── domain/
│   ├── application/
│   ├── storage/
│   ├── api/
│   ├── worker/
│   └── pvlog/
└── ui/
    ├── app/
    ├── pages/
    ├── widgets/
    ├── features/
    ├── entities/
    ├── shared/
    ├── main.tsx
    └── index.css
tests/
├── rust/
├── ui/
├── contract/
├── e2e/
├── performance/
├── fixtures/
└── support/
```

Vite, TypeScript, Tailwind, ESLint, imports, and the `@/` alias are configured with `src/ui/` as the frontend source root. No `src/ui/src/` directory is created. No `#[cfg(test)]` modules, co-located `*.test.*` files, crate-local `tests/` folders, fixtures, mocks, fake services, or other test-only source are placed under `src/`. Rust test harness packages under `tests/rust/` are workspace members and depend on the production crates through their public APIs. Vitest, Playwright, contract, end-to-end, and performance tooling are configured to discover their code under the appropriate root `tests/` subtree. Shared test-only code belongs under `tests/support/`; production code must not depend on it.

Domain types and use cases must not depend on Axum, SQLx, third-party wire formats, or a specific identity provider. The modern API calls canonical application services so validation and authorization remain centralized. Background jobs use a database-backed queue with leases, bounded retries, idempotent handlers, and a dead-letter state; no external broker is required initially.

Alternative considered: microservices per API area. Rejected because atomic ingestion/aggregation, dual-database testing, and self-host deployment are simpler in one process boundary; modules can be extracted later from measured pressure.

### 2. Expose one modern API over the canonical domain model

The router is mounted at `/api/v1`. It uses plural resources, JSON, RFC 3339 timestamps, explicit units, cursor pagination, bounded date ranges, sparse field/include controls where useful, `Idempotency-Key` on retried writes, ETags for mutable resources, and RFC 9457 problem details. API versions are path-major; additive fields are permitted, while removals or semantic breaks require a new major version and migration period.

Every identifier generated by PVLog for an entity, event, job, session, audit record, or request correlation uses UUID version 7. UUIDv7 values are serialized as canonical lowercase hyphenated strings in the modern API and stored using native UUID/binary representations rather than arbitrary text where the database supports them. Their time ordering improves index locality but is not an authorization boundary and clients must treat them as opaque. External provider subjects, caller-supplied idempotency keys, and imported source identifiers retain their native formats and map to separate internal UUIDv7 identifiers.

Alternative considered: provide a third-party compatibility adapter. Rejected because it would preserve legacy limitations, broaden the security surface, and make release quality depend on an external protocol.

### 3. Treat the committed OpenAPI 3.1 document as a release artifact

`openapi/pvlog-v1.yaml` will describe every modern operation, security scheme, parameter, schema, error, webhook, example, and deprecation. Rust handlers and DTOs remain the implementation source, but CI generates a candidate specification from route/DTO metadata, normalizes it, and fails on an unexplained diff against the committed contract. Route coverage tests ensure every Axum modern route has an operation ID and every documented operation is mounted.

The documentation site will render the committed contract and combine it with quickstarts, authentication, ingestion, querying/charting, pagination/errors, webhooks, deployment, backup, and functional coverage guides. Examples are tested against an ephemeral server.

Alternative considered: handwritten OpenAPI with no generated comparison. Rejected because it gives better prose initially but predictably drifts from the Rust implementation.

### 4. Use a hot-row, immutable-segment, and rollup storage model

Canonical measurements use integer base units (watts, watt-hours, millivolts, milli-degrees, basis points) and UTC epoch milliseconds, with the system's IANA timezone retained for local-day semantics. Each accepted observation has provenance, quality flags, receive time, and a deterministic uniqueness key. Extended values use a registered channel definition rather than untyped numbered columns in the modern domain.

Recent and mutable observations live as indexed rows in `telemetry_hot`, partitioned by time on PostgreSQL and indexed by `(system_id, measured_at)` on both databases. After a configurable lateness window (default 35 days), a worker compacts one system-day into an immutable, versioned Protocol Buffers columnar segment compressed with Zstandard. Segment rows store time bounds, field presence, count, version, compressed/uncompressed lengths, and a cryptographic content hash. The format version and migration reader preserve long-term decodability.

Corrections to archived data are written as small overlay rows, immediately visible to reads, then folded into a replacement segment by an idempotent compaction job. Raw points are deleted from the hot table only after the segment, rollups, counts, and hashes verify in one recoverable state transition. No accepted raw value is discarded by aggregation.

Separate rollup tables store 15-minute, hourly, daily, monthly, and yearly aggregates with count, sum, min, max, first/last, and quality coverage. Daily/lifetime system summaries support overview pages. Query planning selects raw/segment data only when the requested resolution needs it and otherwise reads the coarsest rollup below the requested point budget. Rollups are reproducible from raw segments and are not the system of record.

Alternative considered: permanent row-per-observation storage. Rejected because multi-billion-row indexes and backups are avoidable for the stated retention horizon. TimescaleDB was also rejected as a required dependency because it would violate plain PostgreSQL portability and provide no SQLite path.

### 5. Split SQLite into a management catalog and one data database per account

An account is the tenancy, authorization, export, backup, and SQLite storage boundary. Every PV system belongs to exactly one account, while a user may belong to multiple accounts. The SQLite profile SHALL use an instance-wide `management.sqlite3` plus one opaque, non-user-named database file per account under a managed data directory.

The management database stores users, local password credentials and recovery state, external auth connectors and identity links, sessions, RBAC roles/permissions/assignments, accounts, memberships, API credential hashes/scopes, quotas, global configuration, account-database routing and schema state, provisioning/deprovisioning state, and global security audit events. It does not store raw telemetry. Each account database stores that account's systems, equipment, tariffs, extended channels, hot observations, archived segments, corrections, rollups, summaries, account-local jobs, alerts, webhooks, imports/exports, and account-local audit history.

Globally unique system identifiers are registered in the management catalog so authentication and routing complete before opening account data. Database paths are derived only from opaque registry values beneath the configured data root; names supplied by users never become paths. The process maintains a bounded LRU of account connection pools, applies per-account concurrency limits, and closes idle pools so large account counts do not exhaust file descriptors. WAL, foreign keys, busy timeout, checkpoints, and integrity checks are configured independently for every SQLite file. Writers are serialized within an account but different accounts can write concurrently.

Account provisioning uses a recoverable state machine: reserve the account in management, create and migrate a temporary data database, verify it, atomically move it to its final opaque path, then mark routing active. Deprovisioning first disables routing and quarantines the file before retention-aware deletion. Startup reconciles incomplete states and orphaned files without guessing ownership.

SQLite cannot provide atomic transactions across the management and account databases, so cross-boundary changes use explicit reservations and transactional outbox/inbox records.

Management and account schemas have independent versions. Upgrade tooling migrates the management database first using backward-compatible routing metadata, then migrates account files with bounded parallelism and per-account status. An account with a failed migration remains unavailable and diagnosable without corrupting or silently serving an old schema; policy determines whether instance readiness tolerates isolated account failures.

SQLx-backed repository interfaces still have PostgreSQL and SQLite implementations with shared contract tests. PostgreSQL keeps logically separated management and account-owned tables in one database, includes `account_id` in every owned key/index, and uses native range partitions for hot data and rollups, B-tree indexes for system/time access, and BRIN indexes where append ranges benefit. PostgreSQL-specific scaling stays inside its adapter.

The SQLite profile targets self-hosting across many independently active accounts while retaining a single-writer limit within each account database. The PostgreSQL profile remains the required certified configuration for the reference scale of 5,000 systems at five-minute cadence, 25-year retention, burst ingestion of at least 250 observations/second, and concurrent chart traffic. SQLite benchmarks additionally report account count, active pool count, file descriptors, concurrent account writers, per-account size, checkpoint behavior, and management-projection lag; documentation will not imply that SQLite meets the PostgreSQL scale profile.

Backups use SQLite's online backup API for a consistent management snapshot and each account file, recorded in a signed/checksummed backup-set manifest with account ID, schema version, sequence/checkpoint, and file hash. Account-scoped export bundles remain the supported portability unit and contain manifest/schema versions, system metadata, credentials excluded by default, raw segment payloads, corrections, and checksums. They support single-account restore, account transfer, SQLite-to-PostgreSQL migration, and disaster recovery without requiring unrelated accounts to move together.

Alternatives considered: one monolithic SQLite database was rejected because all accounts would contend on one writer and share one corruption/backup blast radius; one database per PV system was rejected because it makes account-wide queries and lifecycle operations unnecessarily expensive. SQLx `Any` and one query set was rejected because lowest-common-denominator SQL would hide important PostgreSQL scaling features while still failing on dialect details.

### 6. Make ingestion deterministic, bounded, and recoverable

All ingestion paths normalize into one command containing system, timestamp, fields, source, idempotency identity, and caller. Validation checks ownership, timestamps, physical/configured limits, mutually dependent fields, cumulative/net semantics, and channel definitions. Batches validate per item, enforce configured item/body limits, and provide a documented atomic mode plus a partial mode with item results.

An accepted uniqueness key makes retries return the prior outcome without double counting. A conflicting payload for the same key returns a conflict unless explicitly submitted as a correction with sufficient scope. Ingestion commits canonical data and an aggregation invalidation/job in the same transaction. Workers recalculate affected rollups and summaries, so late data and corrections converge.

Backpressure is explicit: request size and concurrency are bounded, overload returns a retryable problem with `Retry-After`, and metrics distinguish validation failures, duplicates, corrections, and queue lag.

### 7. Unify local authentication, external connectors, RBAC, and server-side sessions

Every human identity resolves to one local user record, regardless of whether the user authenticates with a local password or one of several external connectors. Local authentication is a complete supported mode rather than a bootstrap-only fallback: administrators can create, invite, disable, unlock, and delete users; instance policy controls self-registration, email verification, invitation expiry, password rules, and recovery. Passwords use Argon2id with versioned parameters, breached/common-password policy hooks, rate limiting, and single-use expiring reset tokens stored only as hashes.

External login is handled as a backend-for-frontend flow. The backend persists protocol-neutral connector records and supports multiple concurrently enabled OpenID Connect and OAuth 2.0 Authorization Code connectors. OIDC uses discovery where configured, issuer/audience/signature/time validation, `state`, `nonce`, and PKCE. Generic OAuth2 connectors use configured authorization, token, and user-info endpoints plus normalized subject/name/email/avatar claim mappings, `state`, and PKCE. Provider access/refresh tokens stay in encrypted server-side storage only when required and are never exposed to the SPA.

Google, GitHub, Facebook, and X are delivered as administrator-facing setup presets and conformance-tested examples that populate the generic OIDC/OAuth2 connector model. Backend settings, DTOs, health keys, services, and modules remain named for protocol behavior (`oidc`, `oauth2`, `issuer`, endpoints, scopes, and claim mappings), never for a vendor. Preset labels/icons/default endpoints and provider-specific setup guidance stay at the UI/documentation/configuration-seed boundary. If a provider requires nonstandard behavior, it is isolated behind an adapter named for that protocol behavior rather than the vendor.

External identities are unique by connector plus immutable provider subject. Linking an external identity to an existing local user requires an authenticated session and recent reauthentication. Email matching alone never silently links accounts unless the email is verified by the connector and an administrator explicitly enables that policy. Unlinking cannot remove the user's last viable login method, and connector disablement does not delete local users or their RBAC assignments.

Authorization uses deny-by-default hierarchical RBAC. Instance roles control global administration and audit access; account roles control account management; system roles control viewing and telemetry actions. Built-in owner, administrator, manager, contributor, viewer, and auditor roles map to explicit permissions, and authorized account administrators can define constrained custom roles without granting permissions they do not possess. Application services evaluate the effective permission set before storage routing or mutation, so local and external login methods receive identical authorization.

All interactive methods create revocable server-side sessions using rotated secure `HttpOnly`/`SameSite` cookies, absolute and idle expiry, logout/revocation, concurrent-session policy, and CSRF protection. The frontend receives only normalized local user, connector display metadata, session, and authorization information; it contains no provider tokens or vendor-specific authentication logic.

Modern API tokens are random, shown once, stored only as keyed hashes, scoped by action/account/system, optionally expiring, and revocable. Legacy API keys map to the same principal and authorization policy while accepting `X-Pvoutput-Apikey` and `X-Pvoutput-SystemId`. Security-relevant authentication, linking, RBAC, session, and credential mutations append audit records with actor, target, action, request ID, timestamp, and safe metadata.

Alternatives considered: JWT-only stateless browser sessions were rejected because revocation, logout, provider token isolation, and self-hosted administrative control are safer with server-side sessions. Vendor-specific backend login modules were rejected because they duplicate security-sensitive flows and violate provider neutrality; standards-based connectors with data-driven presets provide the required provider coverage with a smaller audit surface.

### 8. Keep integrations optional and isolate unreliable networks

Insolation, regional supply, OIDC/OAuth2 login, webhooks, and telemetry export use adapter interfaces with timeouts, circuit breakers, bounded retries, caches, and health reporting. Core telemetry ingest/query and enabled local authentication continue when optional providers fail. Regional data and insolation records store provider, license/provenance, fetch time, and validity window.

Webhook endpoints must be HTTPS by default, pass SSRF protections, and resolve against blocked private/link-local ranges unless an administrator explicitly allows local delivery. Deliveries are signed, replay-identifiable, observable, and retried with exponential backoff and dead-letter inspection.

### 9. Build a dense, accessible data application

The React/Vite SPA uses the repository's Feature-Sliced Design layers and consumes only the modern REST API through native `fetch` wrapped in TanStack Query. OpenAPI-derived types help author clients, but all external responses are validated with Zod. Zustand stores ephemeral UI state only. Runtime deployment config is loaded from `/runtime-config.json`; browser tracing is centralized and disabled by default.

Chart endpoints accept a desired maximum point count and return the selected resolution, coverage, gaps, and series units. The UI must not request millions of raw points to draw a fixed-width chart. Charts include non-color cues, keyboard-accessible summaries/data tables, localized dates/units, and export links. English and German ship together, all fonts/assets are local, and light/dark themes consume semantic design tokens.

### 10. Package one observable, upgradeable self-hosted system

Container images run unprivileged and provide `server`, `worker`, `migrate`, `doctor`, `export`, `import`, and `verify` commands. Compose examples use documented `.env.example` values and provider-neutral OIDC/OAuth2 names. Schema migrations take an advisory/lease lock and are never silently run by every replica. Startup, liveness, readiness, dependency, job-lag, and build/version endpoints have distinct semantics.

OpenTelemetry traces/metrics and structured JSON logs include request/job IDs without credentials or raw secrets. Operators receive dashboards/alerts for error rate, latency, ingest lag, compaction lag, webhook failures, database size, partition horizon, backup age, and integrity verification.

## Risks / Trade-offs

- **[Segment format longevity]** Opaque compressed blobs cannot be queried directly with SQL and risk decoder obsolescence. → Use a versioned, widely specified protobuf envelope, retain migration readers, verify hashes, expose export tools, and continuously test old fixtures.
- **[Correction complexity]** Late corrections can make raw segments and rollups diverge. → Use overlay visibility, transactional invalidation, generation numbers, idempotent rebuilds, and reconciliation jobs that compare counts/hashes.
- **[Dual database divergence]** SQLite and PostgreSQL semantics differ in locking, types, and query plans. → Maintain separate adapters/migrations, run identical contract suites, and publish separate certified capacity profiles.
- **[Cross-database consistency]** SQLite management and account databases cannot commit atomically together. → Use recoverable provisioning states, account-local transactional outboxes, idempotent management projections, sequence checkpoints, and reconciliation tooling.
- **[Many SQLite files]** Large account counts can exhaust file descriptors, slow startup/migrations, and complicate backups. → Use opaque paths, bounded lazy connection pools, bounded migration/backup concurrency, per-account health state, and manifest-driven operations.
- **[Nonstandard centralized tests]** Keeping all Rust and frontend tests outside their source packages reduces access to private implementation details and requires explicit test discovery. → Test stable public behavior, use root test harness packages/configuration, keep reusable fakes in `tests/support/`, and enforce the boundary in CI.
- **[External identity takeover]** Automatic linking by mutable or unverified email can merge an attacker's identity into an existing user. → Key identities by connector/subject, require explicit authenticated linking and recent reauthentication, make email auto-link opt-in and verified-only, and audit every link change.
- **[Provider behavior drift]** Social providers can change scopes, endpoints, claims, review requirements, or user-info availability. → Keep protocol adapters generic, version the preset catalog, test each supported preset against current provider sandboxes/documentation before release, and show connector health/configuration diagnostics.
- **[Scale estimate uncertainty]** Extended-channel density and compression vary greatly by fleet. → Benchmark multiple realistic/synthetic distributions and size storage from measured bytes per system-day with safety margins.
- **[External data licensing/availability]** Regional supply and insolation providers may not cover all regions or permit redistribution. → Keep providers pluggable, record provenance, allow administrator-supplied sources, and show unavailable rather than fabricate data.
- **[Large initial scope]** Full API, UI, and operations can delay a useful release. → Implement in vertical slices with acceptance gates: foundation, ingest/query, durable storage, product UI, integrations, then scale certification.
- **[Security of self-host defaults]** Easy deployment can encourage weak public exposure. → Default private, require explicit secret/bootstrap setup, ship restrictive CORS/CSP/cookies, scan images/dependencies, and document reverse-proxy/TLS boundaries.

## Migration Plan

1. Establish the `src/crates/`, `src/ui/`, and root `tests/` layout, workspace and test-harness configuration, configuration model, CI quality gates, empty OpenAPI contract, database adapters, separate SQLite management/account migration runners, account provisioning/routing, auth skeleton, and deployment smoke test.
2. Implement account and system management plus a modern end-to-end telemetry slice on PostgreSQL and routed SQLite account databases, including raw hot rows, chart queries, and OpenAPI examples.
3. Add segments, corrections, rollups, integrity verification, import/export, backup/restore, and synthetic scale tooling; keep compaction feature-gated until reconciliation tests pass.
4. Complete the modern API for notifications, optional data providers, and every canonical application capability.
5. Deliver the web onboarding, dashboards, charts, data quality, settings, and admin operations, followed by notifications and optional data providers.
6. Run long-horizon capacity and failure testing, publish the certified profiles and feature coverage report, and cut the first stable release only when conformance, recovery, and warning-free checks pass.

For rollback, application releases remain backward-compatible with the previous management and account schemas during a documented window. Destructive schema cleanup is deferred to later releases. Segment compaction can be disabled without losing hot or already archived data; account-scoped export and manifest-driven database-native backups are verified before upgrade. A failed management migration stops startup, while a failed account migration isolates that account and leaves a recorded state for operator recovery.

## Open Questions

- Which regional supply and insolation datasets can be enabled by default under licenses compatible with this repository? Until resolved, adapters and administrator configuration are implemented without bundling restricted data.
- What exact hardware defines the published PostgreSQL scale profile? The benchmark suite will ship first; release certification must record a reproducible reference machine and storage class.
- Should public system discovery be globally disabled or merely private-by-default? The design defaults to private and gives administrators a global disable switch pending product feedback.
