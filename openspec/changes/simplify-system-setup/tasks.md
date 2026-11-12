# Tasks

Groups 1–5 fix the setup and navigation and can ship without groups 6–9. Groups 6–9 add data parity and import.

## 1. System model and creation

- [x] 1.1 Add additive SQLite and PostgreSQL migrations for `declared_capacity_watts`, location label, commissioning date and string temperature coefficient and shading entries; verify the migration tests pass on both databases and the previous schema version still starts.
- [x] 1.2 Allow systems without inverters in the domain and storage layers and audit forecasting, per-string charts, capacity validation and aggregation for a "missing equipment" path; verify with tests that a system without inverters ingests and reports system-level readings and that each view reports equipment as the missing input.
- [x] 1.3 Extend system create and update to accept and return name, description, location label, country code, coordinates, timezone, status interval, commissioning date and declared capacity, deriving the timezone from the location's country when omitted and from the client otherwise; verify with API tests for the name-only case, timezone derivation and each field-specific rejection.
- [x] 1.4 Make creation with inverters and strings one all-or-nothing server operation and persist string temperature coefficient and shading; verify that an invalid string leaves no system behind and that saved loss inputs are returned unchanged.
- [x] 1.5 Report declared versus derived capacity on the system resource; verify both scenarios of the declared-capacity requirement with tests.
- [x] 1.6 Document the extended system and string schemas in OpenAPI and update the contract fixture and checksum; verify `openapi:lint`, `openapi:routes`, `openapi:compare` and `test:release-artifacts` pass.
- [x] 1.7 Keep string identifiers stable when an inverter is saved: update strings that are resubmitted with their identifier, create those without one, and remove only those left out, so measurement channels bound to a string stay bound; verify with repository tests on both databases that a string's identifier and its channel binding survive an inverter update and that an omitted string is removed.
- [x] 1.8 Allow a string without module manufacturer and model, so capacity, orientation and tilt are enough to describe an array, while forecast readiness keeps reporting the missing module identity; verify with tests that such a string is stored on both databases and that the forecast names module identity as missing.

## 2. Data sources and system-restricted credentials

- [x] 2.1 Add migrations for the data-source table and for the data-source reference on stored readings on both databases; verify migration tests pass.
- [x] 2.2 Issue, regenerate and revoke telemetry-write credentials restricted to one system and owned by a data source, with audit events that contain no secret; verify with tests that such a credential is accepted for its system and rejected for another system of the same account and for every read.
- [x] 2.3 Implement data-source create, list, read, rename and delete with the one-time configuration response, and define the smalog, SBFspot, generic and import profiles with their server-rendered snippets; verify that the secret appears only in the create and regenerate responses and that an unsupported profile is rejected with the list of supported ones.
- [x] 2.4 Attribute every accepted reading to its data source and derive the connection state, last reading time and daily count; verify the never-connected, receiving, stale and disconnected states with tests that control the clock.
- [x] 2.5 Add the test-reading operation and the removal of test readings, excluding them from totals, statistics and forecasts; verify that a test reading changes no energy total and that removal deletes only marked readings.
- [x] 2.6 Document the data-source, profile and test-reading resources in OpenAPI and add an uploader guide section that shows the generated configuration; verify the OpenAPI contract checks and `test:docs` pass.

## 3. Setup status and verification

- [x] 3.1 Implement the setup-status resource with the eight items and their done, recommended, optional and blocked states; verify each scenario of the setup-status requirement with tests.
- [x] 3.2 Implement first-data verification returning the latest normalized reading and its data source, distinguishing test readings; verify with tests for no reading, test reading only, and a real reading.
- [x] 3.3 Make the dashboard and system-list responses distinguish "no reading" from a reading time and derive current, stale and absent status from telemetry; verify with API tests that a system without readings reports no observation instead of age zero.
- [x] 3.4 Document the setup-status and verification resources in OpenAPI; verify the OpenAPI contract checks pass.

## 4. Guided setup and per-system settings in the UI

