## Context

PVLog already has `ApiScope`, bearer request authentication, an `api_credentials` management table, and a one-time token application service. A second, system-bound ingestion-key implementation duplicates credential storage, lifecycle UI, authentication, quotas, and transport rules. Account API keys become the single non-interactive credential model.

## Goals / Non-Goals

**Goals:**

- Let an authenticated owner manage several independently named and revocable API keys in the user's implicit account context.
- Make least-privilege choices understandable: read systems, change systems, read PV data, or upload PV data.
- Return cleartext only from creation, retain only a keyed digest, and never expose it from list or revoke operations.
- Reuse the existing bearer principal and authorization checks so scope enforcement remains at each protected operation.
- Keep SQLite and PostgreSQL behavior equivalent and audit credential lifecycle actions.
- Remove system ingestion keys, credential-bearing URLs, and the custom upload-key header from every supported surface.
- Authorize telemetry uploads through the same account ownership boundary as other scoped bearer operations.
- Provide a single account settings destination for personal data, local password changes, and API-key management.

**Non-Goals:**

- Allowing one key to span multiple unrelated accounts.
- Letting an API key create, list, or revoke other API keys.
- Adding arbitrary custom scope strings or implicit scope inheritance.
- Changing the login email address; that requires a separate verified identity-change workflow.

## Decisions

### Current-account resource path

The lifecycle API uses `/api/v1/account/api-keys` rather than accepting an `accountId`. The authenticated user is the ownership context, matching the existing user-account identity model and preventing cross-account identifiers from entering request bodies or routes. Only cookie-authenticated users with account-management authority may call these endpoints; bearer keys are rejected to prevent credential self-propagation.

### Explicit scope vocabulary

The public API exposes `systems:read`, `systems:write`, `telemetry:read`, and `telemetry:write`, mapped to the existing internal `ApiScope` variants. No scope implies another scope: a client that uploads and later reads telemetry must receive both. `systems:write` governs system and equipment mutation and is never selected automatically.

### Account-wide keys with operation authorization

New self-service keys have `system_id = null`, meaning they may target any system owned by their account, subject to their explicit scopes. The normal system authorizer verifies account ownership and permissions after scope checking, including on telemetry ingestion. Internal support for optional system restrictions may remain on the general credential model, but no separate system-ingestion credential type is exposed.

### One credential transport

All API-key clients use `Authorization: Bearer <api-key>`. The canonical single and batch observation routes remain stable. The custom `x-pvlog-api-key` header and `/api/v1/push/{system_id}/{ingestion_key}/...` routes are removed so authentication, redaction, OpenAPI security declarations, and client examples have one consistent model. Query credentials remain invalid.

### One-time secret and safe metadata

Creation returns `{ apiKey, credential }`, where `apiKey` exists only in that response and `credential` is safe metadata. Listing returns metadata only. The token retains an opaque UUIDv7 identifier prefix for efficient lookup and is stored as a keyed digest. Revocation is idempotent only for an existing credential in the current account; unknown or foreign identifiers use the same not-found response.

### Persistence and service reuse

The application token service is extended with account-scoped list and revoke operations and backed by the management repository. Scope values are stored using the existing stable snake-case database representation. No new table is required; migrations are limited to indexes or constraints only if current schema inspection shows a gap.

Previously published system-ingestion-key migrations remain byte-for-byte immutable in the migration catalog even though the feature is removed. A new forward migration drops their obsolete tables and supporting indexes. This preserves checksum history for existing installations while leaving freshly provisioned databases in the unified-key end state.

### Current-user profile resource

The authenticated user's profile is exposed at `/api/v1/account/profile` without an account or user identifier in the route. Cookie-authenticated users may read their own email and display name and may change their display name. Bearer credentials are rejected. Email is displayed read-only until a verified email-change workflow is designed, avoiding an unsafe direct mutation of the login identity.

Local password changes continue to use `/api/v1/auth/password`, require the current password, and revoke other sessions after success. This keeps password verification and credential handling out of the general profile resource.

### Account UI placement

Profile data, password changes, and API keys appear together at `/account`, not in instance administration. The former `/account/api-keys` URL redirects to the consolidated page. Account settings are available to every authenticated user; the API-key section is shown only when the current session has credential-management authority.

The API-key create form requires a name and at least one permission, explains every permission in task language, optionally accepts an expiry, and defaults to no permissions. After creation, a modal/alert presents the key once with a copy action and explicit loss warning. The list shows name, scopes, creation/expiry/revocation state and provides an individually confirmed revoke action.

The system configuration no longer contains an upload-key tab or system-bound key actions. Uploader guidance links users to account API keys and identifies `telemetry:write` as the minimum permission.

### Complete bilingual presentation

German and English locale catalogs have identical key structures, and every statically referenced UI key must exist in both. Navigation, reporting, administration metadata, lifecycle states, role kinds, connector protocols, and known audit vocabulary are localized instead of displaying backend enum or action identifiers directly. Unknown server-extensible audit values remain visible through an explicit raw fallback rather than rendering a missing translation key.

## Risks / Trade-offs

- [Users accidentally grant write access] -> No preselected scopes, explicit descriptions, and a distinct destructive-system-change label.
- [A secret leaks through UI or telemetry] -> Keep it only in mutation state, never cache it in query data, redact request/auth headers, and test that list responses omit it.
- [Account-wide scope is broader than a single integration needs] -> Preserve internal system restriction support and leave per-key system selection as a future additive feature.
- [Revocation races with an in-flight request] -> Authentication checks active state on every request; already authenticated in-flight work may complete.
- [Multiple database backends drift] -> Run the shared management repository contract against SQLite and PostgreSQL implementations where available.
- [Existing uploaders stop working] -> Treat removal as an explicit breaking migration, document the bearer replacement, and provide equivalent canonical endpoint examples.
- [Users expect to edit their email] -> Display the login email clearly as read-only and explain that verified email changes are not yet available.
- [Locale catalogs silently drift] -> Test exact DE/EN key parity and statically referenced key coverage; keep a focused assertion for critical navigation and reporting labels.

## Migration Plan

Deploy account API-key lifecycle support before removing the legacy routes from an existing installation. Before upgrading, create account API keys with `telemetry:write` and update uploaders to send them as bearer credentials to the canonical observation routes. The release then removes legacy key endpoints, header/path authentication, UI, schemas, and storage adapters. Previously issued system ingestion keys are not migrated because their cleartext secrets cannot be recovered; a forward migration drops their obsolete persistence after the immutable creation migration has been recognized. Rollback requires restoring the previous application version and its legacy storage schema.

## Open Questions

None. Per-system restrictions and key rotation can be added later to the unified account API-key model without restoring a second credential type.
