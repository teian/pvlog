## Context

PVLog already models effective-dated inverters and PV strings, including module count, confirmed module manufacturer/model, peak power per module, derived string peak power, orientation, tilt, and optional module/inverter technical snapshots. Systems carry timezone and geographic location. Provider-neutral insolation adapters, caches, circuit breakers, jobs, telemetry rollups, and data-quality coverage also exist, but the provider model represents only generic insolation samples and PVLog does not calculate expected or forecast generation.

Two related but different questions must be answered without conflating them. A forward power forecast estimates future generation from predicted weather and is useful for planning. A historical performance baseline estimates what the configured plant could reasonably have generated under weather that actually occurred; only that baseline supports diagnosing plant underperformance. Comparing actual output with a prior weather forecast measures both forecast error and plant behavior and must not be labeled inverter or system efficiency.

The solution must remain provider-neutral, work for both routed SQLite account databases and PostgreSQL, preserve effective-dated configuration and forecast provenance, degrade safely when external providers fail, and avoid fabricated precision when configuration, weather, or telemetry coverage is incomplete.

## Goals / Non-Goals

**Goals:**

- Produce versioned, reproducible interval power and energy estimates from the installed string configuration and normalized weather/insolation inputs.
- Produce future forecasts from predicted weather and historical expected-generation baselines from observed or reanalysis weather.
- Aggregate string estimates exactly to inverter and system totals while retaining completeness, uncertainty, provenance, and configuration/model versions.
- Compare actual generation with historical expected generation only at supported telemetry granularity and expose a clearly named generation performance ratio.
- Reconcile results after equipment/configuration changes, provider revisions, late telemetry, or corrections.
- Expose bounded API queries and accessible localized UI views without making ingestion or ordinary historical queries depend on provider availability.

**Non-Goals:**

- Safety-critical inverter control, battery dispatch, automated trading, or export-limit control.
- Guaranteeing a specific forecast accuracy or treating forecasts as contractual production commitments.
- Inferring module manufacturer/model or string topology from telemetry.
- Allocating system-level actual generation down to inverters or strings when no corresponding measurement exists.
- Calling actual-versus-forecast or actual-versus-expected ratios inverter conversion efficiency.
- Bundling a weather provider whose license does not permit the intended self-hosted use.

## Decisions

### 1. Keep nameplate capacity, expected generation, forecast generation, and actual generation distinct

The existing server-derived `module_count × module_peak_power_watts` value remains string DC nameplate capacity and aggregates by effective period to inverter and system installed capacity. It is not itself potential energy. The model produces separate interval series:

- `forecast_power` / `forecast_energy`: future output calculated from a weather forecast run;
- `expected_power` / `expected_energy`: historical weather-adjusted output calculated from observed or reanalysis inputs;
- `actual_power` / `actual_energy`: accepted PVLog telemetry;
- `generation_performance_ratio`: actual energy divided by expected energy for intervals with compatible, sufficiently covered inputs.

Actual divided by a previously issued forecast may be exposed as forecast realization, but never as plant efficiency. Inverter conversion efficiency remains a separate electrical measurement or datasheet characteristic.

Alternative considered: use one `potential_generation` series for both future and historical comparisons. Rejected because forecast error would be indistinguishable from equipment underperformance and later provider revisions could silently change the meaning of historical ratios.

### 2. Snapshot every model run against effective configuration

Each calculation uses the string, inverter, system location/timezone, and forecast-model settings effective for the target interval. A canonical configuration digest references or embeds the confirmed module/inverter specification snapshot, module count, per-module and aggregate power, orientation, tilt, loss assumptions, calibration factor, and effective period. Forecast results retain that digest, model identifier/version, and input run identifier so they remain reproducible after configuration changes.

Forecast settings use bounded fixed-point values for soiling, shading, mismatch, wiring, availability, and user calibration. Defaults are explicit and versioned. Missing required geometry, location, module capacity, or weather fields makes the affected scope incomplete rather than substituting zero or an invented default. Catalog templates may prefill values, but saved installation snapshots remain authoritative.

