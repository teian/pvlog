## 1. Repository and Quality Foundation

- [x] 1.1 Create the Cargo workspace with all production crates under `src/crates/` for domain, application, storage, HTTP API, PVOutput compatibility, workers, and the executable entrypoint.
- [x] 1.2 Add pinned Rust dependencies for Axum/Tokio, Tower, Serde, validation, SQLx SQLite/PostgreSQL, tracing/OpenTelemetry, cryptography, Protobuf, Zstandard, and OpenAPI generation with dependency policy checks.
- [x] 1.3 Configure formatting, Clippy, tests, `cargo check` with zero warnings, dependency/security/license auditing, and CI caching for all Rust feature/database combinations.
- [x] 1.4 Implement typed provider-neutral runtime configuration, secret redaction, production safety validation, and configuration unit tests.
- [x] 1.5 Create the React/Vite/pnpm strict-TypeScript frontend with `src/ui/` as the source root, FSD layers directly under it, no nested `src/ui/src/`, Tailwind v4, shadcn/ui, native fetch, TanStack Query, Zod, Zustand, i18next, and test/lint tooling.
- [x] 1.6 Add English/German localization bootstrap, semantic light/dark tokens, locally packaged Noto fonts, runtime config loading, and browser telemetry bootstrap disabled by default.
- [x] 1.7 Create a minimal valid `openapi/pvlog-v1.yaml`, local documentation renderer shell, OpenAPI linting, and generated-versus-committed contract comparison tooling.
- [x] 1.8 Add developer commands and root `tests/` CI smoke tests that boot the server and worker against an ephemeral SQLite management database with multiple account databases and an ephemeral PostgreSQL database.
- [x] 1.9 Create centralized `tests/rust`, `tests/ui`, `tests/contract`, `tests/compatibility`, `tests/e2e`, `tests/performance`, `tests/fixtures`, and `tests/support` harnesses and configure Cargo, Vitest, Playwright, and CI discovery.
- [x] 1.10 Add a repository-structure check that rejects production backend code outside `src/crates/`, production UI code outside `src/ui/`, a nested `src/ui/src/` source root, test-only code outside root `tests/`, and production dependencies on `tests/`.

## 2. Domain Model and Application Boundaries

- [x] 2.1 Define strongly typed UUIDv7 identifiers with strict parse/deserialize validation, UTC timestamps, IANA timezones, integer unit types, money, visibility, quality flags, and validation errors with boundary tests.
- [x] 2.2 Model local users, password/recovery state, external connector identities, sessions, hierarchical RBAC roles/permissions/assignments, accounts, memberships, API credentials, scopes, quotas, storage routing state, and audit events without Axum, SQLx, or provider-specific dependencies.
- [x] 2.3 Model PV systems, effective-dated capacity/equipment, tariffs, calculation modes, privacy, lifecycle state, and extended channel definitions.
- [x] 2.4 Model canonical observations, cumulative/net/battery semantics, sources, idempotency identities, corrections, segment metadata, rollups, and coverage.
- [x] 2.5 Model teams, favourites, ranking eligibility, alert rules/events, webhook subscriptions/deliveries, providers, jobs, imports, and exports.
- [x] 2.6 Define repository, clock, credential, identity, webhook, insolation, supply, job queue, and transactional unit-of-work interfaces with in-memory fakes.
- [x] 2.7 Implement deny-by-default RBAC evaluation for built-in and constrained custom roles at instance, account, and system scope and test delegation, privilege-escalation, role/scope, and cross-account isolation matrices.

## 3. Database Schemas and Repository Contracts

