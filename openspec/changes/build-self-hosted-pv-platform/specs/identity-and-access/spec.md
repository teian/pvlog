## ADDED Requirements

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

#### Scenario: Account manager creates a custom role
- **WHEN** an account manager creates a custom role from permissions they are authorized to delegate
- **THEN** assignments of that role grant exactly those permissions within the account and cannot affect instance administration or another account

#### Scenario: External and local users have the same role
- **WHEN** one user authenticated locally and another through an external connector hold the same role assignments
- **THEN** authorization evaluates the same effective permissions for both users

### Requirement: Account tenancy boundary
The system SHALL organize users and PV systems into accounts, SHALL allow a user to belong to multiple accounts, and SHALL require every PV system to belong to exactly one account. Authorization and storage routing SHALL resolve the account before account-owned data is accessed.

#### Scenario: User accesses another account by identifier
- **WHEN** a user supplies a valid system or resource identifier owned by an account for which they have no permission
- **THEN** the system denies the request without opening or exposing that account's data database

#### Scenario: User switches accounts
- **WHEN** a user who belongs to multiple accounts selects another authorized account
- **THEN** subsequent account-scoped operations use that account's roles, quotas, and storage boundary

### Requirement: Scoped modern API tokens
The system SHALL issue high-entropy API tokens that are displayed once, stored only as non-reversible keyed hashes, scoped to explicit actions and systems, optionally expire, and can be revoked independently.

#### Scenario: Scoped token is accepted
- **WHEN** a valid non-expired token with telemetry write scope targets an allowed system
- **THEN** the system authenticates the token principal and permits the authorized ingestion operation

#### Scenario: Revoked token is rejected
- **WHEN** a previously issued token has been revoked
- **THEN** every subsequent request using it is rejected without revealing whether its identifier once existed

### Requirement: Legacy PVOutput credentials
The system SHALL support per-system PVOutput-compatible API keys with independent read-only or read-write policy and SHALL map legacy authentication to the same authorization model as modern credentials.

#### Scenario: Legacy header authentication succeeds
- **WHEN** a client supplies a matching `X-Pvoutput-Apikey` and `X-Pvoutput-SystemId`
- **THEN** the compatibility API authenticates the system principal and enforces the key policy

#### Scenario: Read-only legacy key attempts a write
- **WHEN** a read-only legacy key calls a compatibility service that mutates state
- **THEN** the system returns the documented forbidden legacy error and makes no change

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
