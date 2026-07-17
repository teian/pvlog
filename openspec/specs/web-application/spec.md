# Web Application Specification

## Purpose
TBD - created by archiving change build-self-hosted-pv-platform. Update Purpose after archive.
## Requirements
### Requirement: Guided onboarding
The web application SHALL guide an administrator through instance readiness and a user through authentication, first-system configuration, equipment/capacity, timezone, API/uploader credentials, a test reading, and verification of live data.

#### Scenario: First system receives data
- **WHEN** a user completes onboarding and submits the documented test reading
- **THEN** the UI confirms ingestion, displays the normalized reading, and links to uploader and troubleshooting documentation

### Requirement: Operational dashboard
The web application SHALL provide system selection, current power/energy status, freshness, daily and lifetime KPIs, recent alerts, data quality, ingestion health, and responsive light/dark layouts.

#### Scenario: Latest data is stale
- **WHEN** the most recent expected reading is older than the configured threshold
- **THEN** the dashboard visibly labels it stale with the last received time and does not imply that old power is live

### Requirement: Interactive historical charts
The web application SHALL provide keyboard-operable range, resolution, series, aggregation, timezone, comparison, zoom, and export controls for generation, consumption, grid, battery, environmental, financial, and extended data.

#### Scenario: User selects a long range
- **WHEN** a user changes a chart from one day to 25 years
- **THEN** the UI requests a bounded point budget, displays the returned resolution and coverage, and remains interactive without loading every raw observation

### Requirement: Data quality and correction workflows
Authorized users SHALL be able to inspect missing/suspect intervals, source conflicts, rejected ingestion, counter resets, and aggregate lag, then submit audited corrections or initiate reprocessing within their permissions.

#### Scenario: User corrects a suspect point
- **WHEN** a manager edits a suspect historical reading and confirms the change
- **THEN** the UI uses optimistic concurrency, displays reconciliation progress, and eventually shows corrected charts and statistics

### Requirement: Administration workflows
The web application SHALL expose authorized management for local users, invitations, password recovery/disablement, roles and permission assignments, linked identities, generic external OIDC/OAuth2 connectors, Google/GitHub/Facebook/X setup presets, sessions, memberships, systems/equipment, credentials, tariffs, channels, alerts, webhooks, imports/exports, jobs, storage health, backups, and instance configuration.

#### Scenario: Non-administrator opens instance settings
- **WHEN** a regular system owner navigates to instance administration
- **THEN** the UI and API deny access without exposing configuration or secret metadata

#### Scenario: Administrator configures external login
- **WHEN** an instance administrator selects a supported preset or generic OIDC/OAuth2 connector and completes its required fields
- **THEN** the UI tests the protocol-neutral configuration, stores secrets only through the backend secret mechanism, and enables login only after validation succeeds

### Requirement: Frontend architecture and runtime boundaries
The frontend SHALL use `src/ui/` itself as the React/Vite source root, with Feature-Sliced Design directories such as `app/`, `pages/`, `widgets/`, `features/`, `entities/`, and `shared/` directly beneath it and no nested `src/ui/src/` directory. It SHALL use strict TypeScript, native `fetch` through TanStack Query, Zod validation at external boundaries, Zustand only for ephemeral UI state, local package/assets, and deployment configuration loaded from `/runtime-config.json`. All frontend test files, fixtures, mocks, and test-support source SHALL live beneath the repository-root `tests/` directory rather than beside production UI code.

#### Scenario: API response violates its schema
- **WHEN** a frontend data boundary receives a response that fails its Zod schema
- **THEN** the feature reports a safe localized error and telemetry signal rather than rendering unchecked data

#### Scenario: Frontend test discovery runs
- **WHEN** the frontend unit, component, or browser test commands execute
- **THEN** they discover test code beneath `tests/ui/` or `tests/e2e/` and no test-only module is required beneath `src/ui/`

#### Scenario: Frontend source layout is validated
- **WHEN** the repository structure check examines the UI
- **THEN** it accepts production modules directly beneath `src/ui/` and rejects creation of a nested `src/ui/src/` source root

### Requirement: Internationalization and accessibility
The web application SHALL ship English and German translations, use localized dates/numbers/units, and conform to WCAG 2.1 AA with semantic landmarks, keyboard access, visible focus, sufficient contrast, reduced motion, accessible names, and non-visual chart alternatives.

#### Scenario: Chart is used without a pointer
- **WHEN** a keyboard or screen-reader user interacts with a chart
- **THEN** the user can select series/range and access an equivalent localized data summary or table without relying only on color or hover

### Requirement: Frontend performance and resilience
The production application SHALL target initial usable load below two seconds on the documented reference client/network, cancel obsolete requests, bound cached/query data, and preserve actionable error/retry states during API or network failures.

#### Scenario: User rapidly changes chart ranges
- **WHEN** several chart requests are superseded by a newer selection
- **THEN** obsolete requests are cancelled or ignored and only the newest selection updates the view

### Requirement: Accessible yield forecast and performance experience

The web application SHALL provide responsive English/German views for forward power and energy forecasts, expected-versus-actual historical generation, forecast realization, and generation performance at supported system and inverter scopes. Charts SHALL have keyboard-accessible data-table and textual-summary alternatives and SHALL expose units, time range, freshness, issue time, uncertainty, coverage, configuration/model version, provenance, and missing/partial/unavailable reasons.

#### Scenario: User reviews tomorrow's forecast

- **WHEN** an authorized user opens a system with a current weather forecast
- **THEN** the interface shows the forecast curve and energy summary with issue time, horizon, uncertainty, provider attribution, and last-update state

#### Scenario: User investigates underperformance

- **WHEN** actual and historical expected generation have sufficient compatible coverage
- **THEN** the interface compares both energy values and labels their ratio as generation performance without calling it inverter efficiency

#### Scenario: Forecast is stale or unavailable

- **WHEN** the latest forecast is stale, partial, or unavailable
- **THEN** the interface shows the relevant state and reason without plotting missing values as zero or hiding existing actual-generation data

#### Scenario: User accesses the data without a chart

- **WHEN** a keyboard or assistive-technology user selects the tabular alternative
- **THEN** the interface exposes the same intervals, values, units, uncertainty, freshness, coverage, and provenance in a logical reading and focus order

### Requirement: Forecast configuration guidance

The web application SHALL let authorized users review forecast-input completeness and manage bounded loss/calibration settings while preserving catalog-prefilled equipment values as editable confirmed snapshots. It SHALL explain which inputs affect nameplate capacity, forecast generation, expected generation, generation performance, and inverter efficiency.

#### Scenario: Configuration is incomplete

- **WHEN** a string lacks required location, orientation, tilt, module, or model input
- **THEN** the interface identifies the exact missing fields, links to the relevant configuration, and leaves ordinary telemetry functions available

#### Scenario: User changes a loss assumption

- **WHEN** an authorized user saves a valid effective-dated loss or calibration setting
- **THEN** the interface confirms the effective boundary, invalidates affected modeled results, and shows recalculation progress without changing actual telemetry

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