Alternative considered: read the latest mutable equipment rows whenever a forecast is queried. Rejected because historical forecasts and performance results would change after edits and could not be audited.

### 3. Normalize immutable weather forecast and observation runs behind provider-neutral ports

The provider boundary gains a weather-forecast capability and normalized immutable runs. A run records provider/configuration ID, issue time, fetch time, valid interval/horizon, spatial reference, resolution, license/provenance, and ordered points. Points carry interval bounds and the irradiance/weather inputs supported by the adapter, including plane-of-array irradiance directly or sufficient global/direct/diffuse irradiance data for transposition, plus ambient temperature when required by the selected model; optional cloud cover, wind, and provider uncertainty are retained with explicit units.

Adapters validate ordering, bounds, horizon, location coverage, units, and required fields before persistence. A newer run does not overwrite an older run. Query APIs select an explicit run or apply a documented issue-time policy. Cache/circuit-breaker behavior may return a clearly stale run within policy, but must never relabel it as fresh.

Observed/reanalysis insolation remains a distinct input kind for historical expected-generation calculations. Forecast inputs are not silently promoted to observed weather after their validity interval.

Alternative considered: store provider JSON and calculate directly from provider-specific fields. Rejected because it couples the domain to vendors, weakens validation, and prevents reproducible cross-provider tests.

### 4. Use a deterministic, versioned yield pipeline with explicit uncertainty

The initial model is an internal, deterministic pipeline using fixed-point/integer boundary values and documented rounding:

1. resolve solar position and plane-of-array irradiance from the system location, time, string orientation/tilt, and normalized weather input when plane-of-array irradiance is not supplied;
2. estimate module temperature from ambient/weather inputs using the selected versioned model;
3. calculate string DC output from irradiance, effective module nameplate power, temperature coefficient when available, and bounded loss/calibration factors;
4. aggregate strings by inverter, apply inverter conversion characteristics and clipping when configured, then aggregate inverters to the system;
5. integrate interval power to energy and attach coverage and uncertainty.

The pipeline caps physical outputs at configured bounds, records assumptions/defaults, and returns incomplete when required inputs are absent. Provider uncertainty and model uncertainty are combined conservatively into lower/central/upper estimates; absence of uncertainty data yields an explicit unknown range rather than zero uncertainty.

Alternative considered: introduce a large external forecasting service or Python runtime. Rejected for the initial implementation because it complicates offline self-hosting and reproducibility. The versioned port/model boundary permits a more advanced model later without rewriting stored runs.

### 5. Aggregate estimates upward but compare actuals only where measured

String estimates sum to inverter estimates, and inverter estimates sum to system estimates for the same interval, forecast run, model version, and effective configuration. Coverage reports included/missing capacity and reasons. Partial results may be returned only when explicitly requested and must expose excluded capacity; complete-mode queries fail closed for incomplete topology or inputs.

Performance comparison occurs at system scope when only system generation telemetry exists, at inverter scope when inverter generation channels exist, and at string scope only when string-level generation is actually measured. PVLog never distributes an aggregate actual value proportionally across children. Ratios require a positive expected-energy denominator, compatible intervals, minimum actual/weather coverage, and non-suspect data. The API returns unavailable with reason codes otherwise.

Alternative considered: estimate string actuals by installed-capacity share. Rejected because shading, clipping, orientation, outages, and measurement placement make the derived values look more precise than they are.

### 6. Persist immutable runs and reconcile derived projections asynchronously

Account databases store normalized weather runs and points, calculation runs, interval results, model/configuration digests, provenance, uncertainty, and invalidation state. Forecast issuance and provider fetches enqueue idempotent calculation jobs. Effective configuration changes invalidate only intersecting intervals; observed-weather revisions, late telemetry, and corrections invalidate dependent expected/performance buckets. Worker leases, bounded retries, dead-letter state, and per-account concurrency reuse the existing job infrastructure.

Forward forecasts retain enough run history to reproduce what was known at an issue time, subject to a documented retention policy. Historical expected series and daily/monthly/yearly summaries use the existing rollup invalidation/rebuild pattern. Reads may serve the last successful result with stale metadata while recalculation is pending, but never mix intervals from incompatible model/configuration versions without reporting the boundary.

