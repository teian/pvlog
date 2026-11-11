# Spec Delta

## ADDED Requirements

### Requirement: Lifetime energy counters

The ingestion operations SHALL accept lifetime generation and consumption counters in addition to interval energy. When only a counter is supplied, the system SHALL derive interval energy from the difference to the previous counter of the same source, SHALL treat a decreasing counter as a reset, and SHALL NOT derive energy across a reset or a missing predecessor.

#### Scenario: Counter increases

- **WHEN** a source submits a lifetime generation counter that is higher than its previous one
- **THEN** the difference is stored as interval generation energy and the counter is stored as supplied

#### Scenario: Counter resets

- **WHEN** a source submits a lifetime counter lower than its previous one
- **THEN** no interval energy is derived for that reading and the reading is flagged as a counter reset

#### Scenario: Interval and counter are both supplied

- **WHEN** a reading contains interval energy and a lifetime counter
- **THEN** both are stored and the supplied interval energy is used

### Requirement: Grid import and export

The ingestion operations SHALL accept grid import and export as power and as energy, SHALL store them in canonical units, and SHALL include them in interval, daily and lifetime aggregates.

#### Scenario: Net metering reading

- **WHEN** a reading supplies grid export power and grid import energy
- **THEN** both values are stored and appear in the system's aggregates for that period

#### Scenario: Import and export contradict

- **WHEN** a reading supplies import and export power that are both greater than zero for a system configured for net flow
- **THEN** the reading is rejected with a field-specific validation problem

### Requirement: Temperature and voltage as measurements

The system SHALL store supplied temperature and voltage as queryable measurements and SHALL provide minimum and maximum temperature and mean voltage per aggregation period.

#### Scenario: Temperature is aggregated

- **WHEN** readings with temperature are stored for a day
- **THEN** the daily aggregate reports that day's minimum and maximum temperature

### Requirement: Reading message

The ingestion operations SHALL accept an optional text message of at most 30 characters per reading and SHALL return it when the reading is inspected.

#### Scenario: Message is too long

- **WHEN** a reading carries a message longer than 30 characters
- **THEN** the reading is rejected with a field-specific validation problem

### Requirement: Per-device measurements

The ingestion operations SHALL accept, within one reading, measurements for individual inverters identified by the source's device identifier: power, current and voltage per AC phase, grid frequency, grid import and export power, daily and lifetime energy, operating and feed-in time, device and grid-relay status codes, temperature, signal quality, and power, current and voltage for any number of MPPT trackers numbered 1 to 255.

#### Scenario: Two inverters report in one reading

- **WHEN** a reading contains values for two device identifiers
- **THEN** each device's values are stored separately and the system-level generation equals what the reading states for the system

#### Scenario: Tracker values are submitted

- **WHEN** a reading contains DC power, current and voltage for a tracker bound to a string
- **THEN** the values are stored for that string and its aggregates are updated

#### Scenario: Sparse tracker numbers

- **WHEN** a reading contains trackers 1 and 5 for one inverter
- **THEN** both are stored under their own numbers and no tracker 2 to 4 is created

#### Scenario: Value is missing

- **WHEN** a reading omits a value the device did not report
- **THEN** the value is stored as missing and is not treated as zero in any aggregate

### Requirement: Unified device ingestion

The system SHALL accept, for one system, a single request that carries device identities, timestamped measurements with system-level values and per-device phase, tracker, battery and extra values, archive energy samples, daily summaries and device events, with integer values in the units their field names state. A member that is absent SHALL be stored as missing. The request SHALL be validated completely before anything is stored.

#### Scenario: Logger sends a complete cycle

- **WHEN** a logger submits one request with two devices, their measurements, a daily summary and an event
- **THEN** all items are stored and each value reads back unchanged for its device

#### Scenario: Item is invalid

- **WHEN** one measurement of a request contains a state of charge outside its range
- **THEN** nothing from that request is stored and the response identifies the item by index and reason

#### Scenario: Unknown member is sent

- **WHEN** a request contains a member the declared schema version does not define
- **THEN** the request is rejected with a field-specific problem

#### Scenario: Source has no device detail

- **WHEN** a request contains only system-level values per measurement
- **THEN** the measurements are stored as system readings without any device

### Requirement: Identity-keyed idempotency

Items of the ingestion API SHALL be identified by device and measurement time, device and date, or device and event identifier, without a client-supplied idempotency key. Resubmitting an identical item SHALL store nothing; resubmitting an item with changed values SHALL replace the earlier values.

#### Scenario: Request is resubmitted

- **WHEN** the same request is submitted twice
- **THEN** the second response reports every item as unchanged and no stored value differs

