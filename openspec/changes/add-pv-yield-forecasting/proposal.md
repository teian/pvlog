## Why

PVLog already records string composition, installed peak power, orientation, tilt, system location, and optional insolation data, but it does not turn those inputs into an expected generation baseline or forward-looking power forecast. Operators therefore cannot distinguish low production caused by weather from avoidable underperformance, compare actual generation with weather-adjusted potential, or plan around predicted output.

## What Changes

- Treat the effective-dated module composition of each inverter string as the authoritative installed DC-capacity input and aggregate it deterministically to inverter and system totals.
- Add versioned forecast-model settings for configurable losses and calibration without overwriting confirmed equipment data or historical model inputs.
- Extend provider-neutral external data support from historical insolation samples to time-bounded weather forecasts containing irradiance and the environmental inputs required by the selected yield model.
- Calculate interval power and energy forecasts per string, aggregate them to inverter and system forecasts, and retain model version, equipment/configuration snapshot, provider provenance, issue time, horizon, freshness, coverage, and uncertainty.
- Compare actual generation with weather-adjusted expected generation only where compatible telemetry and forecast coverage exist, exposing the result as a generation performance ratio rather than conflating it with inverter conversion efficiency.
- Add modern API and accessible English/German web views for forecast curves, expected-versus-actual energy, performance ratios, input completeness, stale/unavailable states, and model provenance.
- Recompute affected forecasts and performance summaries idempotently when equipment periods, forecast inputs, late telemetry, or corrections change.
- Keep ingestion, historical queries, and previously calculated actual generation available when the weather provider or forecast worker is unavailable; never fabricate missing forecasts or actual measurements.

## Capabilities

### New Capabilities

- `pv-yield-forecasting`: Weather-driven string, inverter, and system power/energy forecasts with provenance, uncertainty, coverage, aggregation, recalculation, and actual-versus-expected performance comparison.

### Modified Capabilities

- `pv-system-management`: Make forecast-relevant string configuration and effective-dated DC-capacity aggregation explicit, including configurable loss/calibration inputs and completeness validation.
- `notifications-and-integrations`: Extend provider-neutral insolation integrations with forecast weather inputs, issue/horizon semantics, provenance, freshness, caching, and degraded behavior.
- `analytics-and-charting`: Add expected and forecast energy series, actual-versus-expected performance ratios, coverage rules, rollups, and recalculation behavior.
- `web-application`: Add accessible localized forecast and generation-performance views with transparent completeness, freshness, uncertainty, and provenance states.

## Impact

- Domain and application models for PV strings, effective configurations, weather forecasts, yield-model inputs/results, aggregation, and performance statistics.
- SQLite and PostgreSQL account schemas/repositories for model settings, provider forecast snapshots, calculated forecast series, provenance, uncertainty, and invalidation state.
- Provider adapters, worker jobs, cache/circuit-breaker behavior, rollup reconciliation, telemetry correction handling, and observability.
- `/api/v1` forecast, performance, and configuration contracts plus the committed OpenAPI document, route/schema coverage, examples, and exports.
- System/equipment administration, dashboards, charts, localized copy, data tables, and tests across domain, storage, API, workers, UI, contracts, end-to-end behavior, and deterministic model fixtures.
