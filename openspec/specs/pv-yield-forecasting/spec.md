# pv-yield-forecasting Specification

## Purpose
TBD - created by archiving change add-pv-yield-forecasting. Update Purpose after archive.
## Requirements
### Requirement: Reproducible PV yield calculation

The system SHALL calculate versioned interval PV power and energy estimates from the effective system location, string orientation and tilt, module composition and confirmed technical values, inverter characteristics, explicit loss/calibration settings, and normalized weather or insolation inputs. Each result SHALL identify its model version, configuration snapshot or digest, input run, assumptions, units, interval, and rounding semantics, and SHALL be unavailable rather than fabricated when required inputs are missing.

#### Scenario: Complete string inputs are modeled

- **WHEN** a string has an effective module configuration and geometry and the selected weather run provides the required interval inputs
- **THEN** the system returns deterministic power and energy estimates tied to the exact configuration, input run, and model version

#### Scenario: Required geometry is missing

- **WHEN** a calculation requires plane-of-array conversion but the effective string orientation or tilt is unavailable
- **THEN** the system marks that string estimate unavailable with a field-specific completeness reason and does not substitute zero generation

#### Scenario: Configuration changes inside a query range

- **WHEN** a string configuration changes during a requested forecast or expected-generation range
- **THEN** each interval uses the configuration effective at that time and the response exposes the configuration boundary

### Requirement: Distinct forecast and expected-generation baselines

The system SHALL calculate forward forecast generation only from weather data identified as a forecast and SHALL calculate historical expected generation from observed or reanalysis weather inputs. It MUST NOT silently reinterpret an expired forecast as observed weather or label actual generation divided by forecast generation as plant or inverter efficiency.

#### Scenario: Future weather forecast is available

- **WHEN** a valid weather forecast run covers a future interval
- **THEN** the system produces forecast power and energy carrying the forecast issue time, horizon, freshness, provider provenance, and uncertainty

#### Scenario: Historical performance is requested

- **WHEN** actual generation and sufficiently covered observed or reanalysis weather inputs exist for a historical interval
- **THEN** the system calculates expected generation from those historical inputs before calculating generation performance

#### Scenario: Only a prior forecast covers the historical interval

- **WHEN** actual generation exists but no eligible observed or reanalysis input covers the interval
- **THEN** the system may report actual-versus-forecast realization but marks generation performance unavailable

### Requirement: Hierarchical forecast aggregation

The system SHALL calculate string estimates independently and aggregate compatible interval results to their containing inverter and PV system. Aggregates SHALL report included and excluded strings, included and total effective DC capacity, completeness, and uncertainty, and SHALL NOT combine results with incompatible intervals, forecast runs, model versions, or effective configurations without exposing the boundary.

#### Scenario: All strings have complete inputs

- **WHEN** every effective string beneath an inverter has a complete estimate for the same interval and calculation run
- **THEN** the inverter estimate equals the sum of its string estimates and the system estimate equals the sum of its inverter estimates subject only to documented inverter conversion and clipping behavior

#### Scenario: One string is incomplete

- **WHEN** one effective string lacks required forecast inputs
- **THEN** complete-mode aggregation is unavailable and explicitly requested partial aggregation identifies the missing string and excluded capacity

### Requirement: Actual-versus-expected generation performance

The system SHALL calculate generation performance as actual energy divided by positive expected energy only for aligned intervals that meet configured actual-telemetry and weather-input coverage and quality thresholds. It SHALL expose the ratio, actual and expected energy, interval, scope, coverage, uncertainty, and unavailable reason, and SHALL NOT allocate aggregate actual telemetry to unmeasured child scopes.

#### Scenario: System performance has sufficient coverage

- **WHEN** system generation telemetry and historical expected generation cover the same interval above configured quality thresholds
- **THEN** the system returns a system generation performance ratio with both energy values and their coverage metadata

#### Scenario: Inverter actual telemetry is absent

- **WHEN** only system-level actual generation is measured
- **THEN** inverter and string performance ratios remain unavailable even if modeled inverter and string expected generation exists

#### Scenario: Expected energy is zero or unavailable

- **WHEN** an interval has no positive expected energy or fails required input coverage
- **THEN** the system returns no performance ratio and provides a stable unavailable reason instead of dividing by zero or returning zero percent

### Requirement: Forecast versioning and reconciliation

The system SHALL persist immutable weather/input runs and versioned calculation runs, SHALL retain the run selected for a published forecast or performance result, and SHALL idempotently invalidate and rebuild only affected intervals after effective equipment changes, provider revisions, model-setting changes, late telemetry, or corrections. Previously issued forecasts SHALL remain queryable according to retention policy.

#### Scenario: Provider publishes a revised forecast

- **WHEN** a provider publishes a newer run for an overlapping horizon
- **THEN** the system stores a distinct run, calculates a distinct forecast version, and preserves the older issued forecast without overwriting it

#### Scenario: Late telemetry changes actual energy

- **WHEN** accepted late data or a correction changes actual generation in a summarized interval
- **THEN** the system invalidates and idempotently rebuilds the dependent performance result while leaving the immutable expected-generation input run unchanged

### Requirement: Forecast and performance API resources

The system SHALL provide authorized bounded `/api/v1` resources for forecast runs, forecast and expected-generation series, aggregated summaries, input completeness, and actual-versus-expected performance. Responses SHALL use explicit units and include scope, resolution, issue time where applicable, model/configuration versions, provenance, freshness, uncertainty, coverage, and stable unavailable reasons.

#### Scenario: Client queries a system forecast

- **WHEN** an authorized client requests a bounded system forecast range and resolution
- **THEN** the API returns compatible forecast points within documented limits plus run, model, provenance, freshness, uncertainty, and completeness metadata

#### Scenario: Provider is unavailable

- **WHEN** no permitted fresh or stale forecast run can satisfy a request
- **THEN** the API returns an explicit temporarily unavailable problem without affecting telemetry ingestion or ordinary stored-data queries

#### Scenario: Forecast data is exported

- **WHEN** an authorized user exports a forecast or performance view
- **THEN** the export preserves the displayed values, units, interval semantics, versions, coverage, uncertainty, and provider attribution

