## 1. Credential Domain and Persistence

- [x] 1.1 Define system ingestion key domain models, lifecycle states, safe metadata, generation format, lookup prefix, keyed hashing, constant-time verification, and normalized ingestion principal.
- [x] 1.2 Add additive SQLite and PostgreSQL management migrations for system-bound ingestion keys, hash-key versioning, rotation lineage/overlap, revocation, and safe last-used metadata.
- [x] 1.3 Implement backend-neutral repositories and application services for issue-once, rename, list-metadata, regenerate/rotate, revoke, verify, and auditable lifecycle operations with active account-user/system authorization.
- [x] 1.4 Add cross-engine migration, repository, cryptographic verification, collision, expiry/overlap, revocation, and tenant-isolation tests.

## 2. Authentication and Ingestion Routes

- [x] 2.1 Extend request authentication to accept `x-pvlog-api-key` on canonical single/batch ingestion routes and reject malformed, query-string, mixed, or ambiguous credential transports.
- [x] 2.2 Add generated `/api/v1/push/{system_id}/{ingestion_key}/observations` single/batch aliases that authenticate before account storage routing and converge on the existing ingestion use cases.
- [x] 2.3 Enforce one-system telemetry-write-only authorization plus per-key quotas, last-used updates, audit identifiers, idempotency, validation, duplicate, batch, and backpressure parity across header, URL, and existing bearer transports.
- [x] 2.4 Redact URL/header key material before tracing and access logging, emit normalized routes and safe key IDs only, and add no-store/referrer protections plus generic authentication problems.
- [x] 2.5 Add authentication, authorization, rotation/revocation, invalid-key, cross-system, ambiguity, redaction, rate-limit, single/batch, and idempotent-retry API tests.

## 3. Management API and OpenAPI

- [x] 3.1 Add active-account-user system ingestion-key create/rename/list/regenerate/revoke endpoints that return cleartext header/URL credentials only on issue or regeneration and safe metadata thereafter.
- [x] 3.2 Extend OpenAPI with header and push-URL security schemes, lifecycle endpoints, single/batch uploader examples, generic failures, transport precedence, rotation, and secret-handling guidance.
- [x] 3.3 Add bidirectional route coverage, schema/contract tests, and tests proving existing bearer ingestion remains compatible.

## 4. Administration and Uploader Experience

- [x] 4.1 Add typed Zod-validated self-service clients and TanStack Query mutations for issuing, renaming, listing, regenerating, and revoking system ingestion keys without caching cleartext secrets.
- [x] 4.2 Implement localized accessible account-user system-key management UI with one-time secret display, renaming, copyable header and complete push URLs, explicit regeneration overlap, revocation confirmation, status, and last-used metadata.
- [x] 4.3 Add English/German component and Playwright tests for creation, copy flows, lost-secret rotation, overlap/revocation, URL-disabled policy, permission denial, and secret disappearance after dismissal/reload.

## 5. Operations, Documentation, and Release Validation

- [x] 5.1 Document header-first uploader setup, URL fallback, curl/shell/home-automation examples, secret storage, rotation/revocation, HTTPS requirements, and incident response for a leaked push URL.
- [x] 5.2 Add Compose/reverse-proxy access-log redaction guidance and automated tests proving keys never appear in logs, traces, metrics, audit records, problems, referrers, or release evidence.
- [x] 5.3 Run warning-free Rust checks/tests, SQLite/PostgreSQL profiles, frontend lint/typecheck/tests/build, Playwright, OpenAPI lint/compare/coverage, security tests, and production embedded-UI validation.
