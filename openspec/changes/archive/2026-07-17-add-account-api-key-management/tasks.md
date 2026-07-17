## 1. Credential lifecycle backend

- [x] 1.1 Extend the API-token repository/service with account-scoped metadata listing and owner-authorized revocation, backed by SQLite and PostgreSQL management storage.
- [x] 1.2 Add cookie-session-only current-account API-key create, list, and revoke handlers with explicit scope mapping, safe problem responses, and lifecycle audit records.

## 2. API contract and authorization

- [x] 2.1 Document API-key lifecycle resources, one-time secret responses, metadata, scopes, and errors in OpenAPI and update route/schema contract coverage.
- [x] 2.2 Verify protected system and telemetry operations enforce their documented read/write bearer scopes without granting implicit permissions.

## 3. Account user interface

- [x] 3.1 Add typed/Zod-validated account API-key client hooks and an accessible account/security management page for create, one-time copy, list, and confirmed revoke workflows.
- [x] 3.2 Add localized permission descriptions, validation, success/error feedback, responsive presentation, and tests that ensure secrets are not redisplayed or cached.

## 4. Verification

- [x] 4.1 Run backend checks and focused domain, storage, API, authorization, frontend, OpenAPI, and browser tests with zero Rust warnings and no new lint errors.

## 5. Replace legacy upload keys

- [x] 5.1 Remove the system-ingestion-key domain, application, persistence, runtime composition, lifecycle routes, and custom authentication principal.
- [x] 5.2 Require account API-key bearer authentication with `telemetry:write` and account/system authorization on canonical single and batch ingestion routes.
- [x] 5.3 Remove legacy ingestion-key paths, push URLs, custom security schemes, schemas, and examples from OpenAPI and contract coverage.
- [x] 5.4 Remove the system upload-key UI and replace uploader guidance with the account API-key workflow.
- [x] 5.5 Remove obsolete migrations, scripts, tests, and dependencies while preserving equivalent unified-key coverage.

## 6. Replacement verification

- [x] 6.1 Run focused and full backend, frontend, OpenAPI, documentation, and browser checks with zero Rust warnings and no new lint errors.

## 7. Consolidated account settings

- [x] 7.1 Add cookie-session-only current-user profile read and display-name update use cases, persistence, handlers, and focused backend tests.
- [x] 7.2 Document the current-user profile resource and schemas in OpenAPI and extend route/schema contract coverage.
- [x] 7.3 Replace the API-key-only page with one localized and accessible account page for profile, password, and permission-aware API-key management, preserving the old URL as a redirect.
- [x] 7.4 Add focused frontend tests and run backend, frontend, OpenAPI, and browser validation with zero Rust warnings and no new lint errors.

## 8. Migration compatibility

- [x] 8.1 Restore the published system-ingestion-key migrations byte-for-byte and add forward-only removal migrations for SQLite and PostgreSQL.
- [x] 8.2 Add upgrade-path regression coverage and verify the existing local SQLite database can migrate and start without schema incompatibility.

## 9. Complete bilingual UI

- [x] 9.1 Audit and correct German and English navigation, reporting, administration, lifecycle, role, connector, and audit labels.
- [x] 9.2 Localize known backend metadata at render boundaries while retaining readable fallbacks for extensible values.
- [x] 9.3 Add locale parity and referenced-key regression tests and run frontend type, lint, build, and UI checks.
