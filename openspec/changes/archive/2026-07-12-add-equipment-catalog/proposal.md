## Why

PV systems currently require equipment details to be entered manually, which is slow and produces inconsistent manufacturer and technical data. Shipping a curated inverter and solar-module catalog makes system setup repeatable while retaining an explicit path for equipment that is not yet listed.

## What Changes

- Ship a versioned catalog of inverter manufacturers/models with DC input topology, total strings, strings per MPPT, tracker counts, current limits, short-circuit limits, voltage ranges, AC output ratings, efficiency, physical, environmental, protection, and communication data.
- Ship a versioned catalog of solar-module manufacturers/models with identity, cell technology, bifacial properties, electrical ratings, temperature coefficients, operating limits, mechanical loads, dimensions, and weight.
- Add searchable, filterable catalog APIs that expose stable catalog identifiers and normalized units.
- Use catalog entries only as optional templates that prefill editable inverter and PV-module forms; persist the values confirmed for the installation rather than enforcing catalog equality.
- Keep complete manual inverter and module entry as a first-class workflow regardless of whether the catalog contains a matching model.
- Record each inverter string's module count, module manufacturer/model, peak power per module, and derived total string peak power, whether values were entered manually or prefilled from the catalog.
- Define provenance, validation, duplicate handling, catalog upgrades, and tests for bundled equipment data.

## Capabilities

### New Capabilities

- `equipment-catalog`: Bundled inverter and solar-module catalog data, normalized technical specifications, query APIs, optional form prefilling, editable installation snapshots, and unrestricted manual equipment entry.

### Modified Capabilities

None.

## Impact

- Domain and application models for inverter and module catalog entries and equipment snapshots.
- SQLite and PostgreSQL management/account persistence plus bundled seed assets.
- `/api/v1` catalog and system-equipment contracts and OpenAPI documentation.
- Onboarding and equipment-administration UI, translations, accessibility, and tests.
- Release process for validating, versioning, and attributing catalog data without relying on a network service at runtime.
