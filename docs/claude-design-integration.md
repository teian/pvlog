# Claude Design Integration Map

## 1. Scope

- Target project: `/home/teian/code/pvlog`
- Primary design export: `design_handoff_system_management/` (system management); the earlier `/home/teian/Downloads/pvlog-design` handoff remains the source for the surrounding shell and reporting pages
- Frameworks: proprietary Design Component HTML reference to React 19, React Router, shadcn/ui, Tailwind CSS 4, and Recharts
- Package managers: no export package manager; target uses pnpm 11
- Integration objective: apply the PV monitoring dashboard's visual language to the production UI while retaining PVLog's routes, APIs, authorization, state handling, accessibility, tests, and deployment model

## 2. Architecture summary

### Design export

The export is an interactive, single-file prototype with custom template elements, inline SVG charts, deterministic mock data, and client-side state for scope, date range, report type, sidebar expansion, and chart hover. It defines dashboard, statistics, seasonal, weather, systems, management, and administration views. Styling is supplied as a separate token sheet using Noto Sans, slate surfaces, a dark blue sidebar, blue interactive emphasis, orange energy accents, compact typography, eight-pixel card radii, and restrained shadows. Supplied raster screenshots are visual references; the logo can be represented by existing Lucide icons, so no image asset is required.

### Target project

PVLog is a Vite SPA using React Router, Feature-Sliced Design, TanStack Query, native fetch clients with Zod validation, shadcn/Radix primitives, Tailwind CSS 4 semantic variables in `src/ui/index.css`, Recharts through the shared Chart primitive, i18next English/German translations, and route-level authentication and permission checks. `AppShell` owns responsive navigation and session controls. Production APIs, not mock state, drive dashboard, historical charts, forecast, system configuration, data quality, and administration views.

## 3. Page-to-route mapping

| Export page                | Target route                  | Target layout                             | Status     | Notes                                                                                                        |
| -------------------------- | ----------------------------- | ----------------------------------------- | ---------- | ------------------------------------------------------------------------------------------------------------ |
| Dashboard / all systems    | `/`                           | `AppShell` + `DashboardPage`              | Restyle    | Preserve live/stale semantics and the existing aggregate dashboard API.                                      |
| Dashboard / system ranges  | `/systems/:systemId`          | `SystemLayoutPage` + `SystemChartsPage`   | Restyle    | Existing range, resolution, category, comparison, table, and export controls remain authoritative.           |
| Statistics                 | `/statistics`                 | `StatisticsPage`                          | Integrated | Uses lifetime summaries and monthly telemetry rollups from the authorized reporting API.                     |
| Forecast                   | `/systems/:systemId/forecast` | `SystemLayoutPage` + `SystemForecastPage` | Restyle    | Uses real forecast, expected-generation, realization, completeness, and settings APIs.                       |
| Systems / management       | `/onboarding`                 | `AppShell` + `SystemManagementView`       | Integrated | Expandable real system trees and a single-page create/edit wizard replace the former first-run-only flow.    |
| Administration             | `/administration?section=…`   | Dedicated administration sidebar          | Integrated | Six focused sections preserve identities, RBAC, connectors, alerts, webhooks, resources, and audit behavior. |
| Authentication             | `/login`                      | Split brand panel + `LoginPage`           | Restyle    | Preserve local credentials, recovery, and configured external connector flows.                               |
| Systems                    | `/systems`                    | `SystemsPage`                             | Integrated | Loads authorized system metadata and installed inverter/string capacity totals.                              |
| Seasonal                   | `/seasonal`                   | `SeasonalPage`                            | Integrated | Aggregates persisted daily summaries into meteorological seasons.                                            |
| Weather                    | `/weather`                    | `WeatherPage`                             | Integrated | Shows the latest persisted provider forecast with matching system-level predicted energy.                    |
| Per-string dashboard scope | No production route           | None                                      | Deferred   | Existing telemetry queries are system-scoped; no fake string readings will be introduced.                    |