- [x] 3.1 Implement migration discovery, checksums, status/plan/apply commands, PostgreSQL advisory locking, SQLite management/account lease locking, and incompatible-schema startup checks.
- [x] 3.2 Create SQLite management migrations for local users, Argon2id credentials, invitations/recovery, protocol-neutral auth connectors, external identities, encrypted provider token state, sessions, RBAC roles/permissions/assignments, accounts, memberships, API credential hashes/scopes, quotas, routing/schema state, provisioning, global audit, and privacy-safe projections.
- [x] 3.3 Implement recoverable SQLite account provisioning/deprovisioning with temporary creation, migration, integrity verification, opaque managed paths, atomic activation/quarantine, and orphan reconciliation.
- [x] 3.4 Create SQLite account-database migrations for systems, equipment, tariffs, channels, account audit, imports/exports, alerts, webhooks, providers, and account-local jobs.
- [x] 3.5 Create SQLite account and PostgreSQL migrations for hot telemetry, archived segment payloads, correction overlays, idempotency, rollups, summaries, invalidations, and data quality.
- [x] 3.6 Create PostgreSQL management/account-owned migrations with `account_id` in all owned keys and constraints plus schemas for teams, projections, jobs, and integrations.
- [x] 3.7 Add PostgreSQL time partitions, partition-horizon management, B-tree/BRIN indexes, constraints, and query-plan fixtures for telemetry and rollups.
- [x] 3.8 Implement bounded lazy SQLite account connection-pool routing, opaque path validation, per-account WAL/foreign keys/busy timeout/writer serialization, idle pool eviction, checkpoints, and integrity probes.
- [x] 3.9 Implement account-local transactional outbox, management inbox/projections, sequence checkpoints, idempotent replay, privacy-first invalidation, and reconciliation tests.
- [x] 3.10 Implement user/account/session/credential/membership/routing/global-audit repositories and run shared authorization and isolation contract tests.
- [x] 3.11 Implement system/equipment/tariff/channel/account-audit repositories for routed SQLite account databases and PostgreSQL with shared effective-date tests.
- [x] 3.12 Implement hot telemetry/idempotency/correction repositories for routed SQLite account databases and PostgreSQL with shared transaction, uniqueness, and range-query tests.
- [x] 3.13 Implement rollup/summary, team/community projection, alerts/webhooks, provider, and job repositories for both engines with shared contract tests.

## 4. Authentication, Authorization, and Audit

- [x] 4.1 Implement local user administration, invitation, optional self-registration, activation/email policy, disable/unlock/delete, and enumeration-resistant lifecycle endpoints.
- [x] 4.2 Implement Argon2id password verification/rehash, password change, single-use hashed recovery tokens, configurable password policy hooks, and brute-force/rate-limit controls.
- [ ] 4.3 Implement built-in and constrained custom RBAC role CRUD, permission assignment/delegation checks, effective-permission calculation, and privilege-escalation tests.
- [ ] 4.4 Implement multiple provider-neutral OIDC connectors with discovery, authorization callback, issuer/audience/signature/time validation, state, nonce, PKCE, secret references, and connector health tests.
- [ ] 4.5 Implement generic OAuth2 Authorization Code connectors with configured endpoints, state/PKCE, normalized user-info subject/name/email/avatar mappings, encrypted server-side token handling, and fake-provider tests.
- [ ] 4.6 Add versioned administrator-facing Google, GitHub, Facebook, and X preset definitions, setup validation, current-provider UI/configuration-catalog conformance tests under `tests/ui/`, and display metadata without vendor-named backend DTOs/services/settings/tests.
- [ ] 4.7 Implement just-in-time local user provisioning plus explicit external identity link/unlink with connector-subject uniqueness, recent reauthentication, verified-email opt-in policy, last-login protection, and takeover tests.
- [ ] 4.8 Implement secure browser session cookies, rotation, CSRF protection, idle/absolute expiry, concurrent-session policy, logout/revocation, and browser-focused security tests for every login method.
- [ ] 4.9 Implement one-time display and keyed-hash storage for modern API tokens scoped by action/account/system, including expiry, rotation, revocation, and constant-time verification.
- [ ] 4.10 Implement per-system legacy PVOutput read-only/read-write keys and header/query authentication mapped to canonical principals and RBAC permissions.
- [ ] 4.11 Implement authorization before account-database routing and append-only management/account audit recording for login, linking, RBAC, session, credential, destructive, import/export, and administrative events.
- [ ] 4.12 Implement configurable principal quotas and rate limiting with modern metadata, legacy opt-in headers, retry timing, and deterministic tests.