- [x] 4.1 Add the `systemSetup` feature and the `/setup` route with the three steps, the "More options" disclosure, the one-time configuration view with copy actions and the waiting state that polls for the first reading; remove the unused `onboarding` feature and redirect `/onboarding`; verify with component tests for the name-only path, skipping the data source, and the waiting, test-reading and verified states.
- [x] 4.2 Redirect a signed-in user who may manage systems and has none to `/setup`; verify with a browser test from first sign-in to a verified test reading.
- [x] 4.3 Add the Settings tab to the system page with routed sections and build General (all system fields, with the location saved) and Data sources (list with state, add, rename, regenerate with confirmation, delete); verify with component tests that the sections are read-only without management permission and that regeneration warns before invalidating.
- [x] 4.4 Move the equipment editor into the Equipment section with quick entry and detailed entry, and remove "Equipment templates" and "System resources" from Administration with redirects; verify that equipment is editable in exactly one place and that the former URLs redirect.
- [x] 4.5 Build the Measurement channels section with unassigned channels highlighted and bind, rename and retire actions; verify with component tests that binding moves the channel under its target and updates the checklist.
- [x] 4.6 Build the Tariffs and Sharing sections on the existing endpoints, including link rotation with confirmation; verify create, edit and delete of a tariff and enable, copy and rotate of a link with component tests.
- [x] 4.7 Show managed data-source credentials on the account page as non-editable entries that link to their data source, and point the key creation form to data sources for uploaders; verify with a component test.

## 5. Honest states, navigation and terminology

- [x] 5.1 Add the setup checklist to the system page and the no-data and no-system empty states to the dashboard and reports, each linking to the exact settings section; verify with component tests that a system without readings shows no live indicator and that a complete checklist is hidden.
- [x] 5.2 Derive status badges in the system list and equipment cards from telemetry; verify the current, stale and absent presentations with component tests. The equipment cards that carried a status were removed in 4.4, so the status now appears in the system list and the navigation.
- [x] 5.3 Replace "Manage" with a Systems entry that carries "Add system", render navigation entries only for permitted destinations, and add a labelled Account entry; verify with a test that a user without management permission sees no entry leading to a forbidden page.
- [x] 5.4 Apply the terminology table to the `de` and `en` locale files, remove the unused `onboarding.*` namespace, and add a locale contract check that fails on retired terms; verify the check fails on a seeded retired term and passes on the cleaned files.
- [x] 5.5 Update `docs/guides` for the new setup flow with current screens and steps; verify `test:docs` passes and the documented steps match the UI.

## 6. Canonical fields for PVOutput parity

- [x] 6.1 Accept lifetime generation and consumption counters, derive interval energy from counter differences with reset detection, and write the existing lifetime columns; verify the increase, reset and both-supplied scenarios with tests.
- [x] 6.2 Accept grid import and export power and energy, write the existing grid columns and include them in interval, daily and lifetime aggregates; verify storage, aggregation and the contradiction rejection with tests.
- [x] 6.3 Write temperature and voltage to their dedicated columns, aggregate minimum and maximum temperature and mean voltage, and backfill the columns from provenance for existing readings; verify aggregates with tests and that the backfill is idempotent.
- [x] 6.4 Accept and return the optional reading message with its length limit; verify acceptance and rejection with tests.
- [x] 6.5 Add system-level daily-summary ingestion with replacement on resubmission and include supplied days in monthly and yearly statistics; verify the supplied-only and resubmission scenarios with tests on both databases.
- [ ] 6.6 Document the new observation fields, the `channels` map and the daily-summary resource in OpenAPI, and write the PVOutput field-mapping reference; verify the OpenAPI contract checks and `test:docs` pass and that every PVOutput status, output and system field appears in the reference.

## 7. Typed channels and per-device measurements

- [ ] 7.1 Migrate channel definitions to data type, unit, scale and aggregation kind and channel values to scaled integers, mapping existing channels to integer, scale 0, unit W; verify that existing channel values read back unchanged after migration on both databases.
- [ ] 7.2 Accept `value` with optional `unit` per channel, register profile-defined keys with the profile's definition, and reject values that contradict a channel's definition without rejecting the rest of the reading; verify with tests for a profile key, a decimal value and a contradicting value.
- [ ] 7.3 Implement channel storage, discovery, binding and aggregation for PostgreSQL; verify by running the channel repository contract test against both databases.
- [ ] 7.4 Aggregate channels bound to the system and to inverters according to their aggregation kind and rebuild aggregates on rebinding; verify inverter aggregates and the rebuild with tests.
- [ ] 7.5 Record equipment discovery proposals from reported device identity and reporting DC inputs, and implement adopt and dismiss; verify the unknown-serial, adopt, never-reporting-input and dismiss scenarios with tests.
- [ ] 7.6 Offer discovery proposals in the verification step and in the Measurement channels section; verify with a component test that adopting creates the inverter and strings and binds their channels.
- [ ] 7.7 Accept and store device events idempotently and list them per system and per inverter; verify that an event uploaded twice is stored once.

