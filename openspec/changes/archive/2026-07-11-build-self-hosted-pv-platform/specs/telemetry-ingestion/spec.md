## ADDED Requirements

### Requirement: Canonical telemetry fields
The system SHALL ingest timestamped generation, consumption, grid import/export, power, cumulative energy, temperature, voltage, battery energy/power/state, and registered extended measurements while preserving units, source, receive time, and quality flags.

#### Scenario: Complete reading is accepted
- **WHEN** an authorized client submits a valid timestamp and supported measurement fields with explicit units
- **THEN** the system normalizes them to canonical integer base units and stores their provenance without losing supplied precision

### Requirement: Single and batch ingestion
The modern API SHALL support single and bounded batch ingestion, including an atomic batch mode and a partial-result mode. The system SHALL enforce configured item count, request size, timestamp range, and concurrency limits before exhausting resources.

#### Scenario: Atomic batch contains an invalid item
- **WHEN** any item in an atomic batch fails validation
- **THEN** the system rejects the entire batch and returns indexed validation details for the failed items

#### Scenario: Partial batch contains mixed outcomes
- **WHEN** a partial-result batch contains valid, duplicate, and invalid items
- **THEN** the system commits valid items and returns a stable outcome for every input position

### Requirement: Deterministic retry behavior
The system SHALL support client idempotency keys and canonical observation uniqueness so retried requests cannot double-count energy or create duplicate observations.

#### Scenario: Identical request is retried
- **WHEN** the same principal repeats an ingestion request with the same idempotency key and payload
- **THEN** the system returns the original outcome without inserting or aggregating the data again

#### Scenario: Idempotency key is reused with different content
- **WHEN** a principal reuses an idempotency key for a different canonical payload
- **THEN** the system returns a conflict and preserves the original outcome

### Requirement: Validation and calculation semantics
The system SHALL validate timestamps, configured physical bounds, dependent values, net/cumulative modes, battery state codes, and power/energy consistency. It SHALL derive documented power or energy values only when the selected system calculation mode permits deterministic derivation.

#### Scenario: Physically impossible reading is rejected
- **WHEN** a submitted value exceeds the capacity-adjusted validation policy effective at its timestamp
- **THEN** the system returns a field-specific validation error and stores no canonical reading

#### Scenario: Cumulative reading is converted
- **WHEN** a system configured for cumulative energy receives successive valid cumulative readings
- **THEN** the system derives interval energy using the documented reset and rollover rules while retaining the original cumulative values

### Requirement: Corrections and late data
The system SHALL allow authorized clients to correct or delete an existing observation with optimistic concurrency and SHALL make accepted late data or corrections visible to reads immediately while scheduling affected aggregates for reconciliation.

#### Scenario: Archived observation is corrected
- **WHEN** an authorized correction targets an observation already compacted into immutable storage
- **THEN** the correction is recorded as an overlay, query results reflect it, and an idempotent segment rebuild is queued

### Requirement: Ingestion backpressure
The system SHALL bound ingestion work and return retryable overload responses with `Retry-After` rather than accepting work it cannot durably process.

#### Scenario: Ingestion capacity is saturated
- **WHEN** configured concurrent ingestion or queue-lag thresholds are exceeded
- **THEN** the system rejects new retryable work with an overload problem and emits a saturation metric

