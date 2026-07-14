## 1. Forecast Domain and Effective Configuration

- [x] 1.1 Add forecast/input/calculation identifiers, normalized weather-run types, forecast-versus-observed classifications, explicit units, uncertainty ranges, completeness reasons, and versioned calculation result models.
- [x] 1.2 Extend effective PV string configuration with bounded fixed-point loss/calibration settings and a canonical forecast-input snapshot/digest without changing confirmed equipment identity or actual telemetry.
- [x] 1.3 Implement effective-dated string-to-inverter-to-system DC nameplate aggregation with configuration-boundary and incomplete-input reporting.
- [x] 1.4 Add domain tests for forecast setting bounds, stable digests, effective-period selection, catalog-customized snapshots, capacity aggregation, missing inputs, overflow, and deterministic serialization.

## 2. Cross-Engine Persistence

- [x] 2.1 Add backward-compatible SQLite and PostgreSQL migrations for forecast settings, immutable weather runs/points, calculation runs/results, provenance, uncertainty, invalidations, and retention metadata.
- [x] 2.2 Implement cross-engine repositories for effective forecast configuration and immutable normalized weather runs with ordering, overlap, revision, classification, and idempotency constraints.
- [x] 2.3 Implement cross-engine repositories for versioned string/inverter/system calculation results, active projections, completeness, run history, and bounded range queries.
- [ ] 2.4 Implement targeted invalidation and retention operations that preserve referenced issued forecasts and historical performance inputs.
- [ ] 2.5 Add SQLite/PostgreSQL migration and repository tests for legacy systems, round trips, immutable revisions, stale selection, effective boundaries, idempotent writes, partial capacity, invalidation, and retention.

## 3. Weather Provider Normalization

- [ ] 3.1 Extend provider-neutral capabilities and application ports with forecast, observed, and reanalysis weather runs while preserving existing insolation and regional-supply behavior.
- [ ] 3.2 Implement normalized run validation for issue/fetch time, horizon, location coverage, interval ordering, irradiance/environmental units, required fields, uncertainty, provenance, and licensing.
- [ ] 3.3 Extend provider cache and circuit-breaker services to select fresh or policy-permitted stale immutable runs without relabeling forecast data as observations.
- [ ] 3.4 Implement one administrator-configured weather adapter path plus deterministic provider fixtures without bundling restricted credentials or licensed data.
- [ ] 3.5 Add provider contract tests for valid normalization, malformed/overlapping intervals, unsupported units, missing irradiance, revisions, stale fallback, open circuits, location mismatch, and provider-independent domain output.

## 4. Deterministic Yield Model

- [ ] 4.1 Implement and document the version-1 solar-position and plane-of-array irradiance calculation with fixed rounding and public reference fixtures.
- [ ] 4.2 Implement the version-1 module-temperature, string DC output, temperature-coefficient, bounded loss/calibration, and physical-cap calculations.
- [ ] 4.3 Implement inverter conversion/clipping and compatible string-to-inverter-to-system aggregation with included/excluded capacity and uncertainty propagation.
- [ ] 4.4 Implement interval power-to-energy integration and distinct forecast-generation versus observed/reanalysis expected-generation calculation paths.
- [ ] 4.5 Implement actual-versus-expected generation performance and actual-versus-forecast realization with coverage/quality thresholds, positive-denominator checks, and no downward allocation of aggregate actuals.
- [ ] 4.6 Add deterministic model tests for reference irradiance, temperature effects, losses, clipping, uncertainty, nighttime/zero expectation, configuration changes, partial topology, aggregation conservation, and unavailable reasons.

## 5. Calculation Jobs and Reconciliation

- [ ] 5.1 Add idempotent provider-poll and forecast-calculation jobs with safe payload references, account routing, leases, bounded retries, concurrency limits, and dead-letter behavior.
- [ ] 5.2 Add coalesced interval invalidation and rebuild jobs for equipment/settings changes, provider revisions, late telemetry, corrections, and model-version changes.
- [ ] 5.3 Extend daily/monthly/yearly/lifetime summary rebuilds with expected generation, forecast realization, generation performance, uncertainty, and independent coverage metadata.
- [ ] 5.4 Add operational metrics and safe diagnostics for provider freshness/failures, forecast age, calculation lag/outcomes, invalidation backlog, completeness, model version, and dead letters.
- [ ] 5.5 Add worker integration tests for duplicate jobs, retries, provider outage, stale results, intersecting invalidations, correction-driven rebuilds, configuration boundaries, and telemetry independence.

## 6. Modern API and Contract

- [ ] 6.1 Add authorized ETag-protected forecast-settings and input-completeness resources for account/system/inverter/string scopes with field-specific validation problems.
- [ ] 6.2 Add bounded forecast-run and forecast/expected series resources with explicit scope, resolution, issue time, versions, provenance, freshness, uncertainty, coverage, partial-capacity, and unavailable metadata.
- [ ] 6.3 Add aggregated performance and forecast-realization resources that align actual/model intervals and refuse unsupported child-scope ratios.
- [ ] 6.4 Extend analysis export with forecast, expected, actual, and performance values plus the same units, interval semantics, versions, coverage, uncertainty, and attribution shown by queries.
- [ ] 6.5 Update the committed OpenAPI contract, generated fixture, examples, operation coverage, problem types, security, pagination/bounds, and backward-compatibility checks for all forecast resources.
- [ ] 6.6 Add API/contract tests for authorization, bounds, ETags, effective settings, run selection, stale/unavailable providers, partial aggregation, model boundaries, exports, generic failures, and continued ingestion availability.

## 7. Accessible Web Experience

- [ ] 7.1 Add Zod-validated forecast/settings/performance clients and TanStack Query hooks that preserve modeled-versus-measured types and invalidate affected resources after settings changes.
- [ ] 7.2 Add localized forecast-input completeness and effective-dated loss/calibration administration linked to existing system and string configuration.
- [ ] 7.3 Add responsive forward forecast charts and summaries with issue time, horizon, uncertainty, freshness, provider attribution, configuration/model version, and partial/unavailable states.
- [ ] 7.4 Add expected-versus-actual and forecast-realization views that distinguish generation performance from inverter efficiency and never render missing values as zero.
- [ ] 7.5 Add keyboard-accessible data tables, textual summaries, non-color cues, localized units/dates, loading/empty/error states, and matching forecast/performance exports.
- [ ] 7.6 Add English/German component and Playwright tests for complete, partial, stale, unavailable, configuration-gap, boundary, recalculation, underperformance, and table-alternative workflows.

## 8. Deployment, Documentation, and Release Validation

- [ ] 8.1 Add documented provider-neutral runtime/Compose configuration for adapter endpoints, secret references, polling, horizons, stale policy, model defaults, feature gating, retention, and worker concurrency.
- [ ] 8.2 Document nameplate capacity, forecast generation, historical expected generation, actual generation, forecast realization, generation performance, inverter efficiency, uncertainty, calibration, provider licensing, and outage behavior.
- [ ] 8.3 Add deterministic backfill/recalculation commands with dry-run, bounded account/range selection, progress, cancellation/retry, and rollback guidance.
- [ ] 8.4 Run warning-free Rust checks and focused tests, SQLite/PostgreSQL profiles, frontend lint/typecheck/tests/build, Playwright, OpenAPI lint/compare/coverage, provider/model fixtures, security checks, and production embedded-UI validation.
