# Agent: Frontend Code Quality

Use these rules for frontend code organization, imports, naming, typing, documentation, linting, and tests.

## Code Organization and Imports

- **Absolute Imports**: Prefer absolute imports over relative imports beyond one level. Use the `@/` alias for `src/`.
- **Extensionless Imports**: Keep local TypeScript/JavaScript imports and re-exports extensionless, such as `./Foo` and `@/Foo`. Use file extensions only when a runtime or tooling constraint requires them.
- **Directory Structure**: Follow the exact structure outlined in the style guide.
- **One Component Per File**: File name matches component name in PascalCase.
- **Index Barrels**: Use `index.ts` for public exports.
- **Shared Components Layout**: Every reusable shared component must live in its own folder under `src/shared/components/<ComponentName>/`.
- **Shared Component Public API**: Each component folder exposes a single `index.ts`; consumers import from `@/shared/components` or the component folder public API, never from nested implementation files.
- **Shared Component File Placement**: The primary component implementation lives directly inside the component folder, and related local helpers stay in the same folder only when they are strictly internal.
- **No Nested Subfolders**: Do not create additional subfolders inside a component folder. If a component needs more structure, split the responsibility into separate top-level shared modules instead.
- **No Legacy UI Barrel**: `src/shared/components/ui/` does not exist; do not recreate a flat `ui` aggregation folder or import from it.
- **Root Shared Barrel**: `src/shared/components/index.ts` is the canonical root barrel for shared reusable components and should re-export the component folder public APIs.

## Naming Conventions

- **Components**: PascalCase, such as `UserProfileCard.tsx`.
- **Hooks**: camelCase with `use` prefix, such as `useUserProfile.ts`.
- **Utilities**: camelCase, such as `formatCurrency.ts`.
- **Constants**: SCREAMING_SNAKE_CASE, such as `MAX_RETRY_COUNT`.
- **Types/Interfaces**: PascalCase, such as `UserProfile` or `ApiResponse<T>`.
- **API Files**: camelCase with `Api` suffix, such as `userApi.ts`.
- **Test Files**: Same name with `.test.tsx` suffix, such as `Button.test.tsx`.

## Type Safety

- **Strict Mode**: Enforce TypeScript strict mode.
- **Avoid `any`**: Use `unknown` for untrusted data and narrow with type guards.
- **Type Imports**: Use `import type` for type-only imports.
- **Validation**: Use Zod schemas at all external data boundaries, including API responses, form inputs, and events.

## Testing Strategy

- **Testing Trophy**: Prioritize Integration > Unit > E2E > Static.
- **Unit Tests**: Pure logic in isolation, such as utils, hooks, and store. Use Vitest. Co-locate tests with the module.
- **Component Tests**: UI + interaction with Vitest + React Testing Library. Co-locate tests with the component.
- **E2E Tests**: Browser flows with Playwright in `e2e/specs/`. Use Page Object Model. When validating runtime behaviour, reproducing UI bugs, or checking console/network output, use the configured browser MCP server first.
- **Coverage**: Thresholds are statements 80%, branches 75%, functions 80%, lines 80%. Exclude `src/main.tsx`, `src/shared/lib/**`, and `**/*.types.ts`.

## Linting and Code Quality

- **Stylelint**: Run via `pnpm lint` alongside ESLint. Config is in `.stylelintrc.json`. Key rules are documented in [frontend-design-system.md](frontend-design-system.md).
- **ESLint Rules**:
  - Enforce FSD boundaries with `boundaries/dependencies`.
  - `react/no-multi-comp`: Error.
  - `unicorn/filename-case`: PascalCase for components.
  - `max-lines`: Warn at 300 lines.
  - `@typescript-eslint/consistent-type-imports`: Error.
  - `max-lines-per-function`: Warn at 100.
  - `complexity`: Warn at 10.
  - `react/jsx-no-literals`: Warn; flags hardcoded string literals in JSX children to enforce i18n.
  - `jsx-a11y` strict preset: enforces accessibility rules at lint time. See [frontend-i18n-accessibility.md](frontend-i18n-accessibility.md).
- **Refactoring Triggers**:
  - File > 300 lines.
  - Function/component > 100 lines.
  - Complexity > 10.
  - UI + business logic coupled.
  - Reusable parts elsewhere.
- **Allowed Exceptions**: Inline subcomponents <= 20 lines, not exported, used once, and presentational.
- **Code Review Checklist**:
  - One component per file.
  - Single responsibility.
  - No business logic in presentational components.
  - Logic in hooks.
  - Respect FSD boundaries.
  - Public APIs only.
  - Shared reusable components must use the per-folder layout with a single `index.ts` public API and no nested subfolders.
  - Shared component consumers must import from `@/shared/components` or the component folder public API only.
  - Add every new reusable shared component to the component showcase so it can be reviewed and verified in context.

## JSDoc Conventions

JSDoc is the contract for every exported symbol. TypeScript types handle machine-level correctness; JSDoc communicates intent, behaviour, and constraints to the next developer.

### What requires a JSDoc block

| Symbol                                 | Required tags                                   |
| -------------------------------------- | ----------------------------------------------- |
| Exported function / hook               | `@param` per parameter, `@returns`              |
| Exported React component               | `@param props` if it has props, `@returns`      |
| Exported `interface` / `type`          | Description + `@property` per non-obvious field |
| Non-obvious exported constant          | One-line description                            |
| Internal helper with non-trivial logic | One-line description minimum                    |

Private, purely mechanical helpers, such as a one-liner that just calls another function, do not need JSDoc beyond a brief inline comment if the name is already clear.

**Exceptions**: `src/shared/components/ui/` files are mechanical shadcn/Radix wrappers; JSDoc is not required there.

### ESLint enforcement

`eslint-plugin-jsdoc` is configured in `eslint.config.js` with the following rules active as warnings on all exported symbols:

- `jsdoc/require-jsdoc` — requires a JSDoc block on every exported function, type alias, interface, and variable declaration.
- `jsdoc/require-param` + `jsdoc/require-param-description` — every `@param` must exist and have a description.
- `jsdoc/require-returns` + `jsdoc/require-returns-description` — `@returns` must be present, except getters, and described.
