# Notifications And Integrations Specification

## Purpose
TBD - created by archiving change build-self-hosted-pv-platform. Update Purpose after archive.
## Requirements
### Requirement: Alert rules
The system SHALL let authorized users configure alert rules for missing generation, low generation, high consumption, high/low net power, standby cost, performance, battery state, and extended channel thresholds with timezone-aware schedules, debounce, cooldown, and enabled delivery channels.

#### Scenario: Missing generation alert fires once
- **WHEN** a system exceeds its configured active-window idle threshold
- **THEN** the system creates one alert event and suppresses repeats until the cooldown or recovery transition permits another event

### Requirement: Secure webhooks
The system SHALL register, verify, update, disable, and delete webhook subscriptions with selected event types. Deliveries SHALL include an event identifier, timestamp, schema version, and keyed signature over the exact payload.

#### Scenario: Consumer verifies a webhook
- **WHEN** a webhook delivery is sent
- **THEN** the consumer can verify its signature, timestamp tolerance, subscription identifier, and stable event identifier using the documented procedure

### Requirement: Reliable delivery lifecycle
Webhook deliveries SHALL be queued transactionally with their originating event, attempted with bounded exponential backoff and jitter, protected from duplicate side effects through stable event identifiers, and moved to an inspectable dead-letter state after the configured limit.

#### Scenario: Endpoint recovers after failures
- **WHEN** a webhook endpoint returns retryable failures and later succeeds
- **THEN** the system retries within policy, records every attempt, and marks the delivery complete without creating a new event identity

### Requirement: Webhook network safety
Webhook registration and delivery SHALL validate schemes and destinations, re-resolve DNS safely, block loopback/link-local/private targets by default, limit redirects and response bodies, and permit local targets only through explicit administrator policy.

#### Scenario: Public hostname resolves to loopback
- **WHEN** a registered public hostname resolves to a blocked address at delivery time
- **THEN** the delivery is refused as a security failure and no connection is made to that address

### Requirement: Pluggable insolation and regional providers
The system SHALL integrate optional insolation and regional supply providers through bounded adapters with timeouts, cache policy, provenance, licensing metadata, and circuit-breaker health. Core telemetry operations SHALL remain available during provider failure.

#### Scenario: Insolation provider is unavailable
- **WHEN** an insolation request cannot be satisfied from a fresh cache and the provider circuit is open
- **THEN** the system returns an explicit temporarily unavailable result while ingestion and stored-data queries continue

### Requirement: Integration observability
The system SHALL expose safe operational metrics and administrative delivery history for alert evaluation lag, webhook attempts/outcomes, provider latency/errors, circuit state, cache freshness, and dead-letter counts.

#### Scenario: Administrator diagnoses failed callback
- **WHEN** a webhook exhausts delivery retries
- **THEN** an authorized administrator can inspect timestamps, safe response metadata, error classification, and retry history without viewing stored secrets

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