## 5. System Management and Modern Resource API

- [ ] 5.1 Implement system create/read/update/archive/restore/delete use cases with safe defaults, optimistic concurrency, audit, and domain tests.
- [ ] 5.2 Implement effective-dated equipment/capacity, tariffs, calculation settings, privacy, and extended-channel use cases with validation tests.
- [ ] 5.3 Add `/api/v1` Axum routing, content negotiation, request IDs, body/concurrency/time limits, compression, CORS/CSP/security headers, and RFC 9457 problem middleware.
- [ ] 5.4 Implement modern system, equipment, tariff, channel, membership, and credential endpoints with ETags, scopes, examples, and integration tests on both databases.
- [ ] 5.5 Implement deterministic filter/sort/cursor pagination primitives and contract tests for concurrent inserts, changed filters, expiry, and invalid cursors.
- [ ] 5.6 Implement dry-run/commit metadata import and asynchronous checksummed system export resources with permissions, expiry, and integrity tests.
- [ ] 5.7 Expand OpenAPI schemas, security requirements, examples, errors, and operation IDs for every completed system-management route and pass route coverage checks.

## 6. Canonical Telemetry Ingestion

- [ ] 6.1 Implement canonical ingestion commands that normalize explicit units, timestamps, source/provenance, quality flags, battery fields, and registered extended channels.
- [ ] 6.2 Implement physical/configuration validation, dependent-field rules, net/cumulative modes, counter reset/rollover handling, and table-driven edge-case tests.
- [ ] 6.3 Implement transactional single-observation insertion with uniqueness, aggregation invalidation, audit context, routed account ownership, and identical SQLite/PostgreSQL outcomes.
- [ ] 6.4 Implement idempotency-key persistence/replay/conflict behavior with expiry policy and concurrent-request tests.
- [ ] 6.5 Implement bounded batch ingestion with atomic and partial modes, stable indexed outcomes, request limits, and rollback tests.
- [ ] 6.6 Implement correction and deletion commands with optimistic concurrency, hot-row updates, archived overlays, immediate merged visibility, and rebuild invalidation.
- [ ] 6.7 Implement ingestion backpressure, overload problems, `Retry-After`, queue-lag gates, and saturation metrics with load-oriented integration tests.
- [ ] 6.8 Implement modern single, batch, correction, and delete telemetry endpoints and complete their OpenAPI contracts and executable examples.

## 7. Durable Segments, Rollups, and Reconciliation

- [ ] 7.1 Define and document the versioned Protobuf columnar segment envelope, deterministic timestamp/value encoding, Zstandard settings, lengths, and content hashes.
- [ ] 7.2 Implement segment encode/decode/version dispatch with golden fixtures under `tests/fixtures/`, sparse/extended values, corruption detection, deterministic bytes, and fuzz/property tests under `tests/rust/`.
- [ ] 7.3 Implement leased idempotent system-day compaction with recoverable state transitions and verified cleanup only after segment and rollup durability.
- [ ] 7.4 Implement merged hot/segment/overlay raw reads with deterministic ordering, deduplication, quality metadata, and old-version fixtures.
- [ ] 7.5 Implement overlay folding and atomic segment replacement with generation checks, crash-point tests, and immediate query consistency.
- [ ] 7.6 Implement 15-minute, hourly, daily, monthly, and yearly rollup builders with sums/min/max/count/first/last/coverage and timezone/DST tests.
- [ ] 7.7 Implement daily/lifetime summaries, dependency invalidation, idempotent rebuilds, and reconciliation of late/corrected data.
- [ ] 7.8 Implement integrity verification and repair planning for hot rows, segments, hashes, overlays, rollups, summaries, and orphaned jobs without silent data mutation.
- [ ] 7.9 Implement management dispatch plus account-local database-backed job leasing, heartbeats, bounded retries, jitter, idempotent handlers, dead-letter inspection, and worker restart tests.

## 8. Query, Statistics, and Chart API