## 4. Component mapping

| Export component          | Existing target component                     | Action        | Notes                                                                                        |
| ------------------------- | --------------------------------------------- | ------------- | -------------------------------------------------------------------------------------------- |
| Fixed navy navigation     | `AppShell`                                    | Restyle       | Preserve mobile drawer, skip link, session controls, permissions, and real system IDs.       |
| Logo lockup               | `AppShell` + Lucide icons                     | Compose       | Use installed icons and translated tagline; do not migrate duplicate raster logos.           |
| KPI tiles                 | `Card` composition in `DashboardPage`         | Restyle       | Keep real values, unavailable states, and stale-data suppression.                            |
| Fault callout             | `Alert`                                       | Reuse         | Existing stale/error/partial states retain semantic alerts.                                  |
| Range and report controls | `ToggleGroup`, `NavLink` tabs, chart controls | Restyle       | Preserve query bounds, accessible pressed states, and route semantics.                       |
| Data charts               | `ChartContainer`, Recharts charts             | Reuse/restyle | No inline SVG chart math is copied.                                                          |
| Data tables               | `Table` and existing chart table views        | Reuse         | Existing localized accessible alternatives remain available.                                 |
| Export control            | Existing analysis/forecast export mutations   | Reuse         | Unlike the prototype, production exports stay functional.                                    |
| Status labels             | `Badge`                                       | Reuse         | Preserve semantic status and translated accessible text.                                     |
| Administration switches   | `Switch`                                      | Add/reuse     | Radix-backed controls update real alert rules with keyboard and focus support.               |
| Administration sidebar    | `AdministrationSidebar`                       | Compose       | Query-backed sections mirror the mocks while retaining a mobile drawer and exit route.       |
| Expandable system cards   | `SystemManagementCard`                        | Add/compose   | Uses `Card`, `Badge`, `Button`, and `AlertDialog` with real lifecycle and equipment data.    |
| Create/edit wizard        | `SystemWizard`                                | Add/compose   | Uses `FieldGroup`, catalog fields, toggles, nested editors, validation, and a success state. |
| Inverter editor           | `InverterDraftEditor`, `InverterCatalogField` | Add/compose   | Catalog snapshots remain editable and persist through the aggregate API.                     |
| PV-string editor          | `StringDraftEditor`, `ModuleCatalogField`     | Add/compose   | Live kWp, orientation, tilt, and module values use the existing equipment catalog.           |

## 5. Design-token mapping

| Category               | Export value                     | Target token                                 | Action                                                                                                        |
| ---------------------- | -------------------------------- | -------------------------------------------- | ------------------------------------------------------------------------------------------------------------- |
| App background         | Slate 50                         | `--background`                               | Shift light theme to the cool slate surface.                                                                  |
| Card surface           | White                            | `--card`, `--popover`                        | Use a clean raised surface with restrained shadow.                                                            |
| Interactive blue       | Blue 600                         | `--primary`, `--ring`, `--sidebar-primary`   | Use for actions, links, selection, and chart emphasis.                                                        |
| Energy orange          | Orange 600                       | `--brand`, `--brand-foreground`, `--chart-2` | Keep the reference orange for graphics and use its accessible darker foreground companion for compact values. |
| Status green/amber/red | Export status scale              | `--success`, `--warning`, `--destructive`    | Add semantic success/warning registrations and retain destructive.                                            |
| Sidebar navy           | Blue/slate 900                   | `--sidebar`                                  | Apply to the persistent navigation surface.                                                                   |
| Selected system scope  | Slate 800                        | `--sidebar-selected`                         | Distinguish scope selection from the blue active-view treatment.                                              |
| Typography             | Noto Sans / mono data            | Existing bundled Noto Sans and `font-mono`   | Reuse locally bundled fonts; no export font files copied.                                                     |
| Radius                 | 8 px cards, 6 px controls        | `--radius` and existing component sizes      | Tighten the global radius to match.                                                                           |
| Content width          | 1280 px, 32 px padding           | `max-w-screen-xl`, responsive `px-4`/`px-8`  | Reuse Tailwind layout primitives.                                                                             |
| Motion                 | Color transitions and live pulse | Existing transitions and motion-safe pulse   | Respect reduced-motion preferences.                                                                           |

