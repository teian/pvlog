# Design

## Context

See proposal.md for the motivation. The constraints that shape the approach:

- **What exists and works:** system create/update (`name`, `timezone` only), inverter and string CRUD under a system, account API keys with four scopes, single and batch observation ingestion, channel auto-discovery and binding (SQLite only), tariff and sharing endpoints, the `sbfspot-push` binary.
- **What exists but is unreachable:** the `onboarding` feature (no backend), channel, tariff and sharing endpoints (no UI), `latitude_e6`/`longitude_e6` and `status_interval_seconds` columns (no write path), dedicated temperature, voltage, grid, battery and lifetime columns in `telemetry_hot` (never written), `EnergyReading::Cumulative` and `GridFlow` in the domain (no wire field), `ObservationSourceKind::Import` (nothing produces it).
- **Credentials:** the authenticated principal already carries an optional system restriction that ingestion checks against the path system, but no API sets it. System-specific keys with a custom header and push URL were removed in `add-account-api-key-management`; the spec requires bearer-only upload.
- **PVOutput** is the hosted service PVLog replaces. Users bring a system description and history from it; nothing needs to keep uploading in its protocol. `build-self-hosted-pv-platform` already lists wire compatibility with PVOutput as a non-goal.
- **smalog** (`teian/smalog`) is the primary source. It is an always-on SMA logger with its own canonical schema on SQLite or PostgreSQL (`inverters`, `inverter_measurements`, `mppt_measurements`, `battery_measurements`, `inverter_energy_samples`, `inverter_daily_yields`, `inverter_events`, `site_consumption_measurements`), explicit integer units, epoch-second timestamps, and `NULL` for missing data. Its export catalog has CSV and MQTT available and PVOutput planned; it has no PVLog export yet and gets a dedicated one. It imports SBFspot databases itself.
- **SBFspot** stays supported through the existing `sbfspot-push` binary for users who have not moved to smalog.

## Goals / Non-Goals

**Goals:**

- A new user gets from sign-in to a verified first reading without leaving one flow, without copying an identifier from the address bar, and with one required input.
- Every per-system setting is reachable from that system, and nothing is editable in two places.
- The UI states truthfully whether a system has data, and what the user should do next.
- PVLog offers one ingestion API designed for loggers — structured, idempotent by identity, cursor-driven, bulk-capable — and smalog uses it as its only path to PVLog.
- Any value PVOutput or smalog stores for a system can be sent to, stored in, and read back from PVLog, with missing values staying missing instead of becoming zero.
- Advanced users keep full control: detailed equipment, manual channel binding, hand-made API keys, raw REST.

**Non-Goals:**

- PVOutput upload protocol compatibility (see decision 9).
- Pulling history from PVOutput's API with the user's PVOutput key, or any other polling connector.
- New analytics, charts or dashboard tiles for the additional measurements beyond making them selectable where channels already are.
- Changing roles, permissions, or the account model.
- MySQL as an SBFspot source.
- Reading a smalog database from the PVLog side, and smalog's diagnostics tables (`poll_transmissions`, `inverter_query_support`, migration bookkeeping), which describe the logger, not the plant.

## Decisions

### 1. The setup is three steps and only the name is required

| Step          | Asks for                                   | Default when skipped                                                                     |
| ------------- | ------------------------------------------ | ---------------------------------------------------------------------------------------- |
| 1 System      | Name (required), location, capacity in kWp | Timezone from the location's country, else from the browser; no coordinates; no capacity |
| 2 Data source | Source profile                             | "Connect later"; the system is created without a source                                  |
| 3 Verify      | Nothing; waits for a reading               | "Finish without data"; the checklist keeps the step open                                 |

The timezone comes from the country the location search reports, using the public-domain country table of the IANA timezone database: exact for countries with one timezone, nearest reference position for the others. Exact boundary data was not bundled because it is only available under ODbL; the timezone stays visible under "More options" for the rare wrong guess near an internal boundary.

