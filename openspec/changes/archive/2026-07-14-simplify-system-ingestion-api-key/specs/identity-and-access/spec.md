## ADDED Requirements

### Requirement: System ingestion API key lifecycle

The system SHALL allow any authenticated active user of the owning account to issue, name, rename, list, regenerate/rotate, and revoke high-entropy ingestion API keys bound to exactly one PV system in that account. The cleartext key SHALL be displayed only once, stored only as a non-reversible keyed hash with a lookup identifier, limited to telemetry ingestion, and independent of interactive users, OAuth/OIDC connectors, browser sessions, and general bearer API tokens.

#### Scenario: Account user creates an ingestion key

- **WHEN** an authenticated active account user creates an ingestion key for a system owned by that account
- **THEN** the system returns the cleartext key and copyable push URL once and subsequently exposes only safe metadata such as key ID, name, system, creation time, last-used time, and status

#### Scenario: Key is used for another system

- **WHEN** a valid ingestion key bound to one system is presented to an ingestion endpoint for a different system
- **THEN** the system rejects the request without revealing whether either system or credential exists

#### Scenario: Account user renames a key

- **WHEN** an authenticated active account user renames a key for a system owned by that account
- **THEN** subsequent metadata shows the new name without changing or redisplaying the credential

#### Scenario: Account user revokes or regenerates a key

- **WHEN** an authenticated active account user revokes or regenerates a key for a system owned by that account
- **THEN** the retired key cannot authenticate any subsequent ingestion request while previously accepted observations remain unchanged

### Requirement: Safe ingestion key transport and handling

The system SHALL accept a system ingestion key only through the `x-pvlog-api-key` request header or the credential segment of a documented generated push URL. It SHALL reject query-parameter keys and ambiguous requests containing multiple credential transports, SHALL require HTTPS outside explicitly configured development mode, and SHALL redact key material from application/proxy logs, traces, metrics, audit metadata, problem details, referrers, and user interfaces after initial display.

#### Scenario: Header key authenticates an uploader

- **WHEN** an uploader sends a valid system key in `x-pvlog-api-key` to that system's ingestion endpoint
- **THEN** the system authenticates the ingestion principal without requiring OAuth, OIDC, cookies, CSRF, or another authorization header

#### Scenario: Generated push URL authenticates an uploader

- **WHEN** an uploader posts to the generated HTTPS push URL containing its valid opaque credential segment
- **THEN** the system authenticates the same system-scoped ingestion principal and applies the same authorization and quotas as header transport

#### Scenario: Credential could be exposed diagnostically

- **WHEN** request processing, rejection, tracing, access logging, or auditing records the URL or authentication metadata
- **THEN** the system records a normalized route and safe key identifier only and never records the cleartext header or URL credential

#### Scenario: Multiple key transports are supplied

- **WHEN** a request contains a header key together with a URL credential or query-parameter credential
- **THEN** the system rejects the request with a generic authentication problem and does not attempt ingestion
