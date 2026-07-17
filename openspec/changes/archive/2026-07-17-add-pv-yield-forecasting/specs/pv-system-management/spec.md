## ADDED Requirements

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