## 6. Asset migration

| Asset                                | Destination               | License checked           | Optimization      | Notes                                                                                       |
| ------------------------------------ | ------------------------- | ------------------------- | ----------------- | ------------------------------------------------------------------------------------------- |
| Supplied PVLog logo PNG/SVG variants | Not migrated              | User-supplied export      | Deduplicated      | Existing Lucide icons reproduce the simple sun/panel mark while inheriting semantic colors. |
| Screenshot references                | Documentation-only source | User-supplied export      | Not shipped       | Used for comparison, never included in production bundles.                                  |
| Noto font declarations               | Existing package assets   | Existing package licenses | Already optimized | Target already bundles variable Noto Sans and Noto Serif.                                   |

## 7. Data and interaction mapping

| Export mock behavior    | Target data source or action                                   | Required adaptation                                                                                            |
| ----------------------- | -------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------- |
| Seeded dashboard KPIs   | `/api/v1/dashboard` through `useDashboard`                     | Present real freshness, coverage, ingestion, and alert states in the new card treatment.                       |
| Seeded per-range charts | Existing bounded analytics series hooks                        | Keep real range, resolution, category, comparison, table, and export workflows.                                |
| Estimated generation    | Forecasting hooks and APIs                                     | Preserve uncertainty, provenance, completeness, and actual-versus-modeled distinctions.                        |
| Scope navigation        | Session system IDs and React Router                            | Keep real system routes; do not synthesize names or string scopes.                                             |
| Export CSV placeholder  | Existing export mutations                                      | Retain real CSV/JSON exports and queued/error states.                                                          |
| Local management edits  | Existing onboarding and administration mutations               | Present users, account-role assignment, invitation, and protected deletion in one compact table card.          |
| Alert rule toggles      | Account alert GET/PATCH endpoints                              | Render server rules and persist enabled state; do not seed mock thresholds.                                    |
| Notification channels   | Account webhook GET endpoint                                   | Show configured endpoints, event counts, and verification state.                                               |
| SMTP configuration      | Instance administration settings API                           | Persist non-secret SMTP metadata and external secret references; test network reachability.                    |
| Retention and backups   | Instance administration settings and backup APIs               | Persist policy metadata and create a verified operator bundle for SQLite deployments.                          |
| Weather feed            | Instance administration settings API                           | Persist a provider-neutral endpoint and external secret reference; test reachability.                          |
| System card hierarchy   | `GET /api/v1/systems/{id}` plus nested inverter GET            | Load lifecycle metadata and effective inverter/string aggregates rather than copying seeded prototype data.    |
| Create/edit system      | System POST/PUT, archive/restore, and nested inverter POST/PUT | Save system metadata first, then complete inverter aggregates; preserve ETag concurrency for lifecycle writes. |
| Delete system           | Confirmed system DELETE with ETag                              | Keep the destructive confirmation dialog and disable deletion of the final visible system.                     |
| Catalog search          | Bundled inverter and solar-module catalog APIs                 | Use searchable datalist fields with a manual fallback and copied specification snapshots.                      |

## 8. Accessibility review

- Semantic structure: retain the skip link, navigation and main landmarks, one page-level heading, card headings, tables, and alerts.
- Keyboard navigation: preserve links/buttons, visible focus rings, pressed-state controls, and table alternatives.
- Focus states: map the export blue to the semantic ring token in both themes.
- Contrast: use semantic light/dark tokens rather than copying light-only inline colors; verify with axe and representative screenshots.
- Forms and validation: keep existing labels, descriptions, `aria-invalid`, and server validation feedback.
- Motion preferences: live indicators may pulse only through motion-safe utilities; charts continue using the reduced-motion hook.

