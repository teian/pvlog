## ADDED Requirements

### Requirement: Accessible yield forecast and performance experience

The web application SHALL provide responsive English/German views for forward power and energy forecasts, expected-versus-actual historical generation, forecast realization, and generation performance at supported system and inverter scopes. Charts SHALL have keyboard-accessible data-table and textual-summary alternatives and SHALL expose units, time range, freshness, issue time, uncertainty, coverage, configuration/model version, provenance, and missing/partial/unavailable reasons.

#### Scenario: User reviews tomorrow's forecast

- **WHEN** an authorized user opens a system with a current weather forecast
- **THEN** the interface shows the forecast curve and energy summary with issue time, horizon, uncertainty, provider attribution, and last-update state

#### Scenario: User investigates underperformance

- **WHEN** actual and historical expected generation have sufficient compatible coverage
- **THEN** the interface compares both energy values and labels their ratio as generation performance without calling it inverter efficiency

#### Scenario: Forecast is stale or unavailable

- **WHEN** the latest forecast is stale, partial, or unavailable
- **THEN** the interface shows the relevant state and reason without plotting missing values as zero or hiding existing actual-generation data

#### Scenario: User accesses the data without a chart

- **WHEN** a keyboard or assistive-technology user selects the tabular alternative
- **THEN** the interface exposes the same intervals, values, units, uncertainty, freshness, coverage, and provenance in a logical reading and focus order

### Requirement: Forecast configuration guidance

The web application SHALL let authorized users review forecast-input completeness and manage bounded loss/calibration settings while preserving catalog-prefilled equipment values as editable confirmed snapshots. It SHALL explain which inputs affect nameplate capacity, forecast generation, expected generation, generation performance, and inverter efficiency.

#### Scenario: Configuration is incomplete

- **WHEN** a string lacks required location, orientation, tilt, module, or model input
- **THEN** the interface identifies the exact missing fields, links to the relevant configuration, and leaves ordinary telemetry functions available

#### Scenario: User changes a loss assumption

- **WHEN** an authorized user saves a valid effective-dated loss or calibration setting
- **THEN** the interface confirms the effective boundary, invalidates affected modeled results, and shows recalculation progress without changing actual telemetry
