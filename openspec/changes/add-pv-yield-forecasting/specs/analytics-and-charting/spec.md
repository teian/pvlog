## MODIFIED Requirements

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

## ADDED Requirements

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
