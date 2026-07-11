## 1. Catalog Domain and Assets

- [x] 1.1 Define typed inverter and solar-module catalog domain models with normalized integer units, stable IDs, revisions, optional template provenance, manual/copied/customized snapshot provenance, explicit MPPT/string topology, per-tracker current limits, DC voltage windows, AC ratings, performance, environmental, communication, and mechanical inverter fields.
- [x] 1.2 Define the versioned bundled JSON schema and add an initial reviewed catalog containing representative inverter and solar-module models with source attribution.
- [x] 1.3 Implement catalog parsing, deterministic indexing, text/manufacturer filtering, bounded pagination, detail lookup, and startup loading without network access.
- [x] 1.4 Add catalog validation for unique IDs, ordering, required provenance, plausible ranges, signed coefficients, and electrical cross-field relationships.
- [x] 1.5 Add unit and fixture tests covering the supplied 450 W bifacial module values, representative symmetric/asymmetric multi-MPPT inverters, string allocation, current/short-circuit limits, voltage-window validation, invalid entries, duplicate handling, search, and revision metadata.

## 2. Persistence and Configuration Snapshots

- [x] 2.1 Extend SQLite and PostgreSQL equipment schemas with nullable template catalog ID/revision, manual/copied/customized provenance, confirmed inverter/module specification snapshots, and per-string module count, manufacturer/model, per-module peak watts, and derived total peak watts while preserving existing manual equipment.
- [x] 2.2 Extend account configuration repositories and application commands to validate and atomically persist user-confirmed snapshots, derive total string power with overflow/bounds checks, and avoid enforcing equality to optional catalog-template values.
- [ ] 2.3 Implement explicit catalog-template reapplication and editable prefilling without silently changing configured equipment.
- [ ] 2.4 Add cross-engine repository and migration tests for legacy/manual equipment, string module composition and derived capacity, contradictory totals, overflow/bounds errors, unchanged and edited prefills, optional provenance, and catalog upgrades.

## 3. Catalog and Equipment APIs

- [ ] 3.1 Add authenticated inverter and solar-module list/detail routes with search, manufacturer filters, bounded pagination, deterministic ordering, and revision metadata.
- [ ] 3.2 Extend inverter/string equipment write and read contracts with optional template references, confirmed snapshot data, module count, manufacturer/model, per-module and derived total peak power, manual/copied/customized provenance, and safe value-validation errors.
- [ ] 3.3 Document catalog schemas, normalized units, pagination, authentication, examples, optional prefilling, editable values, and snapshot semantics in OpenAPI.
- [ ] 3.4 Add API authorization, schema, filtering, not-found, edited-prefill, manual-entry, and bidirectional route-coverage tests.

## 4. Web Equipment Workflow

- [ ] 4.1 Add typed catalog clients and TanStack Query hooks with Zod validation, bounded search caching, loading, empty, and error states.
- [ ] 4.2 Implement accessible localized inverter and solar-module catalog selectors as optional prefilling aids alongside a first-class manual-entry workflow.
- [ ] 4.3 Implement localized technical-data review tables for inverter string/MPPT topology, per-string module count/manufacturer/model/per-module power/calculated total power, DC/AC electrical limits, efficiency, environmental, communications and mechanical data plus module electrical, thermal, mechanical, dimensional, bifacial, revision, and provenance information.
- [ ] 4.4 Keep all prefilled technical fields editable, persist confirmed values through the real equipment APIs, and label manual, copied, customized, and older-template provenance without restricting configuration.
- [ ] 4.5 Add English/German translations plus component and Playwright tests for keyboard selection, manual entry despite a matching model, no-result continuity, edited prefills, validation errors, and snapshot review.

## 5. Release and Documentation

- [ ] 5.1 Document the catalog update/review workflow, normalized units, source attribution rules, custom equipment, and the non-certification/installer-safety boundary.
- [ ] 5.2 Add catalog schema validation and asset checksums to CI and release evidence.
- [ ] 5.3 Run warning-free Rust checks/tests, frontend lint/typecheck/tests/build, OpenAPI lint/compare/coverage, migration profiles, and production embedded-UI validation.
