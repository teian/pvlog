# Equipment catalog and custom equipment

PVLog ships a reviewed inverter and solar-module catalog inside each release. The server loads it locally at startup and never needs a manufacturer service or internet connection at runtime. Catalog selection is optional: it copies values into an editable form, and PVLog stores only the snapshot explicitly confirmed for the installation.

## Safety boundary

Catalog values are reference data, not certification, electrical design, warranty advice, or a compatibility statement. PVLog does not approve combinations of modules, strings, inverters, protection devices, cables, or grid settings. A qualified installer must verify voltage and current limits across the full temperature range, string fusing, local grid rules, mounting loads, grounding, protection, and manufacturer instructions.

## Normalized units

Catalog JSON uses integers to avoid floating-point transcription ambiguity:

| Quantity                                        | Stored unit                     |
| ----------------------------------------------- | ------------------------------- |
| Voltage                                         | millivolts                      |
| Current and fuse rating                         | milliamperes                    |
| Active power                                    | watts                           |
| Apparent power                                  | volt-amperes                    |
| Efficiency, power factor, humidity, bifaciality | basis points (10,000 = 100%)    |
| Temperature coefficient                         | signed parts per million per °C |
| Temperature                                     | milli-degrees Celsius           |
| Frequency                                       | millihertz                      |
| Dimensions                                      | millimetres                     |
| Weight                                          | grams                           |
| Static load                                     | pascals                         |
| Acoustic noise                                  | millidecibels                   |

The UI formats these values into familiar engineering units. Missing published values remain absent and must never be encoded as zero.

## Using custom equipment

Manual entry has the same capabilities as a catalog-prefilled configuration. It remains available when no model matches and when a matching entry exists. For every PV string, enter module manufacturer and model, a positive module count, and peak watts per module. The server calculates and stores total string peak power; clients cannot establish a contradictory total.

Selecting a catalog entry records its stable ID and revision as optional provenance. Unchanged values are labelled `catalog_copied`; edited values are `catalog_customized`; fully manual data is `manual`. A later release never changes a configured snapshot. Reapplying a newer template is an explicit edit action and remains unsaved until confirmed.

## Updating the bundled catalog

1. Use a stable manufacturer product page or manufacturer-published datasheet. Do not scrape sites or copy descriptive copyrighted material.
2. Add only technical facts needed by the typed schema in `assets/equipment-catalog/catalog-v1.schema.json`.
3. Preserve an existing stable ID for the same model. Increase the catalog revision for reviewed corrections or additions.
4. Record `sourceName`, a direct `sourceReference`, and the review/retrieval date. Review regional model variants and datasheet revisions explicitly.
5. Keep inverter and module arrays sorted by ID. Represent asymmetric MPPT current limits per tracker.
6. Run schema validation, application catalog tests, OpenAPI checks, and release checksum generation.
7. Have a second reviewer compare every transcribed value with the attributed source, including signs and unit conversions.

Catalog corrections affect only future prefills. Existing snapshots intentionally retain the older revision and confirmed values for reproducibility.
