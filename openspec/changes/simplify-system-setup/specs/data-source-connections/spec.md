# Spec Delta

## Purpose

Represents each uploader or import that feeds a PV system as a named data source with its own credential, ready-to-use configuration and visible connection health.

## ADDED Requirements

### Requirement: Data source lifecycle

The system SHALL let an authorized user create, rename, list and delete data sources that belong to exactly one PV system. Each data source SHALL have a name and a source profile. Deleting a data source SHALL revoke its credential and SHALL retain the readings it delivered.

#### Scenario: Data source is created

- **WHEN** a user with system management permission creates a data source with a name and a supported profile
- **THEN** the data source is stored for that system and listed with the state never connected

#### Scenario: Data source is deleted

- **WHEN** a user deletes a data source
- **THEN** its credential stops authenticating and readings previously delivered through it remain stored

#### Scenario: Unsupported profile is requested

- **WHEN** a creation request names a profile the instance does not provide
- **THEN** the system rejects the request with a validation problem that lists the supported profiles

### Requirement: Source profiles

The system SHALL provide the source profiles smalog, SBFspot, generic uploader and import, and SHALL expose for each profile its identifier, display name, the configuration snippets it offers and the measurement channel keys it defines.

#### Scenario: Profiles are listed

- **WHEN** a client requests the available source profiles
- **THEN** the response contains each profile with its snippet kinds and well-known channel keys including unit and target kind

### Requirement: One-time uploader configuration

Creating a data source or regenerating its credential SHALL return, exactly once, the server URL, the system identifier, the cleartext credential and profile-specific configuration snippets that are usable without further editing. The cleartext credential SHALL NOT be retrievable afterwards.

#### Scenario: smalog data source is created

- **WHEN** a user creates a data source with the smalog profile
- **THEN** the response contains an export section for smalog's configuration file with server URL and system identifier filled in and the credential referenced through an environment variable, together with that variable's assignment

#### Scenario: SBFspot data source is created

- **WHEN** a user creates a data source with the SBFspot profile
- **THEN** the response contains an environment configuration for the SBFspot uploader with server URL, system identifier and credential filled in

#### Scenario: Generic data source is created

- **WHEN** a user creates a data source with the generic profile
- **THEN** the response contains a complete example request that submits one reading to that system with that credential

#### Scenario: Configuration is requested again

- **WHEN** a client reads an existing data source
- **THEN** the response contains the server URL and system identifier and no credential

### Requirement: Credential regeneration

The system SHALL let an authorized user regenerate the credential of a data source. Regeneration SHALL invalidate the previous credential immediately and SHALL return a new one-time configuration.

#### Scenario: Credential is regenerated

- **WHEN** a user regenerates the credential of a data source
- **THEN** the previous credential is rejected on its next use and the new configuration is returned once

### Requirement: Connection state

The system SHALL derive for each data source a state of never connected, receiving, stale or disconnected, together with the time of its last accepted reading and the count of readings accepted in the last day. A source SHALL be stale when its last accepted reading is older than three status intervals of its system, and disconnected when its credential is revoked or expired.

#### Scenario: Source stops sending

- **WHEN** a data source has delivered readings and none within three status intervals
- **THEN** its state is stale and the time of its last accepted reading is reported

#### Scenario: Credential is revoked elsewhere

- **WHEN** the credential of a data source is revoked from the account's API key list
- **THEN** the data source is reported as disconnected and offers regeneration

### Requirement: Reading attribution

The system SHALL record for every accepted reading the data source it was delivered through and SHALL make that attribution available when readings are inspected.

#### Scenario: Two sources feed one system

- **WHEN** two data sources of the same system deliver readings
- **THEN** each reading is attributed to its own data source and each source reports its own last reading time

### Requirement: Equipment discovery proposals

When a data source reports device identity or per-input values for a device that matches no inverter of the system, the system SHALL record a discovery proposal instead of creating equipment. An authorized user SHALL be able to adopt a proposal, which creates the inverter and its strings and binds the related measurement channels, or dismiss it.

#### Scenario: Unknown inverter serial is reported

- **WHEN** a source delivers values for an inverter serial that no inverter of the system has
- **THEN** a proposal with the reported serial, name, model, firmware and observed MPPT trackers is listed for that system and no equipment is created

#### Scenario: Proposal is adopted

- **WHEN** a user adopts a proposal for an inverter with two reporting MPPT trackers
- **THEN** an inverter with two strings is created and the channels of that serial are bound to the new inverter and strings

#### Scenario: Tracker never reports power

- **WHEN** a device reports an MPPT tracker whose power has always been zero or missing
- **THEN** that tracker is not included in the proposal

#### Scenario: Tracker numbers are sparse

- **WHEN** a device reports trackers 1 and 5
- **THEN** the proposal contains two strings that keep the numbers 1 and 5

#### Scenario: Proposal is dismissed

- **WHEN** a user dismisses a proposal
- **THEN** the same device is not proposed again unless the user resets the dismissal
