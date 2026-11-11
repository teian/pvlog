import { z } from "zod";

/** One effective-dated price for imported or exported energy. */
export const tariffSchema = z.object({
  id: z.uuid(),
  name: z.string(),
  direction: z.enum(["import", "export"]),
  currencyCode: z.string().length(3),
  minorUnitsPerKwh: z.number().int(),
  effectiveFromEpochMillis: z.number().int(),
  effectiveToEpochMillis: z.number().int().nullable().default(null),
});

export type Tariff = z.infer<typeof tariffSchema>;

/**
 * Values of a tariff as the API stores them.
 *
 * @property minorUnitsPerKwh - Price per kilowatt hour in the currency's minor unit, such as cents.
 * @property effectiveToEpochMillis - End of validity; null while the tariff applies indefinitely.
 */
export type TariffInput = Omit<Tariff, "id">;
