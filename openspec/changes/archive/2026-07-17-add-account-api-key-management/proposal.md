## Why

PVLog already authenticates scoped bearer credentials internally, but users cannot manage those credentials from their account or deliberately restrict each key to the minimum required operations. Account owners need independently revocable keys for integrations that may upload or read PV data without receiving permission to change system configuration.

## What Changes

- Add account-level management for multiple named API keys with independent scope sets and optional expiry.
- Offer explicit least-privilege scopes for PV telemetry upload, PV telemetry read, system read, and system management.
- Display a new key's cleartext value exactly once, store only a non-reversible hash, and support listing and revocation without exposing secrets.
- Enforce each key's selected scopes in the existing API authorization boundary and keep its access limited to systems owned by its account.
- Add one authenticated account page for reviewing and changing personal data, changing the local password, and creating, reviewing, copying once, and revoking API keys.
- Document the lifecycle, bearer authentication, scope behavior, problem responses, and examples in OpenAPI.
- Remove the legacy system-bound upload-key lifecycle, `x-pvlog-api-key` transport, credential-bearing push URLs, and their system-level UI.
- Require upload integrations to use an account API key with `telemetry:write` through the standard bearer authorization header.

**BREAKING:** Existing system ingestion keys and generated push URLs stop authenticating. Uploaders must be migrated to an account API key and `Authorization: Bearer <api-key>`.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `identity-and-access`: Make scoped modern API tokens self-service account credentials with multiple independently managed keys and explicit least-privilege scope selection.
- `modern-rest-api`: Define the account API-key management resources and enforce operation-specific bearer scopes.
- `telemetry-ingestion`: Replace special system-key transports with scoped bearer API keys for canonical telemetry ingestion.
- `web-application`: Expose profile, password, and API-key lifecycle management in one authenticated account UI.

## Impact

This affects the identity domain, credential persistence and hashing, request authentication and authorization, current-user profile, account-management and telemetry API handlers, OpenAPI schemas, uploader documentation, and the authenticated account UI. The legacy system-ingestion-key model and all credential-specific transports are removed.
