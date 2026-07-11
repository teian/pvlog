# API and domain reference

## Glossary and semantics

- **System**: aggregate root containing inverters; each inverter owns its PV
  strings. Capacity is calculated from effective-dated strings.
- **Observation**: canonical timestamped values with explicit integer units,
  source/provenance, and quality flags.
- **Coverage**: fraction of expected intervals represented by acceptable data;
  it is not a synthetic fill value.
- **UTC/timezone**: timestamps are instants in UTC; IANA timezones define local
  calendar rollups and DST boundaries.
- **Correction overlay**: immutable correction/deletion applied over hot or
  archived data until segment folding.

Units include watts, watt-hours, basis points, milli-degrees Celsius, and minor
currency units. Quality distinguishes measured, estimated, suspect, rejected,
corrected, and missing data.

## Permission summary

| Scope                             | Purpose                                  |
| --------------------------------- | ---------------------------------------- |
| `systems:read`, `systems:write`   | Aggregate configuration                  |
| `telemetry:write`                 | Observation ingestion/correction         |
| `analytics:read`                  | Series, statistics, and quality          |
| `alerts:read`, `alerts:write`     | Rules and events                         |
| `webhooks:read`, `webhooks:write` | Subscriptions and delivery               |
| `roles:manage`, `audit:read`      | Delegation and audit                     |

Authorization is deny by default and occurs before account-database routing.

## Architecture and compatibility

The domain/application layers are storage and HTTP neutral. SQLite routes each
account to an opaque database; PostgreSQL carries `account_id` in owned keys.
Hot telemetry compacts into versioned Protobuf/Zstandard segments while overlays
preserve immediate corrections. Jobs use leases, heartbeats, idempotent handlers,
bounded retries, and dead letters.

The API follows semantic contract versions. Additive optional fields are
backward-compatible; removals or semantic changes require a new major version
and a documented deprecation window. Generated-client examples consume the
committed OpenAPI document and must not rely on undocumented response fields.
