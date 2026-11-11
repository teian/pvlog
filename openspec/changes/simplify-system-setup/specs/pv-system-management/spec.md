# Spec Delta

## MODIFIED Requirements

### Requirement: System aggregate and equipment model

The PV system SHALL be the aggregate root for zero or more inverters, and each inverter SHALL contain one or more PV strings. Strings SHALL own panel count, panel metadata, orientation, tilt, rated capacity, and effective dates. Batteries, meters, sensors, and other auxiliary equipment SHALL remain attached to the system without bypassing the inverter/string ownership hierarchy for generation equipment.

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

#### Scenario: System has no equipment

- **WHEN** a system without inverters receives telemetry
- **THEN** system-level readings are stored and shown, and views that need strings report equipment as the missing input

## ADDED Requirements

### Requirement: Declared system capacity

The system SHALL store an optional declared capacity per PV system. It SHALL use the declared capacity for display and physical validation while the system has no strings, and SHALL use the capacity derived from strings once any string exists, reporting the declared value as overridden.

#### Scenario: Capacity without equipment

- **WHEN** a system has a declared capacity and no strings
- **THEN** the system reports that capacity and marks it as declared

#### Scenario: Strings are added later

- **WHEN** strings are added to a system that has a declared capacity
- **THEN** the reported capacity is the sum derived from the strings

### Requirement: Location, timezone and reporting interval

The system SHALL let an authorized user set and change a system's location label, coordinates, timezone, status interval and commissioning date, SHALL return them when the system is read, and SHALL reject coordinates outside valid ranges, unknown timezones and status intervals outside the supported bounds.

#### Scenario: Location is saved

- **WHEN** a user saves a location with coordinates for a system
- **THEN** reading the system returns the same label and coordinates and forecast readiness no longer reports a missing location

#### Scenario: Status interval is changed

- **WHEN** a user changes the status interval of a system
- **THEN** staleness detection and interval-based energy derivation use the new interval for later readings

#### Scenario: Timezone is invalid

- **WHEN** a user submits a timezone that is not a known zone identifier
- **THEN** the system rejects the change with a field-specific validation problem

### Requirement: String loss inputs

The system SHALL persist each string's temperature coefficient and its shading entries and SHALL return them unchanged when the string is read.

#### Scenario: Shading entry is saved

- **WHEN** a user adds a shading entry with a time range and a loss percentage to a string and saves
- **THEN** reading the string returns that entry

### Requirement: Stable string identity

Saving an inverter SHALL keep the identifier of every string that is resubmitted with its identifier, SHALL create a string for every entry without one, and SHALL remove only strings that are left out. Measurement channels bound to a kept string SHALL remain bound.

#### Scenario: Inverter is edited

- **WHEN** an owner renames an inverter and saves it with its existing strings
- **THEN** each string keeps its identifier and every channel bound to it stays bound

#### Scenario: String is left out

- **WHEN** an owner saves an inverter without one of its former strings
- **THEN** that string is removed and the other strings keep their identifiers

#### Scenario: Unknown string identifier is submitted

- **WHEN** a save names a string identifier that does not belong to the inverter
- **THEN** the system rejects the request without changing the inverter

### Requirement: Strings without module identity

The system SHALL accept a string that states panel count and power per panel without a module manufacturer or model. Forecast readiness SHALL report the missing module identity for such a string.

#### Scenario: Array is described by capacity only

- **WHEN** an owner saves a string with panel count, power per panel, orientation and tilt and no module manufacturer or model
- **THEN** the string is stored and contributes to the system's capacity

#### Scenario: Forecast for a string without module identity

- **WHEN** forecast readiness is evaluated for a string without module manufacturer or model
- **THEN** the string is reported as not forecast-ready with module identity as the reason

### Requirement: Typed measurement channels

Each measurement channel SHALL have a data type, a unit, a decimal scale and an aggregation kind, and its values SHALL be stored without loss at that scale. A channel key defined by a source profile SHALL be registered with the profile's definition; any other unknown key SHALL be registered as unassigned with the unit the client supplied.

#### Scenario: Profile key is first seen

- **WHEN** a reading contains a channel key that a source profile defines as a string voltage
- **THEN** the channel is registered with the profile's unit, scale and aggregation kind

#### Scenario: Decimal value is stored

- **WHEN** a reading supplies a decimal value for a channel
- **THEN** the value is returned with the same precision when the channel is read

#### Scenario: Value does not fit the channel

- **WHEN** a reading supplies a value whose unit or type contradicts the channel's definition
- **THEN** that value is rejected with a problem that names the channel and the reading's other values are processed

### Requirement: Channel binding and aggregation

The system SHALL let an authorized user bind a measurement channel to the system, an inverter or a string of the same system, and SHALL aggregate bound channels at their target according to their aggregation kind. Rebinding a channel SHALL rebuild the affected aggregates.

#### Scenario: Channel is bound to an inverter

- **WHEN** a user binds a power channel to an inverter
- **THEN** that inverter's aggregates include the channel's values

#### Scenario: Channel targets foreign equipment

- **WHEN** a binding names an inverter or string of another system
- **THEN** the system rejects the binding

### Requirement: Measurement channels on every supported database

Measurement channel discovery, storage, binding and aggregation SHALL behave identically on every database the product supports.

#### Scenario: Channel reading on the server database

- **WHEN** a reading with channel values is submitted to an instance running on the server database
- **THEN** the channels are registered and their values stored as on the embedded database