- [ ] 8.1 Implement a query planner that selects hot rows, archived segments, or the coarsest valid rollup from time range, requested resolution, fields, timezone, and maximum points.
- [ ] 8.2 Implement multi-series raw and aggregate queries with explicit units, resolution, coverage, gaps, provenance, and bounded result validation.
- [ ] 8.3 Implement daily/monthly/yearly/lifetime statistics for generation, consumption, grid, efficiency, peaks, environment, battery, finance, and coverage.
- [ ] 8.4 Implement missing/suspect interval, source conflict, counter reset, rejected-ingestion, and aggregate-lag detection without fabricated raw points.
- [ ] 8.5 Implement system/team comparison and ladder services using effective capacity, eligibility, privacy, coverage, normalization, and tie rules.
- [ ] 8.6 Implement modern time-series, statistics, missing-data, comparison, ladder, and synchronous/asynchronous CSV/JSON export endpoints.
- [ ] 8.7 Complete OpenAPI query models and executable examples, including point budgets, timezone/DST, errors, jobs, CSV headers, and conditional caching.
- [ ] 8.8 Add query-plan regression tests and performance harness assertions for 30-day and 25-year chart service objectives.

## 9. PVOutput Compatibility Contract and Core Services

- [ ] 9.1 Capture a dated, machine-readable inventory of every official r2 route, method, parameter, condition, response field, error, restriction, and donation feature and generate the human compatibility matrix.
- [ ] 9.2 Build reusable compatibility parsing/formatting for authentication, form/query inputs, legacy dates/times/booleans, CSV escaping/empty fields, success text, and legacy errors, with all golden inputs/outputs under `tests/compatibility/` and `tests/fixtures/`.
- [ ] 9.3 Implement and fixture-test `addoutput.jsp`, including single/CSV/batched daily outputs, all documented fields, calculations, restrictions, and errors.
- [ ] 9.4 Implement and fixture-test `addstatus.jsp`, including cumulative energy, power calculation, net data, battery state, extended values, restrictions, and errors.
- [ ] 9.5 Implement and fixture-test `addbatchstatus.jsp`, including documented batch formats, net fields, item status codes, limits, and retry behavior.
- [ ] 9.6 Implement and fixture-test `getstatus.jsp` and `getstatistic.jsp`, including history, day statistics, system selection, date/range filters, and errors.
- [ ] 9.7 Implement and fixture-test `getoutput.jsp`, `getextended.jsp`, `getmissing.jsp`, and `deletestatus.jsp`, including aggregate/team/insolation flags and legacy field ordering.
- [ ] 9.8 Add end-to-end uploader compatibility tests that prove legacy requests and the modern API converge on identical canonical data and statistics.

## 10. Remaining PVOutput, Community, and Provider Services

- [ ] 10.1 Implement privacy-safe account projection events and management-catalog system search, visibility, country/location filters, favourites, and modern API resources with freshness and privacy integration tests.
- [ ] 10.2 Implement team lifecycle, membership transfer/join/leave, eligibility, projected aggregates, ladders, ranking coverage, and cross-account projection lag behavior with modern API tests.
- [ ] 10.3 Implement and fixture-test `getsystem.jsp`, `postsystem.jsp`, `search.jsp`, `getfavourite.jsp`, and `getladder.jsp` against the compatibility inventory.
- [ ] 10.4 Implement and fixture-test `getteam.jsp`, `jointeam.jsp`, and `leaveteam.jsp`, including ownership, membership limits, eligibility, and documented errors.
- [ ] 10.5 Define insolation and regional supply adapter contracts, persistence/cache/provenance models, circuit breakers, and administrator configuration without bundling unapproved data.
- [ ] 10.6 Implement configured insolation and regional supply adapters with freshness/licensing metadata, degraded behavior, and deterministic fake-provider tests.
- [ ] 10.7 Implement and fixture-test `getinsolation.jsp` and `getsupply.jsp`, including region keys, timezone/date/history behavior, unavailable providers, and field order.
- [ ] 10.8 Run the full 21-route compatibility conformance suite and require every matrix entry to link to a passing test or documented intentional difference.

## 11. Alerts, Webhooks, and Notifications