#### Scenario: Value matures

- **WHEN** a daily summary is resubmitted with higher energy and marked complete
- **THEN** the stored summary has the new values and exists once

### Requirement: Ingestion cursor

Every ingestion response SHALL state per category the stored, unchanged and rejected counts and the latest measurement time, sample time, summary date and event time held per device. The system SHALL also return these marks on request for the calling data source.

#### Scenario: Logger advances its cursor

- **WHEN** a request is accepted
- **THEN** the response contains the marks up to which the logger need not send again

#### Scenario: Logger lost its state

- **WHEN** a data source requests the ingestion state of its system
- **THEN** it receives the latest held marks per device and category for the data it delivered

### Requirement: Bulk ingestion and discoverable limits

The ingestion API SHALL accept several thousand measurements per request up to a documented limit, SHALL accept compressed request bodies, and SHALL publish its supported schema versions, item and size limits and accepted encodings through a capabilities resource.

#### Scenario: Compressed bulk request

- **WHEN** a compressed request within the published limit is submitted
- **THEN** it is accepted and processed as an uncompressed request would be

#### Scenario: Limit is exceeded

- **WHEN** a request exceeds the published item or size limit
- **THEN** it is rejected with a problem that states the limit

#### Scenario: Limits are discovered

- **WHEN** a client requests the capabilities
- **THEN** the response lists the supported schema versions, limits and encodings

### Requirement: Extra device values

The ingestion API SHALL accept per device additional named values with a unit, and SHALL store each as a typed measurement channel of that device.

#### Scenario: Vendor-specific value is sent

- **WHEN** a measurement contains an extra value with a key and unit not known before
- **THEN** a measurement channel with that unit is registered for the device and the value is stored

### Requirement: System values derived from devices

For measurements that carry device values the system SHALL derive system generation power, lifetime and interval energy, grid import and export and battery state from the devices that reported them, SHALL use an explicitly supplied system value in preference to a derived one, SHALL leave a system value missing when no device reported it, and SHALL NOT derive interval energy across a counter reset or a change in the set of reporting devices.

#### Scenario: One of two inverters is silent

- **WHEN** a measurement contains values for only one of two known inverters
- **THEN** system power is that inverter's power, and no interval energy is derived from the lifetime sum for that measurement

#### Scenario: No phase power reported

- **WHEN** a measurement contains an inverter without any AC phase power
- **THEN** that inverter contributes nothing to system power and system power is missing if no other inverter reported

### Requirement: Battery measurements

The ingestion operations SHALL accept battery state of charge, power and energy for the system and state of charge, voltage, current and temperature per device, SHALL store them in canonical units, and SHALL make the latest state and the per-period change available.

#### Scenario: Battery state is submitted

- **WHEN** a reading contains a battery state of charge and voltage for an inverter
- **THEN** both values are stored for that inverter and the system's latest battery state reflects them

#### Scenario: State of charge is out of range

- **WHEN** a reading supplies a state of charge above 100 percent
- **THEN** the reading is rejected with a field-specific validation problem

### Requirement: Device identity

The ingestion operations SHALL accept per device the serial number, device family identifier, configured name, reported name, model, firmware version and transport, SHALL keep the latest reported identity per device, and SHALL update a matched inverter's model and firmware when they change.

#### Scenario: Firmware changes

- **WHEN** a source reports a new firmware version for a serial that matches an inverter
- **THEN** the inverter shows the new firmware version and the change time

### Requirement: PVOutput data parity

The system SHALL accept and store every measurement a PVOutput status carries: generation energy and power, consumption energy and power, temperature, voltage, cumulative counters, net import and export, six extended decimal values and a text message. The documentation SHALL map each PVOutput field to its PVLog field.

#### Scenario: Complete PVOutput-equivalent reading

- **WHEN** a client submits a reading containing the equivalent of every PVOutput status field
- **THEN** every supplied value is stored and returned unchanged in its canonical unit

#### Scenario: Mapping is documented

- **WHEN** a user consults the field mapping reference
- **THEN** each PVOutput status, output and system field is listed with its PVLog field or with the statement that it is derived

### Requirement: smalog data parity

The system SHALL accept and store every plant value of smalog's canonical schema: inverter identity, inverter measurements, MPPT measurements, battery measurements, archive energy samples, daily yields with their completeness flag, site consumption and device events. Integer values SHALL be stored in smalog's units without rounding. The documentation SHALL map each smalog column to its PVLog field and SHALL do so through the unified ingestion API without a smalog-specific resource.

#### Scenario: Poll cycle is uploaded

