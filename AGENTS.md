# Agent Notes


## Backend 

- After every Rust code change, run `cargo check` and fix **all warnings** before considering the task done. Zero warnings is the required state. Common warnings to watch: unused imports, unused variables, dead code, unreachable patterns.
- Sandboxed agent Rust build helper: see [docs/agents/agent-rust-builds.md](docs/agents/agent-rust-builds.md).
- Compose `.env` usage: see [docs/agents/compose-env.md](docs/agents/compose-env.md).
- Backend auth provider neutrality: see [docs/agents/backend-auth-provider-neutrality.md](docs/agents/backend-auth-provider-neutrality.md).

## Frontend

When generating, modifying, or reviewing code in `frontend/`, read the relevant frontend instruction files:

- Frontend architecture and stack: see [docs/agents/frontend-architecture.md](docs/agents/frontend-architecture.md).
- Frontend design system, typography, colours, and assets: see [docs/agents/frontend-design-system.md](docs/agents/frontend-design-system.md).
- Frontend aesthetics, layout patterns, component recipes, and visual conventions: see [docs/agents/frontend-aesthetics.md](docs/agents/frontend-aesthetics.md).
- Frontend data fetching, state, telemetry, and runtime configuration: see [docs/agents/frontend-data-telemetry.md](docs/agents/frontend-data-telemetry.md).
- Frontend code organization, quality, documentation, and testing: see [docs/agents/frontend-code-quality.md](docs/agents/frontend-code-quality.md).
- Frontend internationalization and accessibility: see [docs/agents/frontend-i18n-accessibility.md](docs/agents/frontend-i18n-accessibility.md).
