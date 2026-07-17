## Context

PVLog already supports modern bearer credentials and browser sessions, but small PV devices and home-automation scripts benefit from a single system-bound secret that can be pasted into a header or complete push URL. This crosses credential persistence, HTTP authentication, telemetry authorization, rate limiting, audit/log redaction, administration UI, and deployment proxy guidance. URL credentials are intentionally supported for compatibility, despite their greater disclosure risk, so the design must keep them out of normal observability data.

## Goals / Non-Goals

**Goals:**

- Make single and batch ingestion possible with one system-specific API key and no OAuth/OIDC or cookie flow.
- Offer both a conventional header and a complete copyable push URL with identical ingestion semantics.
- Preserve system tenancy, least privilege, revocation, rotation, quotas, idempotency, and safe diagnostics.
- Keep existing bearer ingestion clients compatible.

**Non-Goals:**

- Using ingestion keys for reads, corrections, deletion, administration, or access to another system.
- Accepting credentials in query strings or JSON bodies.
- Replacing interactive authentication or general scoped API tokens.
- Providing anonymous ingestion or manufacturer-specific authentication protocols.

## Decisions

### Add a distinct system ingestion credential

Persist a credential ID, system/account binding, display name, keyed hash, hash-key version, lifecycle status, creation/rotation/revocation timestamps, and safe usage metadata in the management database. Generate a high-entropy random cleartext value containing a non-secret lookup prefix so verification performs one indexed lookup plus constant-time hash comparison rather than scanning hashes.

Reusing general bearer tokens was rejected because it would preserve more scope/configuration complexity for uploaders and make a copyable system push URL harder to manage safely. The new credential remains a normalized principal internally so authorization, quotas, and auditing can reuse existing boundaries.

### Support exactly one credential transport per request

The canonical routes accept `x-pvlog-api-key`; generated aliases place the opaque key in a path segment. A request containing both, any credential query parameter, or an ingestion key plus another Authorization credential is rejected as ambiguous. The two transports converge immediately into the same authenticated system-key principal and the existing ingestion use cases.

The URL form is a path segment rather than a query parameter because query strings are more routinely retained by analytics and proxy tooling. URL transport is still higher risk than a header, so the UI recommends the header when the uploader supports custom headers.

### Redact before generic HTTP observability layers

Match push routes and replace the credential segment with `{redacted}` before constructing trace/log fields. Never attach raw request headers. Proxy examples must normalize or suppress push-route access logs, and responses must use a generic authentication problem. Audit and rate-limit keys use the credential UUID, never cleartext or hash. Responses on push routes set a restrictive referrer policy and no-store cache controls.

Logging the raw URI and filtering afterward was rejected because exporters or middleware could observe it before redaction.

### One-time display with explicit rotation

Creation returns the cleartext header value and URLs once. Later reads return metadata only. Any active user of the owning account may manage keys for its systems, including renaming metadata and regenerating a replacement. Rotation creates a replacement and supports a short user-selected overlap window before revoking the prior key, allowing devices to update without downtime. Immediate revocation remains available.

Recoverable encryption was rejected because the server never needs to reproduce the key: the account user can regenerate it when the value is lost.

### Preserve canonical ingestion contracts

Header and URL routes use the existing observation and batch DTOs, idempotency keys, validation, admission control, persistence, and response types. The authenticated key supplies the authoritative system binding; a mismatching path system is rejected before account storage is opened.

## Risks / Trade-offs

- **[URL credentials leak through infrastructure logs]** → Redact before tracing, ship proxy-safe logging examples, set no-referrer/no-store headers, test logs and telemetry, and recommend header transport.
- **[A copied push URL grants write access]** → Scope to one system and ingestion only, display once, make rotation/revocation easy, expose last-used metadata, and audit lifecycle events.
- **[Credential ambiguity weakens fail-closed behavior]** → Accept exactly one transport and reject mixed credentials before verification.
- **[High-volume invalid keys enable lookup abuse]** → Use bounded prefix parsing, indexed lookup, constant-time verification, per-source admission limits, and generic failures.
- **[Rotation interrupts constrained devices]** → Support an explicit bounded overlap window and show both key states without redisplaying secrets.

## Migration Plan

1. Add nullable/additive credential storage and management APIs without changing existing authentication.
2. Add header authentication on canonical ingestion routes and verify authorization parity.
3. Add redacted push-route aliases, observability tests, and proxy guidance.
4. Expose self-service issue/rename/regenerate/revoke UI and one-time copy instructions to account users.
5. Existing bearer uploaders continue unchanged; operators opt into system ingestion keys.
6. Rollback disables the new routes and issuance while leaving credential metadata available for later cleanup; existing bearer ingestion remains operational.

## Open Questions

- What default and maximum rotation overlap window should deployments permit?
- Should operators be able to disable URL-key transport instance-wide while retaining header-key transport?
- Which reverse proxies need tested redaction snippets in the first release beyond the supplied Compose setup?
