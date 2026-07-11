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

