import { z } from "zod";

const id = z.uuid();

/** One address suggestion from the server-side location search. */
export const geocodingResultSchema = z.object({
  displayName: z.string().min(1),
  latitude: z.number().min(-90).max(90),
  longitude: z.number().min(-180).max(180),
  countryCode: z.string().length(2).optional(),
  attribution: z.string().min(1),
});

/** A PV system as the management API returns it. */
export const systemRecordSchema = z.object({
  id,
  accountId: id,
  name: z.string(),
  description: z.string().default(""),
  commissioningDate: z.string().nullable().default(null),
  timezone: z.string(),
  locationLabel: z.string().nullable().default(null),
  countryCode: z.string().nullable().default(null),
  latitudeMicrodegrees: z.number().int().nullable().default(null),
  longitudeMicrodegrees: z.number().int().nullable().default(null),
  statusIntervalSeconds: z.number().int().positive().default(300),
  declaredCapacityWatts: z.number().int().nullable().default(null),
  derivedCapacityWatts: z.number().int().nullable().default(null),
  capacityWatts: z.number().int().nullable().default(null),
  capacitySource: z.enum(["derived", "declared"]).nullable().default(null),
  visibility: z.enum(["private", "account", "unlisted", "public"]),
  lifecycle: z.enum(["active", "archived", "pending_deletion"]),
  version: z.number().int().positive(),
  createdAt: z.number().int(),
  updatedAt: z.number().int(),
});

/** Whether a system has data at all, and whether it is current. */
export const dataStateSchema = z.enum(["none", "current", "stale"]);

/** One step of setting a system up, with why it is still open. */
export const setupItemSchema = z.object({
  key: z.enum([
    "location",
    "capacity",
    "data_source",
    "first_data",
    "channels",
    "equipment",
    "tariff",
    "forecast",
  ]),
  state: z.enum(["done", "recommended", "optional", "blocked"]),
  reason: z.string().optional(),
  count: z.number().int().nonnegative().optional(),
});

/** Ordered setup status of one system. */
export const setupStatusSchema = z.object({
  systemId: id,
  items: z.array(setupItemSchema),
  dataState: dataStateSchema,
  lastReceivedAtEpochMillis: z.number().int().nullable().default(null),
});

/** A reading as the first-data check shows it. */
export const verifiedReadingSchema = z.object({
  id,
  measuredAtEpochMillis: z.number().int(),
  receivedAtEpochMillis: z.number().int(),
  generationPowerWatts: z.number().int().nullable().default(null),
  generationEnergyWh: z.number().int().nullable().default(null),
  consumptionPowerWatts: z.number().int().nullable().default(null),
  consumptionEnergyWh: z.number().int().nullable().default(null),
  dataSourceId: id.nullable().default(null),
  dataSourceName: z.string().nullable().default(null),
  test: z.boolean(),
});

/** Whether a system has received real data, with the newest reading. */
export const firstDataSchema = z.object({
  systemId: id,
  received: z.boolean(),
  reading: verifiedReadingSchema.nullable().default(null),
});

export type GeocodingResult = z.infer<typeof geocodingResultSchema>;
export type SystemRecord = z.infer<typeof systemRecordSchema>;
export type DataState = z.infer<typeof dataStateSchema>;
export type SetupItem = z.infer<typeof setupItemSchema>;
export type SetupStatus = z.infer<typeof setupStatusSchema>;
export type VerifiedReading = z.infer<typeof verifiedReadingSchema>;
export type FirstData = z.infer<typeof firstDataSchema>;

/**
 * Optional system fields shared by creation and update.
 *
 * @property statusIntervalSeconds - Omitted on update to keep the current interval.
 */
export interface SystemDetailsInput {
  description?: string;
  commissioningDate?: string | null;
  locationLabel?: string | null;
  countryCode?: string | null;
  latitudeMicrodegrees?: number | null;
  longitudeMicrodegrees?: number | null;
  statusIntervalSeconds?: number | null;
  declaredCapacityWatts?: number | null;
}

/**
 * Values for a new system; only the name is required.
 *
 * @property timezone - Chosen explicitly; omitted to derive it from the location.
 * @property clientTimezone - Timezone of the browser, used when nothing else decides.
 */
export interface CreateSystemInput extends SystemDetailsInput {
  name: string;
  timezone?: string | null;
  clientTimezone?: string | null;
}

/** Complete replacement of a system's editable fields. */
export interface UpdateSystemInput extends SystemDetailsInput {
  name: string;
  timezone: string;
  visibility: SystemRecord["visibility"];
}
