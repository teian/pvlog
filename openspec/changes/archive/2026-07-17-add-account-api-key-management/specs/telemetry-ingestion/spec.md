## MODIFIED Requirements

### Requirement: Simple system-key ingestion

The single and bounded batch telemetry ingestion operations SHALL accept an account API key with `telemetry:write` through standard bearer authentication without requiring OAuth, OIDC, browser authentication, or a token exchange. The API SHALL verify that the credential's account owns the target system and SHALL apply the same normalization, validation, idempotency, duplicate detection, backpressure, persistence, rate-limit, and audit behavior as every canonical ingestion request.

#### Scenario: Simple uploader sends one reading

- **WHEN** a device sends a valid observation JSON body with a bearer API key containing `telemetry:write`
- **THEN** the system validates account ownership and applies the normal canonical ingestion behavior

#### Scenario: Upload-only API key sends a batch

- **WHEN** a device sends a valid bounded batch with an account API key containing only `telemetry:write`
- **THEN** the system accepts the batch but the same key cannot read telemetry or modify system configuration

#### Scenario: API key targets another account's system

- **WHEN** an otherwise valid telemetry-write key targets a system not owned by its account
- **THEN** the system rejects the request without revealing system or credential ownership details

#### Scenario: Payload validation fails

- **WHEN** a bearer-authenticated device submits an invalid telemetry payload
- **THEN** the system returns the normal field-specific validation problem and stores no invalid canonical observation
