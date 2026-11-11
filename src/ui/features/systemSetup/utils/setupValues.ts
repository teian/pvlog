import type { CreateSystemInput, GeocodingResult } from "@/entities/system";

/** Values of the first setup step as the user types them. */
export interface SetupSystemDraft {
  name: string;
  location: string;
  /** Place chosen from the location search, when the text still matches it. */
  place: GeocodingResult | undefined;
  /** Installed capacity in kilowatts peak, as typed. */
  capacityKilowatts: string;
  /** Explicit timezone; empty to derive it from the location. */
  timezone: string;
  /** Seconds between readings, as typed; empty for the default. */
  statusIntervalSeconds: string;
  commissioningDate: string;
  description: string;
}

/** Creates the empty draft of the first setup step. @returns A draft in which only the name still has to be entered. */
export function emptySetupSystemDraft(): SetupSystemDraft {
  return {
    name: "",
    location: "",
    place: undefined,
    capacityKilowatts: "",
    timezone: "",
    statusIntervalSeconds: "",
    commissioningDate: "",
    description: "",
  };
}

/** Parses a typed capacity in kilowatts peak. @param value - Text with a dot or comma as decimal separator. @returns Whole watts, null for an empty field, or NaN for an unusable value. */
export function capacityWatts(value: string): number | null {
  const text = value.trim().replace(",", ".");
  if (text === "") return null;
  const kilowatts = Number(text);
  return Number.isFinite(kilowatts) && kilowatts > 0
    ? Math.round(kilowatts * 1_000)
    : Number.NaN;
}

/** Names the fields of a draft that cannot be submitted as they are. @param draft - Draft to check. @returns Keys of the invalid fields; empty when the draft can be submitted. */
export function setupSystemIssues(
  draft: SetupSystemDraft,
): ("name" | "capacity" | "statusInterval")[] {
  const issues: ("name" | "capacity" | "statusInterval")[] = [];
  if (draft.name.trim() === "") issues.push("name");
  if (Number.isNaN(capacityWatts(draft.capacityKilowatts)))
    issues.push("capacity");
  const interval = draft.statusIntervalSeconds.trim();
  if (
    interval !== "" &&
    !(
      Number.isInteger(Number(interval)) &&
      Number(interval) >= 30 &&
      Number(interval) <= 86_400
    )
  )
    issues.push("statusInterval");
  return issues;
}

/** Converts a valid draft into the creation request. @param draft - Draft without issues. @param clientTimezone - Timezone of the browser. @returns Request in which every field the user left empty is omitted. */
export function createSystemInput(
  draft: SetupSystemDraft,
  clientTimezone: string,
): CreateSystemInput {
  const place = draft.place;
  const interval = draft.statusIntervalSeconds.trim();
  return {
    name: draft.name.trim(),
    clientTimezone,
    ...(draft.timezone.trim() ? { timezone: draft.timezone.trim() } : {}),
    ...(draft.description.trim()
      ? { description: draft.description.trim() }
      : {}),
    ...(draft.commissioningDate
      ? { commissioningDate: draft.commissioningDate }
      : {}),
    ...(place
      ? {
          locationLabel: place.displayName,
          latitudeMicrodegrees: Math.round(place.latitude * 1_000_000),
          longitudeMicrodegrees: Math.round(place.longitude * 1_000_000),
          ...(place.countryCode ? { countryCode: place.countryCode } : {}),
        }
      : {}),
    ...(interval ? { statusIntervalSeconds: Number(interval) } : {}),
    ...(capacityWatts(draft.capacityKilowatts) === null
      ? {}
      : { declaredCapacityWatts: capacityWatts(draft.capacityKilowatts) }),
  };
}
