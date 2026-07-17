# Equipment Catalog Specification

## Purpose
TBD - created by archiving change add-equipment-catalog. Update Purpose after archive.

## Requirements
### Requirement: Bundled offline equipment catalog
The system SHALL ship a versioned, offline catalog containing inverter and solar-module entries with stable identifiers, manufacturer, model, catalog revision, and provenance metadata.

#### Scenario: Installation has no internet access
- **WHEN** an authenticated user opens equipment selection on an offline installation
- **THEN** the system returns the inverter and solar-module entries bundled with the running release without contacting an external service

#### Scenario: Catalog revision changes
- **WHEN** a software release contains an updated catalog
- **THEN** the API reports the new catalog revision while retaining stable identifiers for unchanged entries

### Requirement: Normalized inverter technical data
Each inverter catalog entry SHALL represent manufacturer/model identity and typed regular technical data. DC input data SHALL include topology, total string-input count, MPPT count, strings per MPPT, maximum input voltage, start voltage, nominal input voltage, MPPT voltage range, maximum operating input current, and maximum short-circuit current when published. Ratings that differ between trackers SHALL be representable per MPPT.

AC and operational data SHALL include phase count, nominal grid voltage and supported voltage/frequency ranges, rated/maximum active power, maximum apparent power, maximum output current, power-factor range, harmonic distortion, maximum and weighted efficiency, standby consumption, operating/derating limits, cooling, acoustic noise, ingress protection, humidity, altitude, communication interfaces, dimensions, and weight when published.

#### Scenario: Inverter detail is requested
- **WHEN** a client requests an inverter entry by its stable identifier
- **THEN** the system returns the available ratings in documented normalized integer units and represents unpublished optional ratings as absent rather than zero

#### Scenario: Multi-MPPT string topology is represented
- **WHEN** an inverter provides two MPPT trackers with two string inputs on the first tracker and one string input on the second tracker
- **THEN** the entry reports two MPPTs, three total string inputs, and the per-MPPT string allocation without flattening it into an ambiguous count

#### Scenario: MPPTs have different electrical limits
- **WHEN** an inverter datasheet publishes different maximum operating and short-circuit currents for individual MPPTs
- **THEN** each tracker records its own current limits in milliamperes while shared voltage limits remain available at inverter level

#### Scenario: Inverter voltage window is represented
- **WHEN** a datasheet provides maximum DC voltage, start voltage, nominal DC voltage, and minimum/maximum MPPT voltage
- **THEN** the catalog retains all four voltage concepts separately in millivolts and validates that the configured ranges are ordered plausibly

### Requirement: Normalized solar-module technical data
Each solar-module catalog entry SHALL represent manufacturer/model identity, cell technology, bifacial properties, Pmax, Voc, Vmp, Isc, Imp, module efficiency, signed Isc/Voc/Pmax temperature coefficients, maximum system voltage, operating-temperature range, maximum series fuse, front/rear static load, dimensions, and weight when published by the manufacturer.

#### Scenario: Complete module datasheet is represented
- **WHEN** a catalog module has values such as 450 W Pmax, 39.30 V Voc, 14.48 A Isc, signed temperature coefficients, 1762 × 1134 × 30 mm dimensions, and 22.0 kg weight
- **THEN** the system returns lossless normalized integer values that the UI can format back into those engineering units

#### Scenario: Bifacial module is represented
- **WHEN** a module datasheet declares an 80 percent bifaciality factor with plus/minus 10 percent tolerance
- **THEN** the entry records bifacial capability, an 8000-basis-point nominal factor, and a 1000-basis-point tolerance

### Requirement: Catalog validation and provenance
The build and server startup SHALL reject catalog assets with duplicate identifiers, missing provenance, invalid units, implausible ranges, inconsistent cross-field ratings, or nondeterministic ordering.

#### Scenario: Invalid electrical relationship is added
- **WHEN** a proposed module entry contains a maximum-power voltage greater than its open-circuit voltage
- **THEN** catalog validation fails and identifies the entry and invalid field relationship

#### Scenario: Datasheet source is missing
- **WHEN** a catalog entry has no source name or source reference
- **THEN** catalog validation rejects the entry before it can ship