- **WHEN** a smalog poll cycle for two inverters with three phases, sparse trackers and one battery is submitted through the ingestion API
- **THEN** every reported value reads back unchanged for its inverter, tracker or battery and missing values read back as missing

#### Scenario: Contract fixture is accepted

- **WHEN** the published smalog fixture payload is submitted to a test instance
- **THEN** every item is accepted and the contract test finds each fixture value in the stored data

#### Scenario: History is backfilled while live data arrives

- **WHEN** stored smalog history is uploaded for a period that overlaps readings already delivered live
- **THEN** no reading, daily yield or event is duplicated

#### Scenario: Archive sample without live measurement

- **WHEN** an archive energy sample exists for a time at which no live measurement was stored
- **THEN** the sample's lifetime energy and power are stored for that inverter and contribute to derived interval energy

### Requirement: SBFspot data parity

The SBFspot uploader SHALL deliver, per inverter serial, the values SBFspot records under the same keys the smalog profile uses: AC power, current and voltage per phase, DC power, current and voltage for trackers 1 and 2, grid frequency, device temperature, daily and lifetime energy, operating and feed-in time, device status, grid relay state and signal quality, together with consumption where recorded and the inverter's serial, name, type and software version.

#### Scenario: Multi-inverter database is uploaded

- **WHEN** the uploader reads a database with two inverter serials
- **THEN** PVLog stores per-inverter and per-input values for both serials and system totals equal to their sum

#### Scenario: Optional tables are absent

- **WHEN** the database has no detailed spot values or no consumption table
- **THEN** the uploader delivers the values that exist and reports which groups were unavailable

#### Scenario: Inverter is replaced

- **WHEN** a serial stops reporting and a new serial starts
- **THEN** no energy is derived across the change and the new serial produces a discovery proposal

### Requirement: Daily summary ingestion

The system SHALL accept a daily summary per system and local date, and optionally per device, containing generated, exported and consumed energy, the lifetime counter at the end of the day, a completeness flag, peak power and its time, a weather condition, minimum and maximum temperature and imported energy by tariff band. Resubmitting a summary for the same source, device and date SHALL replace the earlier one.

#### Scenario: Day has only a supplied summary

- **WHEN** a daily summary is stored for a day without interval readings
- **THEN** daily, monthly and yearly statistics include that day and mark it as supplied

#### Scenario: Current day is updated

- **WHEN** an incomplete summary for the current day is resubmitted with higher energy and later marked complete
- **THEN** each submission replaces the previous one without creating a duplicate

### Requirement: Daily value precedence

For each day the system SHALL report a supplied summary that is marked complete first, otherwise values computed from interval readings with sufficient coverage, otherwise an incomplete supplied summary, and SHALL state which basis was used.

#### Scenario: Complete inverter yield exists

- **WHEN** a day has interval readings and complete per-inverter daily yields from the source
- **THEN** the day's generation is the sum of the supplied yields and the computed value remains retrievable

#### Scenario: Supplied summary is incomplete

- **WHEN** a day has interval readings with sufficient coverage and a supplied summary that is not marked complete
- **THEN** statistics use the computed values

### Requirement: Device events

The ingestion operations SHALL accept timestamped device events with a device identifier, the source's event identifier, an event code, a type, a severity category, a group, a message, an optional old and new value and a user group. An event SHALL be stored once per device and source event identifier, and events SHALL be available per system and per inverter.

#### Scenario: Events are uploaded twice

- **WHEN** the same event is submitted in two uploads
- **THEN** it is stored once

#### Scenario: Parameter change event

- **WHEN** an event carries an old and a new value
- **THEN** both values are returned when the event is read

### Requirement: History import

The system SHALL import PVOutput status and output export files for one system through a dry run followed by an explicit commit. The dry run SHALL report the covered range, accepted and rejected rows with reasons, and overlap with stored data. A committed import SHALL apply the same normalization and validation as live ingestion, SHALL mark readings as imported, and SHALL be repeatable without creating duplicates.

#### Scenario: Dry run reports problems

- **WHEN** a user uploads a file in which some rows have invalid timestamps
- **THEN** the dry run lists those rows with their reasons and stores nothing

#### Scenario: Import is committed twice

- **WHEN** the same file is committed a second time
- **THEN** no reading or daily summary is duplicated and the result reports the rows as already present

#### Scenario: Import overlaps live data

- **WHEN** an import contains timestamps for which readings from a live data source exist
- **THEN** the existing readings are kept and the overlapping rows are reported as skipped

#### Scenario: File exceeds the size limit

- **WHEN** a user uploads a file larger than the configured import limit
- **THEN** the system rejects it and states the limit
