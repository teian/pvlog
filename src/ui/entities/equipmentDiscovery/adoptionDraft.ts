import type {
  DiscoveryAdoption,
  DiscoveryProposal,
} from "@/entities/equipmentDiscovery/equipmentDiscovery.types";
import type { TFunction } from "i18next";

/**
 * One string of an adoption as the form holds it.
 *
 * @property tracker - DC input the string belongs to, or null for a device without one.
 * @property capacity - Capacity in kilowatts peak as typed.
 */
export interface StringDraft {
  tracker: number | null;
  name: string;
  capacity: string;
}

/**
 * Parses a capacity in kilowatts peak.
 *
 * @param text - Input as typed, with a comma or a point.
 * @returns The capacity in watts, or null when it is not a positive number.
 */
export function capacityWatts(text: string): number | null {
  const kilowatts = Number(text.trim().replace(",", "."));
  return text.trim() !== "" && Number.isFinite(kilowatts) && kilowatts > 0
    ? Math.round(kilowatts * 1_000)
    : null;
}

/**
 * Proposes one string per DC input that has delivered power.
 *
 * @param proposal - The reported device.
 * @param t - Translation function for the default names.
 * @returns The string rows to start the form with; a single row without a tracker when no input has delivered power.
 */
export function initialStrings(
  proposal: DiscoveryProposal,
  t: TFunction,
): StringDraft[] {
  if (proposal.trackers.length === 0)
    return [
      {
        tracker: null,
        name: t("discovery.defaultSingleStringName"),
        capacity: "",
      },
    ];
  return proposal.trackers.map((tracker) => ({
    tracker,
    name: t("discovery.defaultStringName", { number: tracker }),
    capacity: "",
  }));
}

/**
 * Turns the form's values into an adoption.
 *
 * @param name - Name of the inverter as typed.
 * @param strings - The string rows.
 * @returns The adoption, or null while a name is blank or a capacity is not a positive number.
 */
export function toAdoption(
  name: string,
  strings: StringDraft[],
): DiscoveryAdoption | null {
  if (name.trim() === "") return null;
  const adopted: DiscoveryAdoption["strings"] = [];
  for (const string of strings) {
    const watts = capacityWatts(string.capacity);
    if (string.name.trim() === "" || watts === null) return null;
    adopted.push({
      tracker: string.tracker,
      name: string.name.trim(),
      capacityWatts: watts,
    });
  }
  return { name: name.trim(), strings: adopted };
}
