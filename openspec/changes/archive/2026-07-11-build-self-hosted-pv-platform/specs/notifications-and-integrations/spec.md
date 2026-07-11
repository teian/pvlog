## ADDED Requirements

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

