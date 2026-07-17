## ADDED Requirements

### Requirement: Provider-neutral weather forecast inputs

The system SHALL integrate optional weather forecast providers through bounded provider-neutral adapters that normalize immutable forecast runs with provider/configuration identity, issue and fetch times, valid horizon, interval resolution, spatial coverage, irradiance and required environmental inputs, explicit units, uncertainty when supplied, license/provenance metadata, and cache freshness. Core telemetry operations SHALL remain available when forecast providers fail.

#### Scenario: Provider forecast is normalized

- **WHEN** an enabled adapter receives a valid provider response covering a configured system location
- **THEN** the system validates and stores an ordered immutable normalized run without exposing provider-specific fields to the yield domain

#### Scenario: Provider response is invalid

- **WHEN** a response has unsupported units, unordered or overlapping intervals, an invalid horizon, missing required irradiance fields, or no coverage for the system location
- **THEN** the adapter rejects the run with a safe diagnostic and preserves the last valid cached run

#### Scenario: Provider is temporarily unavailable

- **WHEN** a fresh run cannot be fetched and a permitted cached run exists
- **THEN** the system may serve it with explicit stale age and provenance while the circuit breaker and provider health report degraded operation

#### Scenario: No usable run exists

- **WHEN** the provider is unavailable and no cache entry satisfies stale policy
- **THEN** forecast requests return temporarily unavailable while ingestion, actual-generation queries, and previously stored results continue operating

### Requirement: Separate forecast and observed weather provenance

The system SHALL classify normalized external weather inputs as forecast, observed, or reanalysis data and retain their issue time, validity, revisions, provenance, and licensing independently. Forecast inputs MUST NOT be relabeled as observed data after the forecast interval elapses.

#### Scenario: Historical expected generation selects inputs

- **WHEN** observed or reanalysis weather covers a historical interval
- **THEN** the calculation records that specific input run and classification as the expected-generation basis

#### Scenario: Forecast and observation overlap

- **WHEN** a prior forecast and later observed weather cover the same interval
- **THEN** both runs remain queryable and the system uses the observed or reanalysis run for generation-performance calculations
