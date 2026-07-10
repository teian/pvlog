## ADDED Requirements

### Requirement: PV system lifecycle
The system SHALL let authorized users create, read, update, archive, restore, and permanently delete PV systems within exactly one owning account, with name, description, IANA timezone, commissioning date, capacity, geographic location, country, visibility, status interval, and data-calculation settings.

#### Scenario: System is created with safe defaults
- **WHEN** an authorized user creates a system with the required name, timezone, and capacity
- **THEN** the system creates a private active system inside the selected authorized account with generated UUIDv7 identifiers and validated default calculation settings

#### Scenario: Invalid timezone is rejected
- **WHEN** a system update supplies a timezone that is not in the supported IANA database
- **THEN** the system returns a field-specific validation error and preserves the prior configuration

### Requirement: Equipment model
The system SHALL represent one or more arrays, inverters, meters, batteries, orientations, and capacity changes over effective date ranges without rewriting historical telemetry.

#### Scenario: Capacity changes over time
- **WHEN** an owner records an additional array with a past or future effective date
- **THEN** statistics and physical validation use the capacity configuration effective for each measurement date

### Requirement: Tariffs and financial settings
The system SHALL store effective-dated import, export, and time-of-use tariffs and SHALL calculate financial summaries from versioned tariff settings while retaining the inputs used for each calculation.

#### Scenario: Tariff is replaced
- **WHEN** a new tariff becomes effective on a given local date
- **THEN** earlier summaries retain the old tariff basis and later summaries use the new tariff basis

### Requirement: Extended channel definitions
The system SHALL let owners define named extended measurement channels with stable identifiers, value type, unit, scale, valid range, display metadata, and effective lifecycle.

#### Scenario: Extended value is validated
- **WHEN** telemetry includes a value for a registered extended channel
- **THEN** the system validates the value against that channel's type and range and preserves its unit semantics

### Requirement: Data portability and erasure
The system SHALL export an authorized system's configuration and complete measurement history in a versioned, checksummed portable format and SHALL permanently erase the system and its owned data only through an explicit, auditable confirmation workflow.

#### Scenario: Complete system export
- **WHEN** an owner requests a full export
- **THEN** the resulting bundle contains a manifest, system metadata, equipment, channel definitions, raw measurements, corrections, and integrity hashes while excluding reusable secrets by default

#### Scenario: Permanent deletion is confirmed
- **WHEN** an owner completes the configured destructive confirmation and retention checks
- **THEN** the system schedules verifiable erasure and records the action in the audit trail

### Requirement: Bulk metadata import
The system SHALL provide a dry-run and commit workflow for importing systems and configuration, with deterministic validation reports and stable source identifiers.

#### Scenario: Import dry run finds errors
- **WHEN** an import bundle contains invalid equipment dates or duplicate source identifiers
- **THEN** the system reports all actionable errors and writes no imported configuration
