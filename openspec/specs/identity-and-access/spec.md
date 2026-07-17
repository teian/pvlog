# Identity And Access Specification

## Purpose

TBD - created by archiving change build-self-hosted-pv-platform. Update Purpose after archive.
## Requirements
### Requirement: Interactive authentication

The system SHALL authenticate interactive users using enabled local credentials or any enabled external OIDC/OAuth2 connector and SHALL resolve every successful method to one local user. Browser credentials SHALL be represented by revocable server-side sessions in secure cookies, with CSRF protection on state-changing requests.

#### Scenario: OIDC login creates a local session

- **WHEN** a valid OIDC callback is received for a user allowed by instance policy
- **THEN** the system provisions or links the local user and issues a rotated server-side session without exposing provider tokens to the browser application

#### Scenario: Local login creates the same session type

- **WHEN** an enabled local user supplies valid local credentials
- **THEN** the system issues the same normalized server-side session and authorization context used for external login

#### Scenario: Cross-site mutation is rejected

- **WHEN** a browser session submits a state-changing request without a valid CSRF proof
- **THEN** the system rejects the request without changing state

### Requirement: Local user lifecycle and password authentication

The system SHALL provide first-class local user creation, invitation, optional self-registration, activation, email verification policy, disablement, unlock, password change, and password recovery. Password verifiers SHALL use Argon2id with versioned parameters; reset and invitation tokens SHALL be single-use, expiring, and stored only as hashes; authentication SHALL apply enumeration-resistant responses and brute-force controls.

#### Scenario: Administrator creates a local user

- **WHEN** an authorized administrator creates or invites a local user with valid role assignments
- **THEN** the user can complete the configured activation flow and authenticate without an external identity provider

#### Scenario: Password recovery is requested for an unknown address

- **WHEN** password recovery is requested for an address that is not an eligible local login
- **THEN** the public response is indistinguishable from an eligible request and no reset credential is created for another user

#### Scenario: Disabled local user attempts login

- **WHEN** a disabled local user supplies otherwise valid credentials
- **THEN** authentication fails, existing sessions are revoked according to policy, and the event is audited without revealing password data

### Requirement: Multiple external authentication connectors

The system SHALL allow administrators to configure, test, order, enable, disable, and delete multiple protocol-neutral OIDC and OAuth2 Authorization Code connectors with secret references, scopes, endpoints/discovery, claim mappings, PKCE, and callback policy. OIDC validation SHALL include issuer, audience, signature, time, `state`, and `nonce`; OAuth2 login SHALL use `state`, PKCE, and a configured normalized user-info subject.

#### Scenario: Multiple connectors are enabled

- **WHEN** an administrator enables two or more valid external connectors
- **THEN** the login page offers each enabled connector and callbacks are correlated to the initiating connector and browser flow

#### Scenario: Connector configuration test fails

- **WHEN** discovery, endpoint validation, callback configuration, or required claim mapping is invalid
- **THEN** the system reports a safe actionable diagnostic and does not enable the connector

### Requirement: Social login presets

The administration UI and documentation SHALL provide maintained setup presets for Google, GitHub, Facebook, and X that populate the generic OIDC/OAuth2 connector fields and required claim mappings without introducing vendor-named backend settings, DTOs, services, or health contracts.

#### Scenario: Administrator configures a GitHub login

- **WHEN** an administrator selects the GitHub preset and supplies the required application credentials and callback configuration
- **THEN** the saved connector is represented by the generic OAuth2 model and a successful provider callback resolves a stable external subject to a local user

#### Scenario: Preset defaults become obsolete

- **WHEN** a maintained provider changes an endpoint, scope, or claim requirement
- **THEN** the preset can be versioned and updated without migrating the generic local user, session, role, or connector storage model

### Requirement: Safe external identity linking

The system SHALL uniquely identify an external identity by connector and immutable provider subject, support explicit link/unlink workflows with recent reauthentication, prevent one external identity from linking to multiple local users, and prevent removal of a user's final viable login method. Email-based automatic linking SHALL be disabled by default and SHALL require both verified provider email and explicit administrator policy when enabled.

