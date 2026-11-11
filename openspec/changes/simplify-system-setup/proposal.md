# Proposal

## Why

Setting up a new PV system is currently the most confusing part of PVLog. There is no guided flow at all: `/onboarding` only opens the system-management list, the prepared onboarding client and its `onboarding.*` strings are dead code, and the backend endpoints they call do not exist. A user must create a system under "Manage", create an account-wide API key under "Account", and then find the system UUID in the browser address bar, because the UI never shows the system ID, the upload URL, or any uploader instructions. The form forces inverter and string details before any data exists, silently discards the location, temperature coefficient and shading it asks for, never asks for a timezone, and a system without data is shown as "Live data received 0 seconds ago". Channels, tariffs and sharing exist in the backend but have no UI, while equipment can be edited in two unrelated places.

At the same time PVLog cannot yet take over from PVOutput, the hosted service it is meant to replace, and cannot receive what its primary source records. PVOutput status fields such as cumulative energy, net flow, extended values and daily outputs have no equivalent on the wire, temperature and voltage are only kept as provenance, and years of PVOutput history cannot be brought along. smalog, the SMA logger that is the intended primary source, records per-inverter AC phase values, a dynamic set of MPPT trackers, battery state, grid import and export, lifetime counters, archive samples, daily yields with a completeness flag and structured device events, and has no way to deliver any of it to PVLog. The existing SBFspot uploader collapses all inverters into one sum and drops almost everything else.

## What Changes

**One guided setup with a single required field**

- Replace the dead onboarding code with a three-step setup: _System_ (name required; location, capacity and timezone optional and inferred where possible), _Connect a data source_, and _Verify first data_.
- Make equipment optional at creation. A system can exist with only a name and a declared capacity; inverters and strings become a detail that is added later or adopted from what the data source reports.
- Persist everything the form asks for: location with coordinates, timezone, status interval, temperature coefficient and shading.

**Data sources instead of loose keys and IDs**

- Introduce a _data source_ per system: a named connection with a source profile (smalog, SBFspot, generic uploader, import), its own upload credential, and a visible state (never connected, receiving, stale, last reading).
- Creating a data source issues an upload-only credential restricted to that one system and shows, once, a complete ready-to-paste configuration: server URL, system ID, key, and a profile-specific snippet (for example the export section for smalog's `config.toml`, the `sbfspot-push` environment file, or a `curl` test call).
- Let the setup wait for the first reading, show it normalized, and offer to adopt inverters and strings discovered from the source.

**One place per system**

- Add a _Settings_ area to each system with sections General, Data sources, Equipment, Measurement channels, Tariffs and Sharing, and give channels, tariffs and sharing the UI they lack.
- Add a server-derived setup status per system (what is done, what is recommended next) and show it as a checklist; replace misleading "live" and "online" indicators with honest _no data yet_ and _stale_ states.
- Remove the duplicate equipment editor and the first-system-only resource view from Administration, rename the "Manage" entry to the thing it opens, hide navigation entries the user is not permitted to use, and use one term each for system, data source, API key and measurement channel in German and English.
- Keep account API keys on the account page for general API use; they are no longer the way to connect an uploader.

**Data parity with PVOutput and smalog**

- Accept and persist every measurement PVOutput stores per status (energy and power for generation and consumption, cumulative counters, net import/export, temperature, voltage, six extended values with decimals, a short text message) and per day (generated, exported, consumed, peak power and time, weather condition, minimum and maximum temperature, import by tariff band).
- Accept and persist everything smalog's canonical schema holds: inverter identity (serial, SUSy ID, configured and reported name, model, firmware, transport); per poll and inverter the three AC phases with power, current and voltage, grid frequency, grid import and export power, daily and lifetime energy, operating and feed-in time, device and grid-relay status, temperature and Bluetooth signal; any number of MPPT trackers with DC power, current and voltage; battery state of charge, voltage, current and temperature; archive energy samples; per-inverter daily yields with their completeness flag; site consumption; and device events with type, category, group, message, old and new value and user group.
- Add the ingestion API a logger would want and make it PVLog's primary upload path: one request carries devices, their measurements (AC phases, MPPT trackers, battery), system values, archive samples, daily summaries and events in integer units; items are idempotent by device and time without per-item keys; the response and a state resource act as a synchronization cursor; history and live data use the same compressed bulk request; PVLog derives system totals. smalog is its reference client through a dedicated export (companion change `add-pvlog-export` in the smalog repository). The existing observation resources remain as the minimal form for scripts.
- Keep SBFspot as a second profile: its uploader sends the same per-inverter keys for the subset SBFspot records.
- Ship well-known channel keys per source profile so these values are typed and bound automatically instead of arriving as unassigned integer watts, and make measurement channels work on PostgreSQL as they do on SQLite.
- Add history import for PVOutput CSV exports, so a migrated system starts with its past data; smalog and SBFspot backfill through their uploaders.

**Not in this change:** PVOutput's upload protocol (`addstatus.jsp`, `X-Pvoutput-*` headers). PVOutput is the service being replaced, not a transport to keep: its data model is covered and its history is imported, and uploads come from smalog's dedicated export.

## Capabilities

### New Capabilities

- `system-setup`: The guided first-system and additional-system setup, the server-derived setup status and recommended next steps, and verification that a system receives data.
- `data-source-connections`: Per-system data sources with source profiles, system-restricted upload credentials, ready-to-use uploader configuration, connection health, and adoption of equipment discovered from a source.

### Modified Capabilities

- `web-application`: Replace the guided-onboarding requirement with the new setup flow; add the per-system settings area, honest empty and stale states, permission-aware navigation, and consistent terminology.
- `pv-system-management`: Allow a system without equipment and with a declared capacity; persist location, timezone, status interval and string loss inputs; type, discover and bind measurement channels on both databases.
- `telemetry-ingestion`: Extend the canonical wire fields to PVOutput and smalog parity, add per-inverter, per-tracker and battery measurements, system and per-inverter daily summaries, structured device events, and history import.
- `identity-and-access`: Add upload credentials restricted to a single system and owned by a data source.

## Impact

- **Frontend:** new `systemSetup` and `dataSources` features, a system settings route with six sections, new channel, tariff and sharing UIs, dashboard and reporting empty states, sidebar and route changes (`/onboarding` becomes `/setup` with a redirect), removal of the `onboarding` feature and of the equipment panels in Administration, and reworked `de`/`en` locale namespaces.
- **API and OpenAPI:** new data-source, setup-status and import resources; extended observation, system and string schemas; daily-summary and event ingestion; an optional system restriction on credential creation.
- **Backend:** system creation and update use cases, credential issuing, telemetry normalization and persistence (dedicated columns instead of provenance JSON), aggregation of temperature, voltage, grid and per-inverter values, channel typing and binding, and PostgreSQL parity for channels.
- **Storage:** migrations for data sources, system capacity and location fields, channel typing, per-device measurements, daily source summaries and device events, on SQLite and PostgreSQL.
- **Uploaders:** the ingestion API with its OpenAPI schema and a shared fixture, used by smalog's dedicated PVLog export (implemented in the smalog repository as the companion change `add-pvlog-export`, which also retires smalog's planned PVOutput export); `sbfspot-push` reads `SpotData`, `Inverters` and `EventData` per serial and sends the shared per-inverter keys. Both configurations can be generated by the UI.
- **Documentation:** uploader guides, a PVOutput, smalog and SBFspot field-mapping reference, and a migration guide.
- **Compatibility:** existing systems, keys and uploads keep working. No breaking change.
