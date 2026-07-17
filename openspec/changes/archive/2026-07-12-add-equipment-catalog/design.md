## Context

PVLog models configured inverters and inverter-owned PV strings but has no bundled equipment knowledge. Operators currently enter manufacturer/model and capacity data manually. The catalog must work offline in self-hosted installations, be identical across SQLite and PostgreSQL, preserve historical configuration, and express decimal datasheet values without floating-point ambiguity.

## Goals / Non-Goals

**Goals:**

- Bundle useful, validated inverter and solar-module model lists with manufacturer attribution.
- Normalize common electrical, thermal, mechanical, and physical datasheet values into explicit integer units.
- Provide fast searchable APIs and accessible onboarding/administration selectors without a runtime network dependency.
- Preserve the exact specifications selected for a configured system even after catalog upgrades.
- Treat catalog selection solely as optional form prefilling and support fully manual or edited equipment values without reduced functionality.

**Non-Goals:**

- Scraping manufacturer websites or automatically importing copyrighted datasheets.
- Claiming certification, compatibility, warranty, or design approval for an inverter/module combination.
- Modeling every manufacturer-specific feature, optimizer, battery, mounting product, or regional certification in the first version.
- Replacing electrical-design validation by a qualified installer.

## Decisions

### Bundle a versioned catalog asset instead of seeding mutable database rows

Catalog entries live in reviewed JSON assets compiled into the server and UI release. Each asset has a schema version, catalog revision, source/provenance metadata, and stable opaque entry IDs. The application parses and validates the asset once at startup and serves queries from an immutable in-memory index.

This keeps SQLite and PostgreSQL behavior identical, avoids migration churn for catalog updates, and permits deterministic release checks. Database-seeded rows and an external catalog service were rejected because they complicate upgrades and make offline behavior depend on mutable state.

### Store normalized integer units and preserve display metadata

Electrical values use millivolts, milliamperes, and watts; ratios and efficiencies use basis points; temperature coefficients use signed parts-per-million per degree Celsius; temperatures use milli-degrees Celsius; dimensions use millimetres; weight uses grams; mechanical loads use pascals. Ranges have explicit minimum and maximum fields. Optional values remain absent rather than being represented as zero.

The UI localizes formatting and converts normalized values back to familiar datasheet units. Free-text notes are restricted to supplemental information such as cell construction because calculations must use typed fields.

### Treat catalog entries as editable templates, not authoritative equipment

Selecting a catalog entry copies its values into the equipment form. Every copied field remains editable, and saving persists the values confirmed by the user as the installation snapshot. The optional catalog entry ID/revision is retained only as template provenance; it does not constrain the saved values or create a live link. A fully manual form is available before and after search, including when no matching catalog entry exists.

Subsequent catalog revisions never mutate configured equipment. Users may explicitly reapply a newer template to the form, review the resulting changes, edit them, and confirm a new snapshot. This costs some duplicated data but guarantees reproducible calculations and ensures the catalog helps data entry without becoming a gatekeeper.

### Make string nameplate capacity a validated derivation

Every inverter-owned PV string stores a positive module count, the confirmed module manufacturer and model, and peak power per module in watts. The server derives total string peak power as `module_count × module_peak_power_watts`, checks for overflow and configured bounds, and stores/returns the derived value used by capacity calculations. Clients display the total live while editing but cannot establish a contradictory total.

Catalog selection may prefill manufacturer, model, and module peak power; all three remain editable. This keeps manual equipment fully supported while ensuring string capacity is consistent. Independently entered module count and total power were rejected because they permit contradictory configuration.

### Keep inverter and module schemas distinct

Inverter entries use the following typed groups:

- **DC input:** topology, total PV-string inputs, MPPT count, strings per MPPT, maximum input voltage, start voltage, nominal input voltage, MPPT minimum/maximum voltage, maximum input current and maximum short-circuit current. Per-MPPT records override shared values where tracker ratings differ.
- **AC output:** phase count, nominal grid voltage, supported voltage/frequency ranges, rated and maximum active power, maximum apparent power, maximum output current, power-factor range, and total harmonic distortion when published.
- **Performance and operation:** maximum/European-weighted efficiency, standby/night consumption, operating-temperature range, derating behavior, cooling method, acoustic noise, ingress-protection rating, humidity range, maximum altitude, and supported communication interfaces.
- **Mechanical:** width, height, depth, and weight.

Module entries include manufacturer/model, cell technology, bifacial state/factor/tolerance, Pmax, Voc, Vmp, Isc, Imp, efficiency, signed Isc/Voc/Pmax temperature coefficients, maximum system voltage, operating-temperature range, maximum series fuse, front/rear static load, dimensions, and weight.

A single generic key/value equipment schema was rejected because it would weaken validation, OpenAPI documentation, filtering, and future compatibility calculations. MPPT topology is represented explicitly instead of as one aggregate string count so asymmetric inverters remain accurate.

### Expose read-only catalog APIs with bounded search

`GET /api/v1/equipment-catalog/inverters` and `/solar-modules` support normalized text search, manufacturer filters, bounded pagination, and selected technical filters. Detail routes return one entry by stable ID. Catalog reads require an authenticated browser session or appropriately scoped API credential but do not grant access to any account data.

System equipment write APIs accept the confirmed specification snapshot and optional template provenance. The server validates technical values and catalog-reference existence but SHALL permit values to differ from the referenced template. It records whether values were entered manually, copied unchanged, or customized after prefilling.

### Validate catalog quality during build and startup

Contract tests enforce unique IDs, normalized manufacturer/model names, plausible ranges, cross-field electrical relationships, valid coefficients, nonnegative limits, provenance, and deterministic ordering. Startup fails with a safe configuration error if the embedded catalog is invalid. Release checks record the catalog asset checksum.

## Risks / Trade-offs

- **[Catalog data becomes stale]** → Version every release, expose revision/provenance, and document a review/update workflow.
- **[Datasheet transcription error]** → Require source attribution, automated plausibility checks, and reviewable per-entry fixtures.
- **[Snapshot duplication increases storage]** → Store compact typed columns/JSON once per configured model; prioritize historical correctness over minor storage cost.
- **[Users assume electrical compatibility]** → Present catalog data as reference information and avoid automatic compatibility claims in this change.
- **[Schema cannot cover every model]** → Keep optional typed fields and manual entry as a coequal path; extend schema additively when common data warrants it.

## Migration Plan

1. Add and validate the embedded catalog assets without changing configured equipment.
2. Add nullable catalog-template provenance, editable snapshot fields, module count, module manufacturer/model, per-module peak power, and derived total peak power to inverter/string configuration persistence.
3. Deploy read-only catalog APIs and UI selectors.
4. Existing equipment remains manual and unchanged; users may optionally use a catalog entry to prefill a later edit.
5. Rollback ignores the additive fields while retaining existing equipment configuration.

## Open Questions

- Which manufacturers and models form the initial reviewed catalog set?
- Which public or manufacturer-provided datasheet sources may be redistributed with acceptable attribution?
- Should later releases support administrator-supplied catalog overlays signed or validated against the same schema?
