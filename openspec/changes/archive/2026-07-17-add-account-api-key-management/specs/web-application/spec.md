## ADDED Requirements

### Requirement: Consolidated account settings

The authenticated web application SHALL provide one `/account` page for personal data, local password changes, and API-key management. It SHALL let the user edit their display name, show their login email as read-only, require and validate current password, new password, and confirmation for password changes, and provide clear success and problem feedback. The former `/account/api-keys` location SHALL redirect to `/account`. The API-key section SHALL only be shown to sessions with credential-management authority.

#### Scenario: User updates their display name

- **WHEN** the user saves a valid changed display name
- **THEN** the page shows the updated value and refreshes the displayed session identity

#### Scenario: Password confirmation does not match

- **WHEN** the user enters different new-password and confirmation values
- **THEN** the page explains the mismatch and does not send a password-change request

#### Scenario: User lacks credential-management authority

- **WHEN** an authenticated user without credential-management authority opens `/account`
- **THEN** personal data and password settings remain available and the API-key controls are not rendered

### Requirement: Complete German and English localization

The authenticated web application SHALL provide German and English values for every user-facing interface element and accessibility label. Locale catalogs SHALL contain the same keys. Navigation and reporting labels and known backend metadata values SHALL be presented in the selected language instead of leaking untranslated English labels or raw enum identifiers. Unknown extensible server values SHALL remain readable through a safe fallback.

#### Scenario: User opens reporting in German

- **WHEN** German is the active language
- **THEN** navigation, reporting headings, lifecycle states, and administration metadata are displayed in German

#### Scenario: User opens the same views in English

- **WHEN** English is the active language
- **THEN** the same elements and accessible names are available in English without missing-key output

#### Scenario: Translation catalogs change

- **WHEN** automated UI checks run
- **THEN** they reject missing DE/EN counterparts and missing statically referenced translation keys

### Requirement: Account API-key management

The authenticated web application SHALL let an account owner create, list, and revoke multiple API keys without exposing a separate account selector. The creation interface SHALL explain each available permission, require at least one permission, make system-changing access visually explicit, and support optional expiry. It SHALL display a newly created key once with a copy action and warning and SHALL never persist that cleartext value in cached query state or redisplay it later.

#### Scenario: Owner creates an upload-only key

- **WHEN** an owner names a key, selects only PV data upload, and confirms creation
- **THEN** the UI displays the new secret once and the resulting list entry identifies only the telemetry-write permission

#### Scenario: Owner creates different keys for different integrations

- **WHEN** an owner creates several keys with different names and permissions
- **THEN** the UI lists each credential independently with its scopes, creation time, optional expiry, and status

#### Scenario: Owner revokes a key

- **WHEN** the owner confirms revocation of a listed key
- **THEN** the UI removes or marks that key revoked, reports success, and does not affect other credentials

#### Scenario: Invalid creation is explained

- **WHEN** the owner omits the name or selects no permissions
- **THEN** the UI prevents submission and identifies the fields that require attention
