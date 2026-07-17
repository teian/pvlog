# Pv System Management Specification

## Purpose

TBD - created by archiving change build-self-hosted-pv-platform. Update Purpose after archive.
## Requirements
### Requirement: PV system lifecycle

The system SHALL let authorized users create, read, update, archive, restore, and permanently delete PV systems within exactly one owning account, with name, description, IANA timezone, commissioning date, capacity, geographic location, country, visibility, status interval, and data-calculation settings.

#### Scenario: System is created with safe defaults

- **WHEN** an authorized user creates a system with the required name, timezone, and capacity
- **THEN** the system creates a private active system inside the selected authorized account with generated UUIDv7 identifiers and validated default calculation settings, and grants the creating user the non-expiring built-in owner role at exactly that system's scope

#### Scenario: Invalid timezone is rejected

- **WHEN** a system update supplies a timezone that is not in the supported IANA database
- **THEN** the system returns a field-specific validation error and preserves the prior configuration

### Requirement: System aggregate and equipment model

The PV system SHALL be the aggregate root for one or more inverters, and each inverter SHALL contain one or more PV strings. Strings SHALL own panel count, panel metadata, orientation, tilt, rated capacity, and effective dates. Batteries, meters, sensors, and other auxiliary equipment SHALL remain attached to the system without bypassing the inverter/string ownership hierarchy for generation equipment.

#### Scenario: System hierarchy is changed

- **WHEN** an owner adds or updates a PV string
- **THEN** the change is validated and persisted through its containing inverter and system aggregate, and a string cannot belong to an inverter from another system

#### Scenario: Connected-array power is submitted

- **WHEN** a client supplies panel count and rated watts per panel for an inverter's connected array
- **THEN** the server derives and returns total array power and rejects contradictory catalog snapshot values without requiring the client to repeat calculated totals

#### Scenario: Inverter is removed from the submitted configuration

- **WHEN** an owner removes an existing inverter in the management interface and saves the system
- **THEN** the inverter and its connected array records are deleted instead of remaining as hidden stale configuration

#### Scenario: Capacity changes over time

- **WHEN** an owner records an additional string with a past or future effective date
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

### Requirement: Forecast-ready effective string configuration

The PV system SHALL use each inverter string's effective-dated module count, confirmed module manufacturer and model, per-module peak power, server-derived aggregate peak power, orientation, tilt, and confirmed technical snapshot as authoritative forecast inputs. It SHALL support effective-dated bounded loss and calibration settings separately from equipment identity, aggregate effective DC nameplate capacity from string to inverter and system without double counting, and expose forecast-input completeness without making incomplete systems invalid for ordinary telemetry use.

#### Scenario: String configuration is forecast-ready

- **WHEN** an authorized user saves a string with valid module composition, peak power, orientation, tilt, and forecast settings
- **THEN** the system reports the effective string, inverter, and system DC capacities and marks the configured forecast inputs complete

#### Scenario: Catalog-prefilled values are customized

- **WHEN** a user edits module or forecast-relevant values prefilled from a catalog entry
- **THEN** the system persists the confirmed installation snapshot and uses it for forecasts without requiring equality to the catalog template

#### Scenario: Forecast inputs are incomplete

- **WHEN** an existing string lacks orientation, tilt, location, or another required input
- **THEN** the system continues accepting authorized telemetry but reports the affected forecast scope as incomplete with actionable missing-field reasons

#### Scenario: Effective string capacity changes

- **WHEN** module composition or an inverter string becomes effective or ceases to be effective at a configuration boundary
- **THEN** capacity aggregation and forecast calculations use the old configuration before the boundary and the new configuration after it

