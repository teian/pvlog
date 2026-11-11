# Spec Delta

## ADDED Requirements

### Requirement: System-restricted upload credentials

The system SHALL issue credentials that are restricted to one PV system, carry only the telemetry write scope, and belong to one data source. Such a credential SHALL authenticate ingestion for its system only, SHALL be rejected for every other system and every other operation, and SHALL be created, regenerated and revoked only by a principal permitted to manage that system.

#### Scenario: Credential targets its own system

- **WHEN** a data source credential submits a valid reading for its system
- **THEN** the reading is accepted

#### Scenario: Credential targets another system of the same account

- **WHEN** a data source credential submits a reading for a different system owned by the same account
- **THEN** the request is rejected without revealing whether that system exists

#### Scenario: Credential is used to read

- **WHEN** a data source credential requests telemetry, system configuration or account data
- **THEN** the request is rejected

#### Scenario: User without management permission requests a credential

- **WHEN** a user who may only read a system attempts to create or regenerate a data source credential
- **THEN** the request is rejected and no credential is issued

### Requirement: Audit of data source credentials

The system SHALL record creation, regeneration and revocation of data source credentials in the security audit trail with the acting principal, the system and the data source, and SHALL never record the cleartext credential.

#### Scenario: Credential is regenerated

- **WHEN** a user regenerates a data source credential
- **THEN** an audit event names the actor, system and data source and contains no secret material
