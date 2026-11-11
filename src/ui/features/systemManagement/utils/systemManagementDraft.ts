import type {
  ManagedInverter,
  SystemInverterDraft,
  SystemStringDraft,
} from "@/features/systemManagement/types/systemManagement.types";

/** Creates a new empty PV string draft. @param index - One-based display position. @returns Valid editable defaults. */
export function emptyString(index = 1): SystemStringDraft {
  return {
    effectiveFrom: Date.now(),
    effectiveTo: null,
    name: `STR-${String(index)}`,
    panelCount: 1,
    panelManufacturer: "",
    panelModel: "",
    modulePeakPowerWatts: 400,
    orientationDegrees: 180,
    tiltDegrees: 30,
    temperatureCoefficient: -0.35,
    shading: [],
    moduleSpecificationSnapshot: null,
  };
}

/** Creates a new empty inverter draft. @param index - One-based display position. @returns Valid editable defaults. */
export function emptyInverter(index = 1): SystemInverterDraft {
  return {
    effectiveFrom: Date.now(),
    effectiveTo: null,
    name: `INV-${String(index)}`,
    manufacturer: "",
    model: "",
    ratedPowerWatts: 0,
    specificationSnapshot: null,
    strings: [emptyString(1)],
  };
}

const minutesToTime = (minutes: number) =>
  `${String(Math.floor(minutes / 60)).padStart(2, "0")}:${String(minutes % 60).padStart(2, "0")}`;

/** Parses a time of day. @param time - Text in the form HH:MM. @returns Minutes after midnight, or NaN for unusable text. */
export function timeToMinutes(time: string): number {
  const match = /^(\d{1,2}):(\d{2})$/u.exec(time.trim());
  if (!match) return Number.NaN;
  const minutes = Number(match[1]) * 60 + Number(match[2]);
  return minutes <= 1_440 ? minutes : Number.NaN;
}

/** Builds editable drafts from the stored inverters of a system. @param inverters - Inverters as last loaded. @returns One draft per inverter that carries the identifiers of its inverter and strings. */
export function equipmentDrafts(
  inverters: ManagedInverter[],
): SystemInverterDraft[] {
  return inverters.map((inverter) => ({
    id: inverter.id,
    effectiveFrom: inverter.effectiveFrom,
    effectiveTo: inverter.effectiveTo,
    name: inverter.name,
    manufacturer: inverter.manufacturer ?? "",
    model: inverter.model ?? "",
    ratedPowerWatts: inverter.ratedPowerWatts ?? 0,
    specificationSnapshot: inverter.specificationSnapshot,
    strings: inverter.strings.map((string) => ({
      id: string.id,
      effectiveFrom: string.effectiveFrom,
      effectiveTo: string.effectiveTo,
      name: string.name,
      panelCount: string.panelCount,
      panelManufacturer: string.panelManufacturer ?? "",
      panelModel: string.panelModel ?? "",
      modulePeakPowerWatts:
        string.modulePeakPowerWatts ??
        Math.round(string.ratedPowerWatts / string.panelCount),
      orientationDegrees: string.orientationDegrees ?? 180,
      tiltDegrees: string.tiltDegrees ?? 30,
      temperatureCoefficient:
        string.temperatureCoefficientPpmPerCelsius === null
          ? -0.35
          : string.temperatureCoefficientPpmPerCelsius / 10_000,
      shading: string.shading.map((entry, index) => ({
        id: `${string.id}-${String(index)}`,
        from: minutesToTime(entry.fromMinuteOfDay),
        to: minutesToTime(entry.toMinuteOfDay),
        label: entry.label,
        degree: entry.lossBasisPoints / 100,
      })),
      moduleSpecificationSnapshot: string.moduleSpecificationSnapshot,
    })),
  }));
}

/** First reason why equipment drafts cannot be saved. */
export type EquipmentIssue =
  | { kind: "inverterName"; inverter: number }
  | { kind: "strings"; inverter: number }
  | {
      kind: "stringName" | "moduleCount" | "modulePower" | "shading";
      inverter: number;
      string: number;
    };

/** Finds the first reason why equipment drafts cannot be saved. @param drafts - Inverter drafts to check. @returns The first issue with the one-based position it concerns, or null when the drafts can be saved. */
export function equipmentIssue(
  drafts: SystemInverterDraft[],
): EquipmentIssue | null {
  for (const [inverterIndex, inverter] of drafts.entries()) {
    const position = inverterIndex + 1;
    if (!inverter.name.trim())
      return { kind: "inverterName", inverter: position };
    if (inverter.strings.length === 0)
      return { kind: "strings", inverter: position };
    for (const [stringIndex, string] of inverter.strings.entries()) {
      const context = { inverter: position, string: stringIndex + 1 };
      if (!string.name.trim()) return { kind: "stringName", ...context };
      if (string.panelCount <= 0) return { kind: "moduleCount", ...context };
      if (string.modulePeakPowerWatts <= 0)
        return { kind: "modulePower", ...context };
      const shadingValid = string.shading.every((entry) => {
        const from = timeToMinutes(entry.from);
        const to = timeToMinutes(entry.to);
        return from < to && entry.degree >= 0 && entry.degree <= 100;
      });
      if (!shadingValid) return { kind: "shading", ...context };
    }
  }
  return null;
}

/** Formats a compass direction from an azimuth for compact management cards. @param degrees - Azimuth degrees. @returns Localizable direction key suffix. */
export function orientationKey(degrees: number | null): string {
  if (degrees === null) return "unknown";
  const directions = [
    "north",
    "northEast",
    "east",
    "southEast",
    "south",
    "southWest",
    "west",
    "northWest",
  ];
  return directions[Math.round(degrees / 45) % directions.length] ?? "unknown";
}
