# Agent: Compose `.env`

- Local compose config file: `deploy/compose/.env`.
- Supported keys and placeholder values: `deploy/compose/.env.example`.
- Do not read, print, or commit secrets from `.env`.
- If `.env` is missing and compose config is needed, create it from `.env.example`.
- Change only the values needed for the requested task.
- If adding/renaming compose env vars, update `.env.example` in the same change.
- Backend-facing auth vars must stay OIDC/provider-neutral; do not add backend `KEYCLOAK_*` settings.
- User-facing details: [../17-compose-env.md](../17-compose-env.md).
