# Agent: Frontend Architecture

These rules apply when generating, modifying, or reviewing code in the faultmanagement frontend.

## Architecture and Design Patterns

- **Feature-Sliced Design (FSD)**: Adopt FSD as the architectural pattern. All source code resides under `src/`.
- **Layer Structure**: Organize code into layers: `app/`, `pages/`, `widgets/`, `features/`, `entities/`, `shared/`. Dependencies flow downward only. A layer may only import from layers below it. Peer modules on the same layer must not import directly from each other.
- **Public APIs**: Each feature exposes a single `index.ts` as its public API. Only exported items may be used by other layers. Features must not import directly from another feature's internals; promote cross-feature sharing to `entities/` or `shared/`.
- **Feature Module Structure**:
  ```
  src/features/<feature>/
  ├── api/                  # REST fetch clients
  ├── components/           # UI components for this feature
  ├── hooks/                # TanStack Query hooks (queries and mutations)
  ├── store/                # Zustand slice (UI state only — no data fetching)
  ├── types/
  ├── utils/
  └── index.ts              # Public API
  ```

## Frontend Stack

- **Framework**: React + Vite.
- **Routing**: React Router.
- **Data Fetching**: TanStack Query with regular REST endpoints using the native `fetch` API only. Do not introduce GraphQL clients or third-party HTTP clients. When working with external library APIs or docs for data-fetching and related client-side integrations, use the configured context7 MCP server first for authoritative, versioned guidance.
- **UI State**: Zustand per feature, for ephemeral UI state only.
- **Forms**: React Hook Form + Zod resolvers.
- **Component Library**: shadcn/ui, owned in-repo. When creating or updating shadcn primitives, use the configured shadcn MCP server first so the structure stays aligned with upstream shadcn patterns, accessibility, and public APIs, then adapt locally as needed.
- **Styling**: Tailwind CSS v4, CSS-first, with design tokens via CSS variables. Never use `@apply`; reference tokens directly with `var(--color-*)`, `var(--radius-*)`, etc.
- **Type Safety**: TypeScript strict mode.
- **Validation**: Zod at all external data boundaries.
- **Internationalization**: i18next + react-i18next + i18next-browser-languagedetector.
- **Observability**: OpenTelemetry browser tracing with centralized bootstrap in `src/shared/lib/telemetry`.
- **Build Tooling**: Vite.
- **Package Manager**: pnpm, managed via `packageManager` in `package.json`. Use pnpm commands for installs and scripts, and commit `pnpm-lock.yaml` instead of `package-lock.json`.

## Additional Rules

- **Authentication**: Handled externally by oauth2-proxy; no auth logic in frontend.
- **Performance**: Page load <2s, drag/drop <200ms.
- **No SSR**: Plain SPA.
- **Path Aliases**: Configure `@/` for `src/` in `tsconfig.json` and `vite.config.ts`.
