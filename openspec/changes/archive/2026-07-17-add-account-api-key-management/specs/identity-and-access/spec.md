## ADDED Requirements

### Requirement: Self-service account identity

The system SHALL let every cookie-authenticated active user read their own profile and change their display name without supplying a user or account identifier. It SHALL display the login email as non-editable until a verified email-change workflow exists. A local password change SHALL require the current password and SHALL revoke the user's other active sessions after success. Bearer credentials SHALL NOT use these self-service identity operations.

#### Scenario: User changes their display name

- **WHEN** an authenticated active user submits a valid new display name
- **THEN** the system updates only that user's profile and returns the updated safe profile data

#### Scenario: User changes their local password

- **WHEN** an authenticated local user supplies the correct current password and a valid new password
- **THEN** the password is replaced and the user's other active sessions are revoked

#### Scenario: API key attempts to edit a profile

- **WHEN** a bearer API key calls a current-user profile or password endpoint
- **THEN** the system rejects the request without changing identity data

## MODIFIED Requirements

### Requirement: Scoped modern API tokens

The system SHALL allow an authenticated account owner to issue multiple independently named high-entropy API tokens in that user's implicit account context. Each token SHALL be displayed once, stored only as a non-reversible keyed hash, restricted to an explicit non-empty set drawn from system read, system write, telemetry read, and telemetry write scopes, optionally expire, and be independently listed and revoked. Tokens SHALL be account-bound, SHALL NOT manage other credentials, and SHALL NOT receive an unselected scope implicitly.

#### Scenario: Upload-only token is accepted

- **WHEN** a valid non-expired token has only telemetry write scope and targets a system owned by its account
- **THEN** the system permits telemetry ingestion and denies telemetry reads and system changes

#### Scenario: Read-only token cannot mutate a system

- **WHEN** a token with system read and telemetry read scopes attempts to update system or equipment configuration
- **THEN** the system rejects the operation without making a change

#### Scenario: Account owner creates several independent tokens

- **WHEN** an authenticated owner creates differently named tokens with different scope sets
- **THEN** each token receives an independent identifier, secret, expiry, scope set, and revocation lifecycle

#### Scenario: Token secret is displayed once

- **WHEN** token creation succeeds
- **THEN** the response displays the cleartext token once and every later list response exposes only safe metadata

#### Scenario: Revoked token is rejected

- **WHEN** a previously issued token has been revoked
- **THEN** every subsequent request using it is rejected without revealing whether its identifier once existed

#### Scenario: Token cannot create another token

- **WHEN** a bearer API token calls the account API-key lifecycle endpoints
- **THEN** the system denies the request even if the token has system write scope

## REMOVED Requirements

### Requirement: System ingestion API key lifecycle

**Reason:** Account API keys now provide the same upload capability through an explicit `telemetry:write` scope, so a parallel system-bound credential lifecycle is redundant and confusing.

**Migration:** Create an account API key with `telemetry:write`, update the uploader to use bearer authentication on the canonical observation route, and revoke/discard the former system ingestion key.

### Requirement: Safe ingestion key transport and handling

**Reason:** The custom `x-pvlog-api-key` header and credential-bearing push URLs are replaced by the standard bearer transport used by every account API key.

**Migration:** Replace the custom header or generated push URL with `Authorization: Bearer <api-key>` and use `/api/v1/systems/{system_id}/observations` or its batch variant.
