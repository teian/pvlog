# Agent: Frontend Data and Telemetry

Use these rules for client-side data fetching, UI state, runtime configuration, and browser tracing.

## Data Fetching and State Management

- **GraphQL Queries**: Execute client-side using `graphql-request` wrapped in TanStack Query.
- **Mutations**: Issue directly from components via TanStack Query mutation hooks. Extract complex or reusable business logic to service modules in `features/` or `entities/`.
- **Caching**: Explicitly define caching for all queries:
  - `staleTime: 0` for user-specific or dynamic data, such as session state.
  - `staleTime: <ms>` for semi-static data.
  - `staleTime: Infinity` for static or rarely changing data.
- **Zustand Restrictions**: Use Zustand only for ephemeral UI state, such as dialog visibility, selections, drag-drop, and temporary interactions. Do not use it for server data, caching, cross-feature state, or business logic.

## Observability (OpenTelemetry)

- **Scope**: Browser tracing only. Keep OpenTelemetry initialization in `src/shared/lib/telemetry` and call it once from `src/main.tsx` before the React tree renders.
- **Bootstrap**: Use `WebTracerProvider` plus browser instrumentations for document load and `fetch`. Prefer a single shared bootstrap module over ad hoc tracing setup in features or components.
- **Runtime Config**: Application deployment settings are loaded from `/runtime-config.json` at startup. In development, Vite serves that file from `VITE_*` environment variables. Do not read deployment settings from `window` globals or hardcode them in feature code.
- **Environment Flags**: Tracing must be disabled by default and enabled only when the fetched runtime config enables it. Read the collector endpoint from the runtime config, which in development is derived from `VITE_OTEL_EXPORTER_OTLP_ENDPOINT` or `VITE_OTEL_EXPORTER_OTLP_TRACES_ENDPOINT`.
- **OTLP/HTTP**: Send traces to an OTLP/HTTP collector endpoint. Do not hardcode collector URLs or credentials in source.
- **Headers**: If collector headers are required, pass them through Vite env vars using standard OTLP header formatting, such as `key=value,key2=value2`. Keep secrets out of the repository.
- **Semantic Conventions**: Set `service.name` and `service.version` on the telemetry resource. Prefer standard OpenTelemetry semantic attribute names for spans and attributes.
- **Self-Instrumentation**: Exclude the OTLP export endpoint from fetch instrumentation so the exporter does not trace its own network calls.
- **No Scattershot Spans**: Do not add tracing setup directly in feature components, pages, or widgets unless there is a strong local business reason. If manual spans are needed, keep them in shared telemetry helpers or feature-specific service modules.