#### Scenario: Signed-in user links another provider

- **WHEN** a signed-in user recently reauthenticates and completes a valid callback for an unclaimed external subject
- **THEN** the system links the subject to that local user and records an audit event

#### Scenario: Provider email matches an existing user

- **WHEN** an external callback supplies an email matching an existing local user but verified-email auto-link policy is not enabled
- **THEN** the system does not silently merge the identities and requires the explicit account-linking flow

#### Scenario: User unlinks the last login method

- **WHEN** a user attempts to unlink the only remaining local or external authentication method
- **THEN** the system rejects the operation until another viable method is established

### Requirement: Hierarchical role-based access control

The system SHALL authorize every protected use case using deny-by-default RBAC with explicit permissions at instance, account, and system scope. It SHALL provide built-in owner, administrator, manager, contributor, viewer, and auditor roles and SHALL allow authorized account administrators to define constrained custom roles without delegating permissions they do not possess. New accounts, systems, roles, and data SHALL default to the minimum access required.

#### Scenario: Viewer cannot modify telemetry

- **WHEN** a viewer attempts to ingest, correct, or delete measurements for a system
- **THEN** the system returns a forbidden response and records no measurement change

#### Scenario: Owner delegates access

- **WHEN** a system owner grants a supported membership role to another user
- **THEN** that user receives exactly the capabilities defined for the role

#### Scenario: Owner delegates restricted system administration

- **WHEN** a system owner assigns another user a manager, contributor, viewer, auditor, or constrained custom role at system scope
- **THEN** that user receives only the role's permissions for that system and receives no implicit access to the owning account or another system

#### Scenario: Account manager creates a custom role

- **WHEN** an account manager creates a custom role from permissions they are authorized to delegate
- **THEN** assignments of that role grant exactly those permissions within the account and cannot affect instance administration or another account

#### Scenario: External and local users have the same role

- **WHEN** one user authenticated locally and another through an external connector hold the same role assignments
- **THEN** authorization evaluates the same effective permissions for both users

### Requirement: Personal ownership and tenancy boundary

The system SHALL present the authenticated user account as the ownership context for that user's PV systems and SHALL NOT require a separate system-account selection or assignment. An internal account scope MAY be used for authorization and storage routing, but it SHALL be provisioned automatically for a user without existing delegated access and SHALL share that user's stable identifier. Additional users SHALL access an owner's systems only through explicit scoped role assignments.

#### Scenario: User has no existing system access

- **WHEN** an active authenticated user without an existing ownership or delegated-access context opens a session
- **THEN** the system automatically provisions the user's internal ownership scope, assigns the user the owner role, and allows system creation without asking for or displaying a separate system account

#### Scenario: User accesses another account by identifier

- **WHEN** a user supplies a valid system or resource identifier owned by an account for which they have no permission
- **THEN** the system denies the request without opening or exposing that account's data database

#### Scenario: Additional administrator accesses an owner's system

- **WHEN** an owner grants another user a restricted role for a system
- **THEN** the additional user accesses that system with the delegated permissions without receiving or selecting a separate system account

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

### Requirement: Configurable quotas and rate limits

The system SHALL enforce administrator-configurable request and ingestion quotas per principal, return modern rate-limit metadata, and reproduce documented legacy rate-limit metadata when requested.

#### Scenario: Modern quota is exceeded

- **WHEN** a principal exceeds its configured request budget
- **THEN** the system returns a retryable problem response with limit, remaining, and reset information

### Requirement: Security audit trail

The system SHALL append tamper-evident audit records for authentication events, credential lifecycle events, authorization changes, system privacy changes, destructive data operations, imports, exports, and administrative actions without recording secrets.

#### Scenario: API token is revoked

- **WHEN** an authorized user revokes an API token
- **THEN** the audit trail records actor, action, token identifier, timestamp, result, and request identifier but not the token value

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

