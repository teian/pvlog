## ADDED Requirements

### Requirement: Resolution-aware series queries
The system SHALL return time-series data at an explicit or automatically selected resolution that honors the requested time range, timezone, fields, aggregation functions, and maximum point budget. Responses SHALL identify actual resolution, units, coverage, gaps, and whether values are raw or aggregated.

#### Scenario: Twenty-five-year chart is requested
- **WHEN** an authorized client requests 25 years of generation with a maximum of 2,000 points
- **THEN** the system selects an appropriate rollup, returns no more than the documented bound plus boundary points, and reports the chosen resolution

#### Scenario: Exact raw points are requested
- **WHEN** an authorized client requests raw resolution for a bounded interval within raw-query limits
- **THEN** the system returns corrected canonical observations ordered by timestamp without substituting rollup values

### Requirement: Daily and lifetime statistics
The system SHALL calculate daily, monthly, yearly, and lifetime generation, consumption, import/export, efficiency, peak, temperature, battery, financial, and data-coverage statistics when the underlying fields are available.

#### Scenario: Late reading changes a daily total
- **WHEN** accepted late data affects a previously summarized local day
- **THEN** the system marks dependent summaries stale, rebuilds them, and serves the corrected total with its coverage state

### Requirement: Missing and suspect data
The system SHALL identify expected-but-missing intervals, duplicate/conflicting source data, invalid or suspect values, counter resets, and incomplete aggregate coverage without fabricating raw observations.

#### Scenario: Uploader stops reporting
- **WHEN** no accepted observation arrives for expected intervals during the configured active window
- **THEN** the query and web application expose a gap with its start, duration, and quality classification

### Requirement: System comparison and ladders
The system SHALL compare authorized or public systems across normalized generation, total generation, efficiency, capacity, location, team, and selected periods while respecting effective capacity and privacy.

#### Scenario: Differently sized systems are compared
- **WHEN** two visible systems with different effective capacities are compared by normalized generation
- **THEN** the result uses the capacity effective for each data period and identifies the normalization unit

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