Alternative considered: calculate every forecast synchronously on read. Rejected because provider latency, multi-string modeling, long horizons, and repeated chart queries would make latency and failure behavior unpredictable.

### 7. Add bounded forecast/performance resources and transparent UI states

Modern API resources expose forecast runs, bounded forecast/expected series, aggregated summaries, input completeness, and performance comparisons. Responses include scope, interval, issue time, model/configuration version, units, lower/central/upper estimates, coverage, freshness, and provenance. ETags protect mutable forecast settings; external secrets remain secret references. Exports preserve the same metadata.

The web application adds responsive forecast and actual-versus-expected charts, summary cards, an accessible data-table alternative, model/input explanations, and administration for loss/calibration settings. English and German text distinguishes forecast, expected generation, actual generation, forecast realization, generation performance, and inverter efficiency. Missing, stale, partial, and provider-unavailable states remain visible rather than collapsing to zero.

Alternative considered: add forecast values to existing generation fields. Rejected because clients could mistake modeled values for measurements and existing API semantics would become ambiguous.

## Risks / Trade-offs

- **[Misleading accuracy]** A simple physical model can look more authoritative than its inputs justify. → Return uncertainty, coverage, assumptions, model version, and reason codes; avoid precision beyond normalized inputs.
- **[Forecast versus performance confusion]** Poor weather forecasts can mimic plant faults. → Use observed/reanalysis weather for generation performance and label actual-versus-forecast separately as forecast realization.
- **[Configuration gaps]** Location, orientation, tilt, coefficients, or loss settings may be absent. → Provide completeness diagnostics and explicit versioned defaults only where scientifically defensible; otherwise mark results unavailable.
- **[Provider drift and licensing]** Fields, horizons, availability, or redistribution rights may change. → Keep adapters provider-neutral, validate normalized contracts, retain provenance/license metadata, and do not bundle restricted sources.
- **[Recalculation load]** Provider revisions and late telemetry can fan out into many jobs. → Coalesce invalidations by account/system/range, use idempotency keys, bound concurrency, and rebuild rollups incrementally.
- **[Dual-database divergence]** Time-series upserts and retention may behave differently in SQLite and PostgreSQL. → Keep shared repository contracts and cross-engine fixtures for ordering, idempotency, invalidation, and aggregation.
- **[Historical forecast volume]** Retaining every provider run can grow quickly. → Define run/result retention tiers while preserving referenced evaluation runs and aggregated verification metadata.

## Migration Plan

1. Add backward-compatible nullable forecast settings and new weather/calculation tables to SQLite and PostgreSQL; existing systems remain valid but forecast-incomplete until required inputs are present.
2. Extend provider configuration/capabilities and normalized ports without changing current insolation behavior, then ship deterministic fixtures and one administrator-configured adapter path.
3. Add calculation/reconciliation workers behind a disabled-by-default feature/configuration gate and backfill effective configuration digests without generating forecasts.
4. Enable forecast APIs and settings, then UI views, after cross-engine, model, provider-failure, OpenAPI, accessibility, and production embedded-UI tests pass.
5. Optionally enqueue bounded forward forecasts and historical expected-generation backfills per account; expose progress and allow cancellation/retry.

Rollback disables provider polling and forecast jobs, leaving telemetry ingestion and existing queries unaffected. New tables and nullable columns remain in place until a later cleanup release; older application versions ignore them. Calculated forecasts can be discarded and reproduced from retained normalized inputs and configuration/model versions.

## Open Questions

- Which forecast and observed/reanalysis providers have licensing and coverage suitable for recommended self-hosted presets?
- What default forward horizon, interval resolution, issue-time cutoff, and run-retention tiers should ship for SQLite and PostgreSQL profiles?
- Which documented module-temperature and irradiance-transposition model should be version 1 after validation against public reference fixtures?
- Should user calibration initially be a single bounded system/string factor, or require enough measured history for an automated calibration workflow in a later change?