### Requirement: Searchable read-only catalog API
The system SHALL provide authenticated, read-only inverter and solar-module list/detail APIs with normalized text search, manufacturer filtering, bounded pagination, deterministic ordering, and catalog revision metadata.

#### Scenario: User searches by manufacturer and model
- **WHEN** an authenticated user searches solar modules using a manufacturer filter and partial model text
- **THEN** the API returns only matching entries in deterministic order with a bounded result count

#### Scenario: Anonymous catalog request
- **WHEN** a request without a valid principal accesses a catalog endpoint
- **THEN** the system denies the request without exposing catalog contents

### Requirement: Optional editable catalog prefilling
The catalog SHALL only assist data entry. Selecting an entry SHALL copy its technical values into an editable equipment form, and the system SHALL persist the values explicitly confirmed for the installation. A catalog entry identifier and revision MAY be retained as template provenance but SHALL NOT require the saved values to equal the catalog.

#### Scenario: Bundled catalog is corrected later
- **WHEN** a later release changes a rating in the referenced catalog entry
- **THEN** existing configured equipment retains its confirmed values and is not silently updated

#### Scenario: User edits prefilled values
- **WHEN** a client supplies a catalog identifier together with technical values that differ from that catalog revision
- **THEN** the server validates and saves the confirmed technical values while recording that the catalog template was customized

#### Scenario: Newer template is reapplied
- **WHEN** an authorized user explicitly chooses to reapply a newer catalog revision
- **THEN** the UI prefills the newer values for review but does not persist them until the user confirms the form

### Requirement: Unrestricted manual equipment entry
The system SHALL allow authorized users to configure an inverter or solar module entirely from their own valid values regardless of whether a matching catalog entry exists. Manual equipment SHALL have the same configuration capabilities as catalog-prefilled equipment.

#### Scenario: Model is not listed
- **WHEN** an authorized user selects custom equipment and enters valid manufacturer, model, and required ratings
- **THEN** the system saves the confirmed values without a catalog identifier and without blocking any downstream system configuration

#### Scenario: Matching catalog model exists
- **WHEN** an authorized user prefers manual entry even though the catalog contains the model
- **THEN** the system permits manual configuration without requiring selection of the catalog entry

### Requirement: String module composition and capacity
Each inverter-owned PV string SHALL record a positive module count, module manufacturer, module model, and peak power per module in watts. The system SHALL derive total string peak power as module count multiplied by per-module peak power and SHALL use the derived value for installed-capacity calculations.

#### Scenario: String is configured manually
- **WHEN** an authorized user configures a string with 18 modules from a stated manufacturer and model at 450 W per module
- **THEN** the system stores the confirmed module composition and returns a total string peak power of 8100 W

#### Scenario: Module catalog entry prefills the string
- **WHEN** a user selects a 450 W module catalog entry during string setup
- **THEN** manufacturer, model, and 450 W per-module power are prefilled while module count remains required and every prefilled value remains editable

#### Scenario: Client submits a contradictory total
- **WHEN** a client submits 18 modules at 450 W per module together with a total other than 8100 W
- **THEN** the server rejects the contradictory total or ignores it and returns the server-derived 8100 W value

#### Scenario: Invalid module count or power is entered
- **WHEN** module count or per-module peak power is zero, negative, outside configured bounds, or their product overflows
- **THEN** the system rejects the string configuration with a field-specific validation error

### Requirement: Catalog-backed accessible equipment workflow
Onboarding and equipment administration SHALL provide keyboard-accessible, localized inverter and solar-module catalog search as an optional prefilling aid, editable technical fields, a technical-data review, catalog revision/provenance display, and an equally accessible manual-entry path.

#### Scenario: Module is selected during string setup
- **WHEN** a keyboard user selects a solar module from the catalog while configuring an inverter string
- **THEN** the UI prefills and displays manufacturer, model, module peak power, and normalized technical data, requires module count, shows the calculated total string power, keeps every prefilled field editable, and saves only after confirmation

#### Scenario: No matching entry exists
- **WHEN** search returns no matching equipment
- **THEN** the UI keeps the manual-entry workflow available without blocking system setup

