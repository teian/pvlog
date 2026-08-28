import type {
  InverterCatalogEntry,
  SolarModuleCatalogEntry,
} from "@/features/equipmentCatalog/types/equipmentCatalog.types";
import type { TFunction } from "i18next";

/** One label/value pair of a technical review table. */
export type TechnicalDataRow = readonly [string, string];

/** Placeholder shown when a catalog entry omits an optional specification. */
const MISSING = "—";

/** Formats the operational rows of an inverter, each of which may be absent. */
function inverterOperationalRows(
  entry: InverterCatalogEntry,
  t: TFunction,
): TechnicalDataRow[] {
  const { operational } = entry;
  const temperature = operational.operatingTemperature;
  const dimensions = operational.dimensionsMillimetres;
  return [
    [
      t("features.equipmentCatalog.fields.efficiency"),
      operational.maximumEfficiencyBasisPoints
        ? `${String(operational.maximumEfficiencyBasisPoints / 100)} %`
        : MISSING,
    ],
    [
      t("features.equipmentCatalog.fields.temperature"),
      temperature
        ? `${String(temperature.minimumMilliCelsius / 1000)}–${String(temperature.maximumMilliCelsius / 1000)} °C`
        : MISSING,
    ],
    [
      t("features.equipmentCatalog.fields.dimensions"),
      dimensions
        ? `${String(dimensions.length)} × ${String(dimensions.width)} × ${String(dimensions.height)} mm`
        : MISSING,
    ],
    [
      t("features.equipmentCatalog.fields.weight"),
      operational.weightGrams
        ? `${String(operational.weightGrams / 1000)} kg`
        : MISSING,
    ],
  ];
}

/** Builds the technical review rows for a selected inverter catalog entry. @param entry - Selected inverter entry. @param t - Active translator. @returns Localized label/value rows. */
export function inverterTechnicalRows(
  entry: InverterCatalogEntry,
  t: TFunction,
): TechnicalDataRow[] {
  const { ac, dc, operational, provenance, revision } = entry;
  return [
    [t("features.equipmentCatalog.fields.revision"), revision],
    [t("features.equipmentCatalog.fields.topology"), dc.topology ?? MISSING],
    [t("features.equipmentCatalog.fields.mppts"), String(dc.mpptInputs.length)],
    [
      t("features.equipmentCatalog.fields.strings"),
      String(dc.totalStringInputCount),
    ],
    [
      t("features.equipmentCatalog.fields.mpptAllocation"),
      dc.mpptInputs
        .map(
          (tracker) =>
            `${String(tracker.trackerIndex)}: ${String(tracker.stringInputCount)}`,
        )
        .join(", "),
    ],
    [
      t("features.equipmentCatalog.fields.dcVoltage"),
      `${String((dc.minimumMpptVoltageMillivolts ?? 0) / 1000)}–${String((dc.maximumMpptVoltageMillivolts ?? 0) / 1000)} V`,
    ],
    [
      t("features.equipmentCatalog.fields.dcCurrent"),
      `${String((dc.maximumInputCurrentMilliamperes ?? 0) / 1000)} A`,
    ],
    [
      t("features.equipmentCatalog.fields.acPower"),
      `${String(ac.ratedActivePowerWatts)} W`,
    ],
    [t("features.equipmentCatalog.fields.phases"), String(ac.phaseCount)],
    [
      t("features.equipmentCatalog.fields.communications"),
      operational.communicationInterfaces.join(", ") || MISSING,
    ],
    ...inverterOperationalRows(entry, t),
    [t("features.equipmentCatalog.fields.source"), provenance.sourceName],
  ];
}

/** Builds the technical review rows for a selected solar-module catalog entry, including the string totals derived from the editable composition. @param entry - Selected module entry. @param moduleCount - Modules configured for the string. @param peakPowerWatts - Peak power currently configured per module. @param t - Active translator. @returns Localized label/value rows. */
export function solarModuleTechnicalRows(
  entry: SolarModuleCatalogEntry,
  moduleCount: number,
  peakPowerWatts: number,
  t: TFunction,
): TechnicalDataRow[] {
  const { specification } = entry;
  const dimensions = specification.dimensionsMillimetres;
  const frontLoad = specification.maximumFrontStaticLoadPascals;
  const rearLoad = specification.maximumRearStaticLoadPascals;
  return [
    [t("features.equipmentCatalog.fields.revision"), entry.revision],
    [
      t("features.equipmentCatalog.fields.cellTechnology"),
      specification.cellDescription ?? specification.cellTechnology,
    ],
    [t("features.equipmentCatalog.fields.pmax"), `${String(peakPowerWatts)} W`],
    [
      t("features.equipmentCatalog.fields.stringTotal"),
      `${String(moduleCount * peakPowerWatts)} W`,
    ],
    [
      t("features.equipmentCatalog.fields.voc"),
      `${String(specification.openCircuitVoltageMillivolts / 1000)} V`,
    ],
    [
      t("features.equipmentCatalog.fields.vmp"),
      `${String(specification.maximumPowerVoltageMillivolts / 1000)} V`,
    ],
    [
      t("features.equipmentCatalog.fields.isc"),
      `${String(specification.shortCircuitCurrentMilliamperes / 1000)} A`,
    ],
    [
      t("features.equipmentCatalog.fields.imp"),
      `${String(specification.maximumPowerCurrentMilliamperes / 1000)} A`,
    ],
    [
      t("features.equipmentCatalog.fields.efficiency"),
      `${String(specification.efficiencyBasisPoints / 100)} %`,
    ],
    [
      t("features.equipmentCatalog.fields.bifaciality"),
      specification.bifacial
        ? `${String((specification.bifacialityFactorBasisPoints ?? 0) / 100)} %`
        : t("features.equipmentCatalog.no"),
    ],
    [
      t("features.equipmentCatalog.fields.temperatureCoefficients"),
      `${String(specification.shortCircuitCurrentTemperatureCoefficientPpmPerCelsius)} / ${String(specification.openCircuitVoltageTemperatureCoefficientPpmPerCelsius)} / ${String(specification.peakPowerTemperatureCoefficientPpmPerCelsius)} ppm/°C`,
    ],
    [
      t("features.equipmentCatalog.fields.dimensions"),
      dimensions
        ? `${String(dimensions.length)} × ${String(dimensions.width)} × ${String(dimensions.height)} mm`
        : MISSING,
    ],
    [
      t("features.equipmentCatalog.fields.weight"),
      specification.weightGrams == null
        ? MISSING
        : `${String(specification.weightGrams / 1000)} kg`,
    ],
    [
      t("features.equipmentCatalog.fields.loads"),
      frontLoad == null || rearLoad == null
        ? MISSING
        : `${String(frontLoad)} / ${String(rearLoad)} Pa`,
    ],
    [t("features.equipmentCatalog.fields.source"), entry.provenance.sourceName],
  ];
}
