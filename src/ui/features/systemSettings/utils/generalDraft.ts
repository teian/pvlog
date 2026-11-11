import type {
  GeocodingResult,
  SystemRecord,
  UpdateSystemInput,
} from "@/entities/system";

/** Editable general settings of a system as the user types them. */
export interface GeneralDraft {
  name: string;
  description: string;
  /** Text of the location field. */
  location: string;
  /** Newly chosen place; undefined keeps or clears the stored one. */
  place: GeocodingResult | undefined;
  timezone: string;
  statusIntervalSeconds: string;
  commissioningDate: string;
  /** Declared capacity in kilowatts peak, as typed. */
  capacityKilowatts: string;
}

/** Field of the general settings that cannot be saved as entered. */
export type GeneralIssue = "name" | "timezone" | "statusInterval" | "capacity";

/** Builds the editable draft from a stored system. @param record - System as last loaded. @returns Draft that shows every stored value. */
export function generalDraft(record: SystemRecord): GeneralDraft {
  return {
    name: record.name,
    description: record.description,
    location: record.locationLabel ?? "",
    place: undefined,
    timezone: record.timezone,
    statusIntervalSeconds: String(record.statusIntervalSeconds),
    commissioningDate: record.commissioningDate ?? "",
    capacityKilowatts:
      record.declaredCapacityWatts === null
        ? ""
        : String(record.declaredCapacityWatts / 1_000),
  };
}

function watts(value: string): number | null {
  const text = value.trim().replace(",", ".");
  if (text === "") return null;
  const kilowatts = Number(text);
  return Number.isFinite(kilowatts) && kilowatts > 0
    ? Math.round(kilowatts * 1_000)
    : Number.NaN;
}

/** Names the fields of a draft that cannot be saved. @param draft - Draft to check. @returns Invalid fields; empty when the draft can be saved. */
export function generalIssues(draft: GeneralDraft): GeneralIssue[] {
  const issues: GeneralIssue[] = [];
  if (draft.name.trim() === "") issues.push("name");
  if (draft.timezone.trim() === "") issues.push("timezone");
  const interval = Number(draft.statusIntervalSeconds.trim());
  if (!Number.isInteger(interval) || interval < 30 || interval > 86_400)
    issues.push("statusInterval");
  if (Number.isNaN(watts(draft.capacityKilowatts))) issues.push("capacity");
  return issues;
}

/**
 * Converts a valid draft into the complete update request.
 *
 * The stored location is kept while its text is unchanged, replaced when a
 * place was chosen, and removed when the text was cleared or edited away.
 *
 * @param draft - Draft without issues.
 * @param record - System the draft was built from.
 * @returns Every editable field of the system.
 */
export function updateSystemInput(
  draft: GeneralDraft,
  record: SystemRecord,
): UpdateSystemInput {
  const place = draft.place;
  const unchanged =
    place === undefined && draft.location === (record.locationLabel ?? "");
  const location = place
    ? {
        locationLabel: place.displayName,
        countryCode: place.countryCode ?? null,
        latitudeMicrodegrees: Math.round(place.latitude * 1_000_000),
        longitudeMicrodegrees: Math.round(place.longitude * 1_000_000),
      }
    : unchanged
      ? {
          locationLabel: record.locationLabel,
          countryCode: record.countryCode,
          latitudeMicrodegrees: record.latitudeMicrodegrees,
          longitudeMicrodegrees: record.longitudeMicrodegrees,
        }
      : {
          locationLabel: null,
          countryCode: null,
          latitudeMicrodegrees: null,
          longitudeMicrodegrees: null,
        };
  return {
    name: draft.name.trim(),
    timezone: draft.timezone.trim(),
    visibility: record.visibility,
    description: draft.description.trim(),
    commissioningDate: draft.commissioningDate || null,
    statusIntervalSeconds: Number(draft.statusIntervalSeconds.trim()),
    declaredCapacityWatts: watts(draft.capacityKilowatts),
    ...location,
  };
}
