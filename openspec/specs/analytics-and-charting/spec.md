# Analytics And Charting Specification

## Purpose
TBD - created by archiving change build-self-hosted-pv-platform. Update Purpose after archive.
## Requirements
### Requirement: Resolution-aware series queries
The system SHALL return time-series data at an explicit or automatically selected resolution that honors the requested time range, timezone, fields, aggregation functions, and maximum point budget. Responses SHALL identify actual resolution, units, coverage, gaps, and whether values are raw or aggregated.

#### Scenario: Twenty-five-year chart is requested
- **WHEN** an authorized client requests 25 years of generation with a maximum of 2,000 points
- **THEN** the system selects an appropriate rollup, returns no more than the documented bound plus boundary points, and reports the chosen resolution

#### Scenario: Exact raw points are requested
- **WHEN** an authorized client requests raw resolution for a bounded interval within raw-query limits
- **THEN** the system returns corrected canonical observations ordered by timestamp without substituting rollup values

### Requirement: Daily and lifetime statistics

The system SHALL calculate daily, monthly, yearly, and lifetime generation, consumption, import/export, electrical efficiency, expected generation, forecast realization, generation performance, peak, temperature, battery, financial, and data-coverage statistics when the underlying measured and modeled fields are available. Expected/forecast values and ratios SHALL retain their input, model, uncertainty, freshness, and coverage metadata and SHALL remain unavailable rather than defaulting to zero when prerequisites are missing.

#### Scenario: Late reading changes a daily total

- **WHEN** accepted late data affects a previously summarized local day
- **THEN** the system marks dependent actual-generation and performance summaries stale, rebuilds them, and serves the corrected totals and ratios with their coverage state

#### Scenario: Weather input revision changes expected generation

- **WHEN** an eligible observed or reanalysis weather run revises an interval used by an expected-generation summary
- **THEN** the system creates a new versioned expected result and rebuilds dependent performance summaries without altering the recorded actual generation

#### Scenario: Forecast prerequisites are incomplete

- **WHEN** forecast, expected-generation, or performance statistics lack required configuration, weather, actual telemetry, or coverage
- **THEN** those fields remain unavailable with stable reason codes while unrelated available statistics are returned normally

### Requirement: Missing and suspect data
The system SHALL identify expected-but-missing intervals, duplicate/conflicting source data, invalid or suspect values, counter resets, and incomplete aggregate coverage without fabricating raw observations.

#### Scenario: Uploader stops reporting
- **WHEN** no accepted observation arrives for expected intervals during the configured active window
- **THEN** the query and web application expose a gap with its start, duration, and quality classification

### Requirement: Query performance objectives
On the certified PostgreSQL scale profile, the system SHALL target p95 server latency below 500 ms for a 30-day single-system chart and below 1,000 ms for a 25-year daily single-system chart under the documented concurrent workload, excluding network transfer outside the server.

#### Scenario: Performance suite executes
- **WHEN** the reference query workload runs against the certified dataset and hardware
- **THEN** the report records p50, p95, and p99 latency and fails certification if either p95 objective is exceeded

### Requirement: Analysis export
The system SHALL export authorized query results as documented CSV and JSON with stable column names, explicit timestamps/timezone, units, quality flags, and selected aggregation metadata.

#### Scenario: Chart data is exported
- **WHEN** a user exports the currently selected chart range and series
- **THEN** the downloaded data represents the same filters and values shown by the chart and includes unit and resolution metadata

### Requirement: Forecast and expected-generation series

The system SHALL provide resolution-aware forecast, expected-generation, actual-generation, and performance series with aligned intervals, explicit power/energy/ratio units, lower/central/upper estimates where available, configuration and model boundaries, provider provenance, freshness, and coverage. It SHALL preserve missing intervals and MUST NOT draw continuous modeled or actual values across uncovered gaps.

#### Scenario: Forecast and actual series are charted together

- **WHEN** an authorized client requests compatible actual and modeled series for a bounded range
- **THEN** the response aligns comparable intervals while independently identifying gaps, quality, uncertainty, and the forecast or expected input basis

#### Scenario: Query crosses a model boundary

- **WHEN** the requested range contains multiple effective configurations or model versions
- **THEN** the response identifies each boundary and does not silently aggregate incompatible ratios across it

#### Scenario: Partial capacity is modeled

- **WHEN** a requested system series includes modeled results for only part of the effective DC capacity
- **THEN** the response exposes included and total capacity and marks the series partial rather than scaling it to the whole system

