# Security policy and review

## Supported releases and reporting

The latest stable minor release receives security fixes. Report suspected
vulnerabilities privately to the instance operator or project security contact;
do not include tokens, cookies, connector secrets, personal telemetry, or live
callback URLs in a public issue. Coordinated fixes may shorten normal API
deprecation windows.

## Release security review

The stable-release gate verifies:

- Argon2id password hashing/rehash, bounded recovery tokens, brute-force
  lockout, and enumeration-resistant lifecycle responses;
- OIDC issuer/audience/signature/time, state, nonce, and PKCE validation plus
  OAuth2 state/PKCE and normalized user-info mapping;
- recent-reauth identity linking, subject uniqueness, takeover resistance, and
  preservation of a last login method;
- deny-by-default hierarchical RBAC, pre-routing account authorization, scoped
  keyed-hash API credentials, quotas, and privilege-escalation matrices;
- secure session cookies, CSRF, rotation/revocation, idle/absolute expiration,
  and concurrent-session policy;
- connector/session secret redaction, encrypted provider state, request-header
  sensitivity, and absence of vendor-specific secret DTOs;
- SSRF-resistant HTTPS webhook delivery with DNS re-resolution, blocked local
  addresses, bounded redirects/body/time, signatures, replay limits, and
  dead-letter controls;
- restrictive CORS, CSP, content-type negotiation, body/concurrency/time limits,
  safe production configuration defaults, and privacy-safe search/projections;
- Rust advisory/license/source policy, pinned frontend dependencies, container
  vulnerability scanning, an unprivileged runtime user, and SBOM/checksums.

The automated evidence is provided by the `oidc-protocol`, `oauth2-protocol`,
`identity-linking`, `authorization-boundary`, `browser-session`, `api-token`,
`request-authorization`, `webhook-security`, repository security tests, OpenAPI
security declarations, `cargo audit`, `cargo deny`, and container CI jobs.