## 8. smalog and SBFspot parity

- [ ] 8.1 Define the smalog profile with the shared SMA key scheme, status-code labels and the `[pvlog]` configuration snippet; verify that each key registers with its documented unit, scale, target kind and aggregation and that the snippet contains no cleartext key outside the environment line.
- [ ] 8.2 Accept system and per-device battery measurements and device identity, write the existing battery columns, and update a matched inverter's model and firmware; verify the battery, out-of-range and firmware-change scenarios with tests.
- [ ] 8.3 Accept per-device daily summaries with lifetime counter and completeness flag and apply the complete-supplied, computed, incomplete-supplied precedence; verify both daily-value-precedence scenarios with tests on both databases.
- [ ] 8.4 Extend device events with source event identifier, type, category, group, message, old and new value and user group; verify idempotency per device and event identifier and that old and new values read back.
- [ ] 8.5 Store archive energy samples that have no live measurement and include them in derived interval energy; verify the archive-sample scenario with a test.
- [ ] 8.6 Implement the unified ingestion resource with complete validation before storing, identity-keyed idempotency and replacement of matured values, system-only measurements, extra device values as channels, and rejection of unknown members; verify every scenario of the unified-ingestion, idempotency and extra-values requirements with tests on both databases.
- [ ] 8.7 Derive system power, lifetime and interval energy, grid and battery values from device values with explicit system values taking precedence, return the cursor marks in the response and through the ingestion-state resource, accept gzip, enforce the limits and publish them through the capabilities resource; verify the derived-values, cursor and bulk scenarios with tests.
- [ ] 8.8 Document the ingestion, state and capabilities resources in OpenAPI and in `docs/guides/ingestion-api.md`, add the complete smalog-column mapping in `docs/guides/smalog-export.md`, generate a fixture payload from smalog schema v1 covering two inverters, three phases, sparse trackers, a battery, archive samples, daily yields and events, and add a contract test that submits it and reads every value back; verify the contract test passes on both databases, the OpenAPI contract checks pass, and every plant column of smalog schema v1 appears in the mapping.
- [ ] 8.9 Make `sbfspot-push` read `SpotData`, `Inverters` and `EventData` per serial and send the shared keys while keeping system totals in the first-class fields; verify with the fixture database that two serials produce per-inverter and per-tracker values whose sum equals the system totals and that a second run uploads no duplicate event.
- [ ] 8.10 Detect absent optional SBFspot tables and a replaced inverter, report unavailable groups, and derive no energy across a serial change; verify both scenarios with fixture tests.
- [ ] 8.11 Document SBFspot backfill, the SBFspot column mapping and the move from SBFspot to smalog in the guides; verify `test:docs` passes and that the uploader runs in dry-run mode with a generated configuration.

## 9. History import

- [ ] 9.1 Parse PVOutput status and output export files into canonical readings and daily summaries with per-row rejection reasons; verify with fixture files covering valid rows, invalid timestamps and unit edge cases.
- [ ] 9.2 Implement the import job with dry run, explicit commit, size limit, overlap handling and idempotent repetition, marking readings as imported; verify the dry-run, double-commit, overlap and size-limit scenarios with tests.
- [ ] 9.3 Add the import flow to the Data sources section with the dry-run summary and progress, and document the import resources in OpenAPI and in a migration guide; verify with a component test of the dry-run and commit states and that the OpenAPI contract checks and `test:docs` pass.

## 10. Integration checks

- [ ] 10.1 Run the complete backend, frontend, OpenAPI, documentation and bundle-budget checks with zero Rust warnings and no new lint errors; verify all commands exit successfully.
- [ ] 10.2 Run a browser test on both databases from first sign-in through setup, submission of the shared ingestion fixture through a generated data-source credential, adoption of discovered equipment and a populated dashboard; verify it passes.
- [ ] 10.3 Upgrade an instance seeded with the previous schema and existing systems, keys and channels; verify that existing uploads keep working, existing channel values are unchanged and no data source was created automatically.

## Workflow follow-up

- Implement the dedicated PVLog export in the smalog repository (change `add-pvlog-export`) against the ingestion API and shared fixture of tasks 8.6–8.8.
- Enable the smalog profile in the setup once an adapter release that passes the contract fixture is available.
