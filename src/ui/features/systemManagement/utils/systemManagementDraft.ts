import type {
  ManagedSystem,
  SystemInverterDraft,
  SystemStringDraft,
  SystemWizardDraft,
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

/** Hydrates the single-page wizard from persisted system and equipment data. @param system - Existing system, or undefined for create mode. @returns Complete editable draft. */
export function systemDraft(system?: ManagedSystem): SystemWizardDraft {
  if (!system)
    return {
      name: "",
      location: "",
      timezone: Intl.DateTimeFormat().resolvedOptions().timeZone,
      active: true,
      inverters: [emptyInverter()],
    };
  return {
    name: system.record.name,
    location: "",
    timezone: system.record.timezone,
    active: system.record.lifecycle === "active",
    inverters:
      system.inverters.length > 0
        ? system.inverters.map((inverter) => ({
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
              temperatureCoefficient: -0.35,
              shading: [],
              moduleSpecificationSnapshot: string.moduleSpecificationSnapshot,
            })),
          }))
        : [emptyInverter()],
  };
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
