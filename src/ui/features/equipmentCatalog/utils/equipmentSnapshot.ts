import type {
  InverterCatalogEntry,
  SolarModuleCatalogEntry,
} from "@/features/equipmentCatalog/types/equipmentCatalog.types";
import type { TFunction } from "i18next";

/** Catalog entry of either equipment kind. */
type CatalogEntry = InverterCatalogEntry | SolarModuleCatalogEntry;

/** How a persisted snapshot relates to its catalog origin. */
export type ValueProvenance =
  "catalog_copied" | "catalog_customized" | "manual";

/** Builds the editable snapshot persisted alongside an equipment configuration. @param entry - Catalog entry to snapshot. @returns The serializable snapshot. */
export function snapshot(entry: CatalogEntry): Record<string, unknown> {
  const common = {
    manufacturer: entry.manufacturer,
    model: entry.model,
    template: {
      entryId: entry.id,
      revision: entry.revision,
      valueProvenance: "catalog_copied",
    },
  };
  return "dc" in entry
    ? { ...common, dc: entry.dc, ac: entry.ac, operational: entry.operational }
    : { ...common, specification: entry.specification };
}

/** Serializes a snapshot exactly as the editor presents it. */
function serialize(entry: CatalogEntry): string {
  return JSON.stringify(snapshot(entry), null, 2);
}

/** Classifies whether an edited snapshot still matches its catalog origin. */
function provenanceOf(
  entry: CatalogEntry | null,
  json: string,
): ValueProvenance {
  if (entry === null) return "manual";
  return json === serialize(entry) ? "catalog_copied" : "catalog_customized";
}

/** Parses an editor value, treating an empty editor as "no snapshot". */
function parseSnapshot(json: string): Record<string, unknown> | null {
  return json ? (JSON.parse(json) as Record<string, unknown>) : null;
}

/** Reads the module peak power from a snapshot, defaulting to 1 W so totals stay finite. */
function modulePeakPowerWatts(
  moduleValue: Record<string, unknown> | null,
): number {
  const specification = moduleValue?.specification as
    Record<string, unknown> | undefined;
  return Number(specification?.peakPowerWatts ?? 1);
}

/** Builds the single string entry describing the configured module composition. */
function stringEntry(
  input: EquipmentSaveInput,
  moduleValue: Record<string, unknown> | null,
  watts: number,
  effectiveFrom: number,
): Record<string, unknown> {
  const { count, module, moduleJson, t } = input;
  return {
    name: t("features.equipmentCatalog.defaultString"),
    panelCount: count,
    panelManufacturer:
      moduleValue?.manufacturer ??
      t("features.equipmentCatalog.customManufacturer"),
    panelModel:
      moduleValue?.model ?? t("features.equipmentCatalog.customModel"),
    valueProvenance: provenanceOf(module, moduleJson),
    moduleSpecificationSnapshot: moduleValue,
    modulePeakPowerWatts: watts,
    effectiveFrom,
  };
}

/** Editor state a save request is built from. */
export interface EquipmentSaveInput {
  /** Modules configured for the string. */ count: number;
  /** Selected inverter entry, or `null` when entered manually. */ inverter: InverterCatalogEntry | null;
  /** Current inverter snapshot editor value. */ inverterJson: string;
  /** Selected module entry, or `null` when entered manually. */ module: SolarModuleCatalogEntry | null;
  /** Current module snapshot editor value. */ moduleJson: string;
  /** Active translator, used for manual-entry fallbacks. */ t: TFunction;
}

/** Builds the equipment configuration save payload from the editor state, recording whether each snapshot was copied, customized, or entered manually. @param input - Current editor state. @returns The save payload. @throws SyntaxError - When either snapshot editor holds invalid JSON. */
export function buildEquipmentSaveRequest(
  input: EquipmentSaveInput,
): Record<string, unknown> {
  const { count, inverter, inverterJson } = input;
  const inverterValue = parseSnapshot(inverterJson);
  const moduleValue = parseSnapshot(input.moduleJson);
  const watts = modulePeakPowerWatts(moduleValue);
  const effectiveFrom = Date.now();
  return {
    name:
      typeof inverterValue?.model === "string"
        ? inverterValue.model
        : input.t("features.equipmentCatalog.customInverter"),
    manufacturer: inverterValue?.manufacturer ?? null,
    model: inverterValue?.model ?? null,
    ratedPowerWatts: inverter?.ac.ratedActivePowerWatts ?? count * watts,
    valueProvenance: provenanceOf(inverter, inverterJson),
    specificationSnapshot: inverterValue,
    effectiveFrom,
    strings: [stringEntry(input, moduleValue, watts, effectiveFrom)],
  };
}