The system is created at the end of step 1, so steps 2 and 3 operate on a real system and can be resumed from the system's settings after an interruption. Advanced inputs (timezone override, status interval, commissioning date, description) sit behind one "More options" disclosure in step 1.

_Alternative considered:_ keep the single long form and add help text. Rejected: the form's problem is that it asks for equipment before the user knows whether the connection works, not that it is under-explained.

### 2. Equipment is optional and can come from the data

A system may have zero inverters. `declared_capacity_watts` on the system gives dashboards and validation a capacity until strings exist; once strings exist, capacity is derived from them and the declared value is ignored and shown as overridden.

Equipment is added in one of three ways, all landing in the same Equipment section:

1. **Adopt from the source.** When a source reports device identity (smalog `inverters`: serial, name, model, firmware) and per-input DC channels, PVLog proposes inverters and one string per MPPT tracker. The user confirms, renames, and completes module data later.
2. **Quick entry.** Capacity, orientation and tilt for a single-array system creates one inverter with one string.
3. **Detailed entry.** The existing catalog-backed editor, unchanged in capability.

Per-string analysis and yield forecasting keep their existing preconditions; the setup status names the missing input instead of the form demanding it up front.

_Alternative considered:_ keep "at least one inverter with one string" and pre-fill placeholders (today's `INV-1`/`STR-1`, 1×400 Wp). Rejected: placeholders are indistinguishable from real configuration, feed wrong capacity into forecasts, and would have to be reconciled with adopted equipment.

### 3. A data source is a first-class record that owns its credential

```
system 1 ── n data_source ── 1 upload credential (telemetry:write, restricted to the system)
                         └── profile: smalog | sbfspot | generic | import
                         └── state: derived from last accepted reading
```

- Creating a data source issues the credential and returns the cleartext once, together with a rendered configuration for the profile. The secret is never retrievable again; "Regenerate" rotates it.
- Deleting a data source revokes its credential. Revoking the credential elsewhere marks the source as disconnected.
- Each accepted observation records the data source it arrived through, which gives per-source "last reading" and lets two sources on one system be told apart.
- Connection state is derived, not stored: _never connected_, _receiving_ (last reading younger than three status intervals), _stale_, _disconnected_.

The credential is an ordinary API token with the existing optional system restriction set, so authentication, hashing, rate limiting and audit are reused. It is listed on the account page as "managed by data source X" and can be revoked there, but not edited.

_Alternative considered:_ keep account-wide keys and only improve the instructions. Rejected: an account-wide `telemetry:write` key can write to every system of the account, the UI cannot tell which uploader uses it, and the user still has to combine key and system ID by hand.

_Alternative considered:_ reintroduce system keys with a custom header or push URL. Rejected: that was removed deliberately; bearer transport stays the only one.

### 4. The ready-to-use configuration is rendered by the server

The data-source creation response includes `serverUrl`, `systemId`, `apiKey`, and `snippets[]` (`{id, title, language, content}`) for the chosen profile: a `[pvlog]` export section for smalog's `config.toml` (decision 7a) with the key referenced as `${PVLOG_API_KEY}` plus the matching environment line, an environment file for `sbfspot-push`, a `curl` test call and a JSON body for the generic profile. The server knows its public base URL and the exact field names; the client only displays and copies. Snippets containing the secret exist only in that one response.

A "Send test reading" button in step 3 stores one clearly marked test reading for the data source, so a user can verify the path before the logger is configured; a logger can do the same by sending a reading with `test: true` through its credential. Test readings are validated like real ones but kept in their own table, so no total, statistic or forecast can include them and removing them cannot touch telemetry.

### 5. Setup status is derived on the server and drives every hint

`GET /api/v1/systems/{id}/setup-status` returns an ordered list of items, each `{key, state: done|recommended|optional|blocked, reason}`:

| Key           | Done when                                       |
| ------------- | ----------------------------------------------- |
| `location`    | Coordinates are stored                          |
| `capacity`    | Declared capacity or at least one string exists |
| `data_source` | At least one data source exists                 |
| `first_data`  | At least one non-test observation is stored     |
| `channels`    | No unassigned channel has received a value      |
| `equipment`   | At least one inverter with one string exists    |
| `tariff`      | An export or import tariff is effective today   |
| `forecast`    | The existing forecast completeness check passes |

The checklist in the system header, the dashboard empty state, the forecast tab's "missing location" reason and the sidebar's "setup incomplete" marker all read this one resource. Only `data_source` and `first_data` are ever shown as _recommended next_; the rest are _optional_ so a working system is never presented as broken.

_Alternative considered:_ compute the checklist in the client from several queries. Rejected: the dashboard bug ("0 seconds ago" for no data) is exactly what client-side inference over partial responses produces.

### 6. Information architecture

```
Sidebar                         System page (/systems/:id)
  Dashboard                       Overview │ Charts │ Forecast │ Data quality │ Settings
  Systems  ── list + "Add system"                                              ├ General
  Statistics / Seasonal / Weather                                              ├ Data sources
  ── per-system links                                                          ├ Equipment
  Administration (instance only)                                               ├ Measurement channels
Account (profile, password, API keys for general API use)                      ├ Tariffs
                                                                               └ Sharing
```

- `/setup` hosts the guided flow; `/onboarding` redirects to it. A user with permission to manage systems and no system is redirected to `/setup` after sign-in.
- "Manage" disappears; "Systems" lists systems and carries "Add system". Editing happens in the system's Settings.
- Administration keeps instance concerns only (users and roles, weather feed, authentication connectors, notifications, retention, logs). "Equipment templates" and "System resources" are removed from it; the equipment catalog stays reachable from the Equipment section where it is used.
- Navigation entries are rendered only when the session holds the permission the route requires.
- Settings sections are separate routes (`/systems/:id/settings/data-sources`), so the checklist and empty states can link to the exact place.

Terminology, fixed in both locales and enforced by a locale contract test:

| Concept                          | English             | German                | No longer used                            |
| -------------------------------- | ------------------- | --------------------- | ----------------------------------------- |
| PV installation                  | System              | Anlage                | plant, system aggregate, Anlagenaggregat  |
| Uploader connection              | Data source         | Datenquelle           | uploader, credential (for this meaning)   |
| Secret for API access            | API key             | API-Schlüssel         | credential, Zugang, Zugangsdaten, API-Key |
| Named value series from a source | Measurement channel | Messkanal             | channel (alone), Kanal (alone)            |
| Webhook or mail target           | Notification target | Benachrichtigungsziel | channel                                   |
| Device catalog                   | Catalog             | Katalog               | catalogue                                 |

### 7. Source profiles carry well-known channel keys

A profile is static data shipped with PVLog: an identifier, the snippets it renders, and a table of channel keys with data type, unit, scale, target kind and aggregation. A key that matches a profile pattern is registered with that definition instead of the current default (integer, W, unassigned).

The smalog and SBFspot profiles share one key scheme for SMA devices, so a system that moves from SBFspot to smalog keeps its channels. Units are smalog's canonical units, which are already integers and need no conversion (serial `S`, tracker `N` from 1 to 255, phase `P` from 1 to 3):

| Key                                                            | smalog column                                                | Unit           | Target   | Aggregation   |
| -------------------------------------------------------------- | ------------------------------------------------------------ | -------------- | -------- | ------------- |
| `inv.S.ac.lP.power` / `.current` / `.voltage`                  | `ac_power_lP_w`, `ac_current_lP_ma`, `ac_voltage_lP_mv`      | W, mA, mV      | inverter | mean, max     |
| `inv.S.grid.frequency`                                         | `grid_frequency_mhz`                                         | mHz            | inverter | mean          |
| `inv.S.grid.import_power` / `.export_power`                    | `grid_import_power_w`, `grid_export_power_w`                 | W              | inverter | mean, max     |
| `inv.S.energy.today`                                           | `energy_today_wh`                                            | Wh             | inverter | last          |
| `inv.S.energy.lifetime`                                        | `energy_total_wh`, `inverter_energy_samples.total_energy_wh` | Wh             | inverter | counter delta |
| `inv.S.operating_time` / `.feed_in_time`                       | `operating_time_s`, `feed_in_time_s`                         | s              | inverter | last          |
| `inv.S.status` / `.grid_relay`                                 | `device_status_code`, `grid_relay_status_code`               | code           | inverter | last          |
| `inv.S.temperature`                                            | `temperature_millicelsius`                                   | m°C            | inverter | min, max      |
| `inv.S.signal`                                                 | `bluetooth_signal_permille`                                  | ‰              | inverter | mean          |
| `inv.S.mppt.N.power` / `.current` / `.voltage`                 | `mppt_measurements`                                          | W, mA, mV      | string   | mean, max     |
| `inv.S.battery.soc` / `.voltage` / `.current` / `.temperature` | `battery_measurements`                                       | ‰, mV, mA, m°C | inverter | last / mean   |

What does not travel as a channel:

| smalog data                                                  | PVLog target                                                                                                                                             |
| ------------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Sum of AC power over all inverters, sum of lifetime counters | first-class `generationPowerWatts` and `generationLifetimeWh`, so dashboards work without any binding                                                    |
| `site_consumption_measurements`                              | first-class `consumptionPowerWatts` and `consumptionEnergyWh`                                                                                            |
| Sum of grid import and export over inverters that report it  | first-class grid fields (decision 8)                                                                                                                     |
| `inverters` identity                                         | device identity on the reading; feeds discovery proposals and updates inverter firmware and model                                                        |
| `inverter_daily_yields` with `is_complete`                   | per-inverter daily summaries (decision 8)                                                                                                                |
| `inverter_events`                                            | device events with the source's `device_event_id` as idempotency key and type, category, group, message, old and new value and user group kept as fields |

Three smalog properties become rules of the wire format: an absent value is omitted and stored as missing, never as zero; tracker numbers are sparse and are not renumbered; status values stay numeric codes and are labelled at display time from the profile.

SBFspot maps onto the same keys with trackers 1 and 2 from `Pdc`/`Udc`/`Idc`, phases from `Pac`/`Uac`/`Iac`, and no battery or grid keys. A serial that matches no inverter creates a discovery proposal (decision 2) rather than an unassigned channel per value.

This requires channels to store more than one integer: values are stored as scaled integers with the channel's `scale`, and the wire accepts `value` with an optional `unit` next to today's `powerWatts`/`energyWattHours`. `tracks_power` and `tracks_energy` are replaced by the profile's aggregation kind, which the aggregation actually reads.

### 7a. One ideal ingestion API, which smalog uses

PVLog defines the ingestion API a logger would want, and smalog is its first and reference client. It is PVLog's API, not a smalog format: nothing in it names smalog, and any logger that knows devices can use it. The existing `/observations` resources stay as the minimal form for scripts and feed the same pipeline.

`POST /api/v1/systems/{id}/ingest`

```
{ "schemaVersion": 1,
  "source":       { "name": "smalog", "version": "0.3.0" },
  "devices":      [ { "id": "2000123456", "kind": "inverter", "familyId", "configuredName",
                      "reportedName", "model", "firmware", "transport" } ],
  "measurements": [ { "measuredAt",
                      "system":  { "consumptionPowerW", "consumptionEnergyWh", ... },
                      "devices": [ { "id",
                                     "ac": [ { "phase", "powerW", "currentMa", "voltageMv" } ],
                                     "gridFrequencyMhz", "gridImportPowerW", "gridExportPowerW",
                                     "energyTodayWh", "energyLifetimeWh", "operatingTimeS", "feedInTimeS",
                                     "statusCode", "gridRelayStatusCode",
                                     "temperatureMillicelsius", "signalPermille",
                                     "trackers": [ { "number", "powerW", "currentMa", "voltageMv" } ],
                                     "battery": { "stateOfChargePermille", "voltageMv", "currentMa", "temperatureMillicelsius" },
                                     "extra": { "<key>": { "value", "unit" } } } ] } ],
  "energySamples":  [ { "deviceId", "measuredAt", "energyLifetimeWh", "powerW" } ],
  "dailySummaries": [ { "deviceId", "date", "energyWh", "energyLifetimeWh", "complete" } ],
  "events":         [ { "deviceId", "eventId", "occurredAt", "code", "type", "category", "group", "message", "oldValue", "newValue", "userGroup" } ] }
```

`GET /api/v1/systems/{id}/ingest/state` returns, for the calling data source, the latest measurement time, sample time, summary date and event time PVLog holds per device. `GET /api/v1/ingest/capabilities` returns the supported schema versions, item and size limits, and accepted encodings.

The properties that make it the right API for a logger:

| Property                             | What it means                                                                                                                                                                                                                                                                                                                                                                               |
| ------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| The logger sends what it has         | Devices, their measurements, archive samples, daily summaries and events travel in one request, structured as the logger stores them. Units are integer SI sub-units named in the field (`W`, `Wh`, `mV`, `mA`, `mHz`, `s`, `‰`, m°C), timestamps are epoch seconds, an absent member means missing and is never read as zero. For smalog this is serialization of rows without arithmetic. |
| PVLog derives the system view        | System power, lifetime and interval energy, grid and battery values are derived from the devices that reported them. A source without device detail sends the `system` member instead; both may be combined, and an explicit system value wins. The mapping to channels (decision 7) is server-side, so it can evolve without a logger release.                                             |
| Identity is the idempotency key      | Measurements and samples are keyed by device and time, summaries by device and date, events by device and event id. No `Idempotency-Key` header, no per-item keys. Re-sending is always safe; a value that matures (today's counter, an incomplete daily summary) replaces its predecessor.                                                                                                 |
| The answer is a cursor               | The response reports per category the stored, unchanged and rejected counts (rejections with index and reason) and the new high-water marks. A logger advances a durable cursor over exactly what PVLog holds, and can ask `ingest/state` when it has lost its own state.                                                                                                                   |
| History and live data are one path   | A request carries up to 5,000 measurements and may be gzip-compressed; the limits are discoverable. First synchronization of years of history, catching up after an outage, and the regular cycle use the same request.                                                                                                                                                                     |
| It can grow without breaking         | `schemaVersion` is explicit, unknown members are rejected with a field-specific problem rather than ignored, and device values PVLog has no field for go into `extra` with a unit, where they become typed measurement channels.                                                                                                                                                            |
| It is still ordinary PVLog ingestion | The data source's system-restricted bearer credential, validation, backpressure with `Retry-After`, rate-limit headers and audit apply unchanged. A request is validated completely before anything is stored.                                                                                                                                                                              |

smalog's side — a dedicated export that reads its database by cursor and posts to this API — is the companion change `add-pvlog-export` in the smalog repository. PVLog publishes the OpenAPI schema and a fixture request that both repositories test against.

_Alternative considered:_ extend `/observations/batch` with a flat channel map per item. Rejected: the logger would have to know PVLog's channel keys and compute plant totals, each item would repeat long key strings and need its own idempotency key, and daily summaries, events and device identity would remain separate calls.

_Alternative considered:_ a `smalog-push` binary in this repository that reads smalog's database, like `sbfspot-push`. Rejected: a second process and configuration on the logger host, and SQLite and PostgreSQL readers for a schema that smalog owns and may migrate.

_Alternative considered:_ subscribe to smalog's MQTT topics. Rejected: no delivery guarantee, no history, no events or daily yields, and a broker connection per system.

### 8. PVOutput parity and system-level smalog values live in the canonical model

| PVOutput                                                                                                     | PVLog wire field                                                                                                          | Storage                                                                                     |
| ------------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------- |
| `v1` energy generation                                                                                       | `generationEnergyWh`                                                                                                      | existing                                                                                    |
| `v2` power generation                                                                                        | `generationPowerWatts`                                                                                                    | existing                                                                                    |
| `v3` energy consumption                                                                                      | `consumptionEnergyWh`                                                                                                     | existing                                                                                    |
| `v4` power consumption                                                                                       | `consumptionPowerWatts`                                                                                                   | existing; added to rollups                                                                  |
| `v5` temperature                                                                                             | `temperatureMillidegreesCelsius`                                                                                          | dedicated column and min/max rollup, no longer provenance only                              |
| `v6` voltage                                                                                                 | `voltageMillivolts`                                                                                                       | dedicated column and mean rollup                                                            |
| `c1` cumulative                                                                                              | `generationLifetimeWh`, `consumptionLifetimeWh`                                                                           | existing lifetime columns; interval energy derived from counter deltas with reset detection |
| `n` net                                                                                                      | `gridImportPowerWatts`, `gridExportPowerWatts`, `gridImportEnergyWh`, `gridExportEnergyWh`                                | existing grid columns and rollups                                                           |
| `v7`–`v12` extended                                                                                          | `channels` with decimal `value` and `unit`                                                                                | channel values                                                                              |
| `m1` text message                                                                                            | `message` (at most 30 characters)                                                                                         | new nullable column                                                                         |
| Daily output                                                                                                 | `dailySummaries` of the ingestion API (decision 7a)                                                                       | new source-summary table                                                                    |
| smalog daily yield per inverter                                                                              | same resource with a device identifier, lifetime counter and completeness flag                                            | same table, keyed additionally by device                                                    |
| Battery (smalog)                                                                                             | `batteryStateOfChargeBasisPoints`, `batteryPowerWatts`, `batteryEnergyWh` for the system; per-inverter values as channels | existing battery columns, now written                                                       |
| System size, panels, inverter, orientation, tilt, shade, install date, coordinates, status interval, tariffs | system, string and tariff fields                                                                                          | existing columns, now writable                                                              |

Lifetime counters are separate fields rather than a flag on the interval fields: a client may send either or both, the meaning of each field never depends on another field, and existing clients are unaffected.

Daily summaries from a source are stored next to, not instead of, summaries computed from interval data. For a given day, a source summary marked complete wins, because the inverter's own counter is more accurate than integrated samples; otherwise computed values win when interval coverage is sufficient; otherwise the incomplete source summary is used and marked as such. This is what makes PVOutput history usable, where older days often exist only as daily outputs.

### 9. No PVOutput wire compatibility

PVOutput is replaced, not emulated. PVLog covers its data model (decision 8) and imports its history (decision 10); new data arrives from smalog's dedicated export. The protocol itself would require key and system ID in custom headers or the query string, which the spec forbids and the code rejects on ingestion routes, and nothing in the target setup still speaks it.

### 10. History import reuses the ingestion path

- **PVOutput CSV:** an import accepts PVOutput's exported status and output files, shows a dry-run summary (range, rows, rejected rows with reasons, overlap with stored data), and on confirmation writes through the same normalization as live ingestion with source kind `import` and idempotency keys derived from system, source and timestamp, so repeating an import is harmless.
- **smalog:** no server import. Its export synchronizes stored history through the same cursor as live data (decision 7a); the Data sources section shows backfill progress from the timestamps it receives.
- **SBFspot:** no server import either. `sbfspot-push` already backfills from a start time and the generated configuration includes that setting. Users with an old SBFspot database can alternatively migrate it into smalog first and backfill from there.

Imports run as jobs in the existing `import_jobs` table and are limited in size per file; larger histories are imported file by file.

### 11. Fixes that ride along because the setup depends on them

- System create and update accept and return location label, country code, coordinates, timezone, status interval, commissioning date and declared capacity; the location search returns the country code.
- Strings persist temperature coefficient and shading entries.
- System creation with equipment becomes one server-side operation instead of a client-driven sequence. The system registry and the equipment live in different databases, so one transaction cannot span them: all equipment is validated completely before the first record is written, and a storage failure afterwards removes the system that was just created.
- Saving an inverter no longer deletes and recreates its strings. Strings carry their identifier in the request and keep it, because measurement channels and adopted equipment refer to strings by identifier.
- A string no longer requires module manufacturer and model, which the quick entry does not ask for; forecast readiness already reports the missing module identity.
- The dashboard response distinguishes "no observation" from "observation at time T"; status badges in the system list and equipment cards are derived from telemetry, not lifecycle.
- Measurement channels are implemented for PostgreSQL.
- The OpenAPI contract documents the `channels` map that the server already accepts.

## Risks / Trade-offs

- **Scope is large** → The task list is ordered so that groups 1–4 (setup, data sources, settings, honest states) are shippable on their own and fix the reported confusion; data parity and import follow in later groups without rework.
- **Systems without equipment break assumptions** in forecasting, per-string charts and capacity validation → Each consumer already has a "not ready" path for missing inputs; group 1 audits them and the setup status reports the reason. Capacity validation falls back to the declared capacity.
- **Channel storage change** (single integer to scaled value with type) touches aggregation → Existing channels migrate as integer, scale 0, unit W, which is their current meaning; rollups are rebuilt by the existing invalidation mechanism.
- **Auto-adopted equipment may be wrong** (a second MPPT input reported but unused) → Adoption is always a proposal the user confirms; inputs that never report power are not proposed.
- **A credential shown once is easy to lose** → Regenerating is one action in the Data sources section and re-renders the snippets; the old secret stops working immediately, stated before confirmation.
- **Test readings could pollute totals** → Flagged, excluded from energy sums and from `first_data`, and removable from the same screen.
- **Computed versus source daily summaries can disagree** → The precedence rule is fixed (decision 8) and the daily view shows which basis was used.
- **The smalog adapter lives in another repository** → PVLog ships the contract, fixture and contract test first; the setup shows the smalog profile as available only once an adapter version that implements the contract is released, and offers the generic profile until then.
- **Removing panels from Administration** may surprise existing operators → The removed entries are replaced by a link to the system's Settings for one release.

## Migration Plan

1. Storage migrations are additive on both databases: new tables for data sources, source daily summaries and device events; new nullable columns on systems, strings, channel definitions and hot telemetry.
2. Existing systems keep their equipment; `declared_capacity_watts` stays null. Existing account API keys keep working for upload and are shown under Account as before. No data source is created automatically; the Data sources section offers "Create a data source for an existing uploader", which issues a new credential.
3. Existing channel definitions are migrated to integer, scale 0, unit W with the aggregation kind matching their `tracks_power`/`tracks_energy` values.
4. Voltage and temperature already stored in provenance JSON are copied into their dedicated columns by a one-time background job; reads use the column and fall back to provenance until the job has finished.
5. `/onboarding` and the removed Administration sections redirect to their replacements.
6. Rollback: the migrations add only nullable columns and new tables, so the previous version runs against the migrated schema. Data written through the new fields is ignored by the previous version, not lost.

## Open Questions

- Which additional source profiles are worth shipping after smalog, SBFspot and generic (Home Assistant, OpenDTU, a Solar-Log export)? Profiles are data, so adding one does not change this design.
- Should the public sharing page show the new per-inverter values? Deferred; sharing keeps its current content.
