## ADDED Requirements

### Requirement: Current-user profile resource

The API SHALL expose cookie-authenticated profile read and update operations at `/api/v1/account/profile` without accepting a user or account identifier. Reading SHALL return only the current user's identifier, email, and display name. Updating SHALL accept only a display name and SHALL return the updated safe profile. Failures SHALL use the standard problem-details representation.

#### Scenario: Current user reads their profile

- **WHEN** an authenticated user requests the current-user profile
- **THEN** the API returns that user's safe profile without credential or authorization internals

#### Scenario: Current user updates their profile

- **WHEN** an authenticated user submits a valid display name with CSRF protection
- **THEN** the API returns the updated profile and does not change the login email

### Requirement: Current-account API-key lifecycle resource

The API SHALL expose cookie-authenticated API-key collection operations at `/api/v1/account/api-keys` and revocation at `/api/v1/account/api-keys/{apiKeyId}` without accepting an account identifier from the client. Creation SHALL accept a name, a non-empty unique set of documented scopes, and an optional future expiry; listing and revocation SHALL never return secret material. All failures SHALL use the standard problem-details representation.

#### Scenario: Owner creates a least-privilege key

- **WHEN** an authenticated owner posts a valid name and `telemetry:write` scope
- **THEN** the API returns `201`, safe credential metadata, and the cleartext bearer key exactly once

#### Scenario: Owner lists keys

- **WHEN** an authenticated owner gets the API-key collection
- **THEN** the API returns deterministic safe metadata for only that account and no digest or cleartext key field

#### Scenario: Owner revokes a key

- **WHEN** an authenticated owner deletes one of the account's API-key resources
- **THEN** the API returns `204` and the credential can no longer authenticate

#### Scenario: Request submits an unknown scope

- **WHEN** creation contains an unsupported scope or no scopes
- **THEN** the API returns an actionable validation problem and creates no credential

### Requirement: Operation-specific bearer scope enforcement

Every bearer-enabled operation SHALL document and enforce its required scope using `systems:read`, `systems:write`, `telemetry:read`, or `telemetry:write`. Account ownership and optional system restrictions SHALL be checked in addition to the action scope.

#### Scenario: Telemetry writer attempts system configuration

- **WHEN** a credential containing only `telemetry:write` calls a system update operation
- **THEN** the API returns a generic authorization problem and does not modify the system

#### Scenario: Telemetry reader queries PV data

- **WHEN** a credential containing `telemetry:read` queries telemetry for a system owned by its account
- **THEN** the API permits the query without requiring `systems:write`

## MODIFIED Requirements

### Requirement: Stable simple uploader endpoints

The API SHALL accept telemetry uploads only on the canonical system observation routes using `Authorization: Bearer` with an account API key containing `telemetry:write`. It SHALL NOT expose a custom upload-key header, credential-bearing push URL, or query-parameter credential transport. The canonical routes SHALL retain their existing request and response telemetry schemas.

#### Scenario: Bearer endpoint is documented

- **WHEN** an account user or uploader opens the API documentation
- **THEN** the documentation provides a copyable `curl` example using `Authorization: Bearer`, JSON content type, idempotency key, and the canonical system observation URL

#### Scenario: Legacy upload-key transport is attempted

- **WHEN** a client submits `x-pvlog-api-key` or calls a former credential-bearing push route
- **THEN** the API does not authenticate that credential transport

#### Scenario: Scoped bearer client ingests telemetry

- **WHEN** a client uses a valid account API key with `telemetry:write` on the canonical ingestion endpoint for a system owned by its account
- **THEN** the API accepts the request without requiring a separate system ingestion key