- [ ] 11.1 Implement timezone-aware alert rule CRUD and evaluation for idle, generation, consumption, net power, standby cost, performance, battery, and extended-channel conditions.
- [ ] 11.2 Implement debounce, cooldown, recovery, deduplication, transactionally queued alert events, and evaluator lag metrics with clock-controlled tests.
- [ ] 11.3 Implement webhook subscription verification/lifecycle, event schemas, stable event IDs, keyed signatures, timestamp/replay guidance, and secret rotation.
- [ ] 11.4 Implement SSRF-safe delivery with HTTPS defaults, DNS re-resolution, address blocking, redirect/body/time limits, explicit local-network policy, and security tests.
- [ ] 11.5 Implement leased delivery attempts, exponential backoff/jitter, outcome history, dead-letter state, administrative replay, and observability.
- [ ] 11.6 Implement modern alert, event, webhook, attempt, and replay endpoints plus OpenAPI webhook/callback definitions and verified consumer examples.
- [ ] 11.7 Implement and fixture-test `registernotification.jsp` and `deregisternotification.jsp`, all documented alert types, registration limits, and legacy callback payloads.

## 12. Web Application Product Workflows

- [ ] 12.1 Implement local login/recovery/activation, external connector selection/callback states, the responsive application shell, account/system navigation, session bootstrap, route authorization, skip link, error boundaries, loading states, and English/German strings.
- [ ] 12.2 Implement guided instance/first-system onboarding, equipment/capacity/timezone setup, credential creation, test ingestion, and verification workflows.
- [ ] 12.3 Implement the operational dashboard with freshness-safe live status, KPIs, data coverage, recent alerts, ingestion health, and responsive light/dark layouts.
- [ ] 12.4 Implement accessible historical chart controls and rendering for generation, consumption, grid, battery, environment, finance, and extended channels with bounded point requests.
- [ ] 12.5 Add keyboard/screen-reader chart summaries and tables, localized time/unit formatting, non-color cues, reduced motion, zoom/comparison, and matching CSV/JSON export.
- [ ] 12.6 Implement data-quality inspection, rejected-ingestion details, missing/suspect intervals, optimistic correction, deletion, and reconciliation progress.
- [ ] 12.7 Implement local user/invitation, hierarchical role/permission, external identity, generic OIDC/OAuth2 connector and Google/GitHub/Facebook/X preset administration alongside system, equipment, tariff, channel, member, credential, privacy, lifecycle, session, and audit views.
- [ ] 12.8 Implement search, favourites, teams, ladders, system comparisons, regional supply, and provider freshness/provenance views.
- [ ] 12.9 Implement alert rules, webhook subscriptions/delivery history, import/export jobs, worker/dead-letter, storage integrity, backup, and instance administration views.
- [ ] 12.10 Add frontend unit/component tests under `tests/ui/`, Playwright critical flows under `tests/e2e/`, API schema failure tests, axe checks, keyboard tests, bundle budgets, and 80/75/80/80 coverage gates without co-located test files under `src/ui/`.

## 13. Documentation and OpenAPI Completion

- [ ] 13.1 Complete `openapi/pvlog-v1.yaml` for every modern operation, schema, security scope, problem, pagination/idempotency/ETag behavior, job, webhook, example, tag, and deprecation.
- [ ] 13.2 Enforce bidirectional Axum route/OpenAPI coverage and normalized generated-versus-committed spec diff checks in CI.
- [ ] 13.3 Build the locally packaged searchable documentation site with version selector, raw OpenAPI download, keyboard accessibility, responsive themes, and no CDN dependency.
- [ ] 13.4 Write and test quickstarts for local authentication/RBAC, generic multi-provider OIDC/OAuth2, Google/GitHub/Facebook/X setup and callback registration, safe identity linking/recovery, system creation, ingestion, corrections, queries/charts, pagination/errors/rate limits, and generated client use.
- [ ] 13.5 Write and test webhook, import/export, SQLite management/account topology, PostgreSQL, account-scoped and full-instance backup/restore, upgrade/rollback, maintenance, observability, and troubleshooting guides.
- [ ] 13.6 Publish the dated per-parameter PVOutput compatibility matrix and uploader migration guide with base URL, credentials, security caveats, examples, and deliberate differences.
- [ ] 13.7 Add API glossary, unit/timezone/quality semantics, permission tables, architecture/segment format docs, changelog/deprecation policy, and generated-client examples.
- [ ] 13.8 Add CI checks for OpenAPI validity, examples/snippets, links/anchors, terminology/spelling, accessibility, screenshots where stable, and documentation-version alignment.