## 9. Risks and deliberate deviations

| Risk or deviation                                  | Reason                                                                                                                                                                                                              | Mitigation                                                                                                                                              |
| -------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------- |
| No seasonal or weather-composition page            | No authoritative production endpoint exists.                                                                                                                                                                        | Document as follow-up API work; do not ship seeded mock values.                                                                                         |
| No all-system string tree                          | Session data exposes system IDs, not names/topology/status.                                                                                                                                                         | Keep accessible real system links and preserve room for richer navigation data later.                                                                   |
| Desktop utility header removed                     | The reference assigns navigation, administration, identity, and sign-out to the fixed sidebar.                                                                                                                      | Retain a mobile-only menu header so off-canvas navigation remains reachable.                                                                            |
| Report navigation with system-scoped data          | Reporting data belongs to an authorized system and may be absent before ingestion or aggregation.                                                                                                                   | Link every design navigation entry to a production route and render explicit empty states without mock values.                                          |
| Existing chart categories remain separate          | The analytics API and user-selectable categories are broader than the mock's single composed chart.                                                                                                                 | Use the export's card, typography, control, and color treatment around the real charts.                                                                 |
| Dark mode differs from screenshots                 | Export supplies only a light content theme.                                                                                                                                                                         | Derive accessible dark semantic counterparts and preserve theme switching.                                                                              |
| No claim of pixel parity                           | Production states and data differ from seeded screenshots.                                                                                                                                                          | Verify layout at 375, 768, and 1440 px and report remaining differences.                                                                                |
| Role selection does not imply exclusive membership | PVLog RBAC supports multiple account and system assignments while the mock shows one role cell.                                                                                                                     | Add a real account role from the row selector; never synthesize or silently revoke other assignments.                                                   |
| PostgreSQL browser backups are unsupported         | PostgreSQL needs an operator-controlled dump destination and credentials outside the browser process.                                                                                                               | Return an explicit 501 response; SQLite creates and verifies an operator bundle locally.                                                                |
| SMTP and weather credentials stay external         | Persisting plaintext provider passwords in browser-managed configuration would expose secrets.                                                                                                                      | Store only secret references and never return resolved credential material.                                                                             |
| Resolved location is preview-only                  | Search-as-you-type now uses the authenticated, debounced, cached, operator-configurable Photon adapter over OpenStreetMap data, but the system lifecycle model still has no persisted address or coordinate fields. | Present an accessible suggestion list with provider coordinates and attribution; add domain/API fields before claiming that coordinates survive a save. |
| Shading and temperature coefficient are draft-only | The current PV-string aggregate persists orientation and tilt but has no shading-window or temperature-coefficient fields.                                                                                          | Preserve the designed advanced controls without inventing storage; extend the equipment domain in follow-up work.                                       |
| Removed inverters are not deleted on edit          | The existing aggregate API supports create and replacement but no inverter deletion operation.                                                                                                                      | Persist additions and edits safely; defer destructive removal until an explicit authorized API exists.                                                  |

## 10. Implementation order

1. Map semantic colors, radii, shadows, typography, and chart tokens in `src/ui/index.css`.
2. Restyle the shared Card primitive as the representative component and verify the build.
3. Apply the navy responsive shell, logo lockup, system navigation, and compact utility header.
4. Restyle the dashboard KPI, freshness, coverage, ingestion, and alert sections.
5. Apply the visual treatment to system navigation, historical charts, and forecasting routes.
6. Integrate the dedicated administration sidebar, section routing, real alert rules, webhook channels, and audit-list treatment.
7. Run responsive, keyboard, accessibility, and screenshot checks.
8. Run lint, typecheck, UI tests, Playwright, production build, and bundle validation.

## 11. Verification commands

```bash
pnpm lint
pnpm typecheck
pnpm test:ui
pnpm test:e2e
pnpm build
pnpm test:bundle-budget
```
