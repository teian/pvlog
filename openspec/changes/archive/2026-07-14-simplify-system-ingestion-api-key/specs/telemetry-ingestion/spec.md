## ADDED Requirements

### Requirement: Simple system-key ingestion

The single and bounded batch telemetry ingestion operations SHALL accept a valid system ingestion API key through either the documented header endpoint or generated push URL without requiring OAuth, OIDC, browser authentication, or a token exchange. Both transports SHALL execute the same normalization, validation, idempotency, duplicate detection, backpressure, persistence, rate-limit, and audit behavior.

#### Scenario: Simple uploader sends one reading

- **WHEN** a device sends a valid observation JSON body with its system API key using either supported transport
- **THEN** the system accepts the observation for the bound system and returns the same success representation as other authorized modern ingestion clients

#### Scenario: Simple uploader sends a batch

- **WHEN** a device sends a bounded atomic or partial-result batch with its system API key
- **THEN** the system applies the documented batch semantics and returns stable indexed outcomes without requiring a different credential

#### Scenario: Uploader retries a request

- **WHEN** a key-authenticated uploader repeats an ingestion request with the same idempotency key and payload
- **THEN** the system returns the original outcome without double-counting or inserting the observation again

#### Scenario: Valid key submits invalid telemetry

- **WHEN** a key-authenticated request contains invalid timestamps, units, fields, bounds, or contradictory values
- **THEN** the system returns the normal field-specific validation problem and stores no invalid canonical observation

#### Scenario: Key quota is exhausted

- **WHEN** an ingestion key exceeds its configured request or ingestion quota
- **THEN** the system returns retryable rate-limit metadata without affecting other keys for the same system
