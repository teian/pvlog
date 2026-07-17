import { readFileSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";

const sourceRoot = resolve(process.argv[2] ?? "../open-pv-module-database");
const catalogPath = resolve(
  "assets/equipment-catalog/pv-module-catalog-v1.json",
);
const sourceModules = JSON.parse(
  readFileSync(resolve(sourceRoot, "dist/modules.json"), "utf8"),
);
const sourceStats = JSON.parse(
  readFileSync(resolve(sourceRoot, "dist/stats.json"), "utf8"),
);
const catalog = JSON.parse(readFileSync(catalogPath, "utf8"));
const revision = `open-pv-module-database-${sourceStats.generated_at.replaceAll("-", ".")}`;

function number(value) {
  if (value == null || String(value).trim() === "") return undefined;
  const parsed = Number(value);
  return Number.isFinite(parsed) ? parsed : undefined;
}

function scaled(value, factor) {
  const parsed = number(value);
  if (parsed == null) return undefined;
  const result = Math.round(parsed * factor);
  return result > 0 ? result : undefined;
}

function signedScaled(value, factor) {
  const parsed = number(value);
  return parsed == null ? undefined : Math.round(parsed * factor);
}

function technology(value) {
  const normalized = value.toLowerCase();
  if (normalized.includes("n-type") || normalized.includes("n type"))
    return "n_type_monocrystalline";
  if (normalized.includes("multi") || normalized.includes("poly"))
    return "polycrystalline";
  if (
    normalized.includes("thin") ||
    normalized.includes("cdte") ||
    normalized.includes("cigs") ||
    normalized.includes("a-si")
  )
    return "thin_film";
  if (normalized.includes("mono")) return "monocrystalline";
  return "other";
}

function optionalFields(record) {
  const fields = {};
  const maximumVoltage = scaled(record.max_system_voltage_v, 1_000);
  const maximumFuse = scaled(record.max_series_fuse_a, 1_000);
  const weight = scaled(record.weight_kg, 1_000);
  const length = scaled(record.length_mm, 1);
  const width = scaled(record.width_mm, 1);
  const height = scaled(record.height_mm, 1);
  if (maximumVoltage != null)
    fields.maximumSystemVoltageMillivolts = maximumVoltage;
  if (maximumFuse != null) fields.maximumSeriesFuseMilliamperes = maximumFuse;
  if (weight != null) fields.weightGrams = weight;
  if (length != null && width != null && height != null)
    fields.dimensionsMillimetres = { length, width, height };
  return fields;
}

function convert(record) {
  const specification = {
    cellTechnology: technology(record.technology),
    cellDescription: record.technology || null,
    bifacial: record.bifacial.toLowerCase() === "true",
    peakPowerWatts: scaled(record.power_stc_w, 1),
    openCircuitVoltageMillivolts: scaled(record.voc_v, 1_000),
    maximumPowerVoltageMillivolts: scaled(record.vmp_v, 1_000),
    shortCircuitCurrentMilliamperes: scaled(record.isc_a, 1_000),
    maximumPowerCurrentMilliamperes: scaled(record.imp_a, 1_000),
    efficiencyBasisPoints: scaled(record.efficiency_percent, 100),
    shortCircuitCurrentTemperatureCoefficientPpmPerCelsius: signedScaled(
      record.temperature_coefficient_isc_percent_per_c,
      10_000,
    ),
    openCircuitVoltageTemperatureCoefficientPpmPerCelsius: signedScaled(
      record.temperature_coefficient_voc_percent_per_c,
      10_000,
    ),
    peakPowerTemperatureCoefficientPpmPerCelsius: signedScaled(
      record.temperature_coefficient_pmax_percent_per_c,
      10_000,
    ),
    ...optionalFields(record),
  };
  const values = Object.values(specification);
  const calculatedPower =
    (specification.maximumPowerVoltageMillivolts *
      specification.maximumPowerCurrentMilliamperes) /
    1_000_000;
  if (
    values.some((value) => value === undefined) ||
    specification.maximumPowerVoltageMillivolts >=
      specification.openCircuitVoltageMillivolts ||
    specification.maximumPowerCurrentMilliamperes >=
      specification.shortCircuitCurrentMilliamperes ||
    specification.efficiencyBasisPoints > 10_000 ||
    specification.shortCircuitCurrentTemperatureCoefficientPpmPerCelsius < 0 ||
    specification.openCircuitVoltageTemperatureCoefficientPpmPerCelsius > 0 ||
    specification.peakPowerTemperatureCoefficientPpmPerCelsius > 0 ||
    Math.abs(calculatedPower - specification.peakPowerWatts) >
      specification.peakPowerWatts / 20
  )
    return undefined;
  const sourceReference = record.datasheet_url || record.source_url;
  if (!URL.canParse(sourceReference)) return undefined;
  return {
    id: `opvmd-${record.id}`,
    revision,
    manufacturer: record.manufacturer,
    model: record.model,
    specification,
    provenance: {
      sourceName: record.source,
      sourceReference,
      retrievedOn: /^\d{4}-\d{2}-\d{2}$/.test(record.verified_at)
        ? record.verified_at
        : null,
    },
  };
}

const imported = sourceModules.map(convert).filter(Boolean);
const curated = catalog.solarModules.filter(
  (entry) => !entry.id.startsWith("opvmd-"),
);
catalog.revision = revision;
catalog.solarModules = [...curated, ...imported]
  .map((entry) => ({ ...entry, revision }))
  .sort((left, right) =>
    left.id < right.id ? -1 : left.id > right.id ? 1 : 0,
  );
writeFileSync(catalogPath, `${JSON.stringify(catalog, null, 2)}\n`);

console.log(
  `Imported ${String(imported.length)} of ${String(sourceModules.length)} Open PV Module Database records; skipped ${String(sourceModules.length - imported.length)} records that violate PVLog's electrical consistency checks.`,
);