## 14. Packaging, Operations, and Recovery

- [ ] 14.1 Implement unprivileged multi-stage images and `server`, `worker`, `migrate`, `doctor`, `export`, `import`, and `verify` commands with build/version metadata.
- [ ] 14.2 Add SQLite and PostgreSQL Compose profiles, persistent data roots for `management.sqlite3` and opaque account files, health checks, `.env.example`, secret generation guidance, OIDC-neutral variables, and upgrade-safe image tags.
- [ ] 14.3 Implement distinct startup, liveness, readiness, dependency, job-lag, build/version, and storage-integrity endpoints with failure-mode tests.
- [ ] 14.4 Implement structured secret-redacted logs, correlated request/job IDs, OpenTelemetry traces/metrics, browser trace ingestion guidance, and operational dashboards/alerts.
- [ ] 14.5 Implement account-scoped versioned checksummed export bundles with segment/correction data, manifest compatibility, resumable import, dry run, account transfer, and SQLite-to-PostgreSQL verification.
- [ ] 14.6 Implement SQLite online backups for management and account databases with coordinated backup-set manifests/checkpoints, plus PostgreSQL backup integration, isolated full/account restore verification, encryption/retention hooks, and automated backup drills.
- [ ] 14.7 Implement operator maintenance for partitions, indexes, WAL, integrity, compaction, reconciliation, dead letters, credential rotation, capacity, and provider cache.
- [ ] 14.8 Document and test upgrade compatibility, migration locks/failures, required space/duration, backup prerequisites, post-upgrade verification, rollback windows, and deferred destructive cleanup.

## 15. Scale, Resilience, and Release Certification

- [ ] 15.1 Build deterministic fleet/history generators under `tests/performance/` and reusable fixtures under `tests/fixtures/` for sparse/dense extended channels, irregular intervals, DST, counter resets, gaps, corrections, and 25-year segmented datasets.
- [ ] 15.2 Build reproducible burst-ingestion and concurrent chart/statistics workloads that record hardware, PostgreSQL settings, bytes per system-day, compression, queue lag, and p50/p95/p99 latency.
- [ ] 15.3 Tune and document the PostgreSQL 5,000-system profile for 13.14 billion modeled five-minute observations, at least 250 observations/second bursts, and the specified chart p95 objectives.
- [ ] 15.4 Benchmark and document the SQLite management/per-account profile across account count, concurrent account writers, per-account size, pool/file-descriptor limits, checkpoint and projection lag, maintenance, and the threshold for migrating to PostgreSQL without implying scale-profile parity.
- [ ] 15.5 Add fault tests for server/worker termination at transaction and compaction transitions, database interruption, disk exhaustion, corrupt segments, queue backlog, and provider/webhook failures.
- [ ] 15.6 Execute management-plus-account and single-account backup/restore, orphan/missing-file reconciliation, cross-database import, projection/integrity reconciliation, migration rollback-boundary, and old-segment-version disaster-recovery exercises.
- [ ] 15.7 Run security review and automated checks for local password/recovery, OIDC/OAuth2 state/nonce/PKCE/token validation, identity linking/takeover, RBAC escalation, sessions/CSRF, scopes, connector secret leakage, SSRF, dependency/container vulnerabilities, CORS/CSP, unsafe defaults, and privacy enumeration.
- [ ] 15.8 Run full Rust/backend/frontend/OpenAPI/docs/accessibility/PVOutput conformance suites with zero Rust warnings and resolve every failure or undocumented compatibility gap.
- [ ] 15.9 Publish the certified capacity report, compatibility snapshot, known limitations, support/deprecation policy, checksums/SBOM, operator runbook, and first stable release notes.
