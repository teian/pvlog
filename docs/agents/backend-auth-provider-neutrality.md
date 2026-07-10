# Agent: Backend Auth Provider Neutrality

- Backend code/config/tests must use OIDC/OAuth/provider-neutral names.
- Do not add backend settings, DTO fields, health keys, services, modules, or tests named after a specific IdP product.
- Allowed backend names include `oidc`, `oidc_provider`, `issuer`, `jwks`, `authorization_endpoint`, and `end_session_endpoint`.
- Keep backend auth as a BFF: provider protocol client, server-side token/session store, local user provisioning, and a small token-to-cookie orchestration facade.
- Local compose/dev infra may use a concrete IdP service such as Dex, but that name must stay at the infrastructure boundary.
- Backend env vars must be provider-neutral, e.g. `OIDC_PROVIDER_PUBLIC_URL`, `OIDC_ISSUER_URL`, `OIDC_CLIENT_ID`, `OIDC_CLIENT_SECRET`.
- If a provider-specific workaround is unavoidable, isolate it in compose/dev docs or an adapter named by protocol behavior, not vendor name.
