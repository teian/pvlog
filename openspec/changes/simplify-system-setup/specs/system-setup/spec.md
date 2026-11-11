# Spec Delta

## Purpose

Guides a user from an empty account to a PV system that verifiably receives data, and reports for every system which setup steps are complete and which one is recommended next.

## ADDED Requirements

### Requirement: Minimal system creation

The system SHALL create a PV system from a name alone. Location, coordinates, timezone, declared capacity, status interval, commissioning date, description and equipment SHALL be optional at creation and changeable later. When no timezone is supplied the system SHALL derive it from the country of the supplied location and otherwise from the timezone the client reports.

#### Scenario: System is created with only a name

- **WHEN** an authorized user submits a system with a valid name and no other field
- **THEN** the system is created without inverters, with a derived timezone and the default status interval, and is immediately usable as an ingestion target

#### Scenario: Timezone follows the location

- **WHEN** a user creates a system with a location in a country that has one timezone and no explicit timezone
- **THEN** the stored timezone is that country's timezone, regardless of the client's timezone

#### Scenario: Country has several timezones

- **WHEN** a user creates a system with coordinates in a country that has several timezones and no explicit timezone
- **THEN** the stored timezone is the country's timezone whose reference position is nearest to the coordinates

#### Scenario: Location has no country

- **WHEN** a user creates a system with coordinates but without a country and no explicit timezone
- **THEN** the stored timezone is the one the client reports

#### Scenario: Name is missing

- **WHEN** a user submits a system without a name
- **THEN** the system rejects the request with a field-specific validation problem and creates nothing

### Requirement: Atomic system creation with equipment

When a creation request includes inverters and strings, the system SHALL create the system and all submitted equipment as one operation that either completes entirely or changes nothing.

#### Scenario: Equipment is invalid

- **WHEN** a creation request contains a valid system and a string with an invalid module count
- **THEN** no system and no equipment is created and the problem response identifies the invalid field

### Requirement: Resumable guided setup

The guided setup SHALL consist of the steps system, data source and verification, SHALL persist the system when the first step completes, and SHALL allow the remaining steps to be skipped and completed later for the same system.

#### Scenario: Setup is interrupted after the first step

- **WHEN** a user completes the system step and leaves the setup
- **THEN** the system exists and its setup status reports the data source step as the recommended next step

#### Scenario: Data source step is skipped

- **WHEN** a user chooses to connect a data source later
- **THEN** the setup finishes without creating a data source or credential

### Requirement: Setup status

The system SHALL expose for each PV system an ordered setup status whose items cover location, capacity, data source, first data, measurement channels, equipment, tariff and forecast readiness. Each item SHALL carry a state of done, recommended, optional or blocked and a machine-readable reason. The status SHALL be derived from stored configuration and telemetry at request time.

#### Scenario: New system without a data source

- **WHEN** the setup status of a system with only a name is requested
- **THEN** the data source item is recommended, first data is blocked by the missing data source, and all other incomplete items are optional

#### Scenario: Working system with missing details

- **WHEN** a system has a data source and stored readings but no tariff and no location
- **THEN** no item is recommended and the tariff and location items are optional

#### Scenario: Unassigned channel has received values

- **WHEN** a measurement channel with stored values is not bound to the system, an inverter or a string
- **THEN** the measurement channels item is not done and its reason states the number of unassigned channels

### Requirement: First-data verification

The system SHALL report whether a PV system has received at least one reading that is not a test reading, and SHALL return the most recent reading in normalized form together with the data source it arrived through.

#### Scenario: First reading arrives

- **WHEN** a data source delivers its first accepted reading
- **THEN** the first data item becomes done and the verification result contains the normalized values, the measurement time and the data source name

#### Scenario: Only a test reading exists

- **WHEN** the only stored reading of a system is a test reading
- **THEN** the verification result shows that reading as a test and the first data item is not done

### Requirement: Test reading

The system SHALL let an authorized user send one clearly marked test reading through a data source. Test readings SHALL be excluded from energy totals, statistics and forecasts and SHALL be removable by the user.

#### Scenario: Test reading is sent

- **WHEN** a user triggers a test reading for a data source
- **THEN** the reading is stored with a test marker, appears in the verification result, and does not change any energy total

#### Scenario: Test reading is removed

- **WHEN** a user removes the test readings of a system
- **THEN** only readings carrying the test marker are deleted
