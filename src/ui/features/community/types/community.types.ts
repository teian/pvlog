import { z } from "zod";

/** A discovery-safe system projection returned by the community catalog. */
export const communitySystemSchema = z.object({
  systemId: z.uuid(),
  displayName: z.string(),
  countryCode: z.string().nullable(),
  locationLabel: z.string().nullable(),
  locationPrecision: z.string(),
  capacityWatts: z.number().int().nonnegative(),
  activity: z.string(),
  projectionAgeMillis: z.number().int().nonnegative(),
  projectionLagEvents: z.number().int().nonnegative(),
  stale: z.boolean(),
});

/** An authorized comparison or ladder row. */
export const comparisonEntrySchema = z.object({
  rank: z.number().int().positive(),
  systemId: z.uuid(),
  displayName: z.string(),
  totalGenerationWh: z.number().int().nonnegative(),
  normalizedGenerationWhPerKw: z.number().int().nonnegative(),
  coverageBasisPoints: z.number().int().min(0).max(10_000),
  tied: z.boolean(),
  projectionAgeMillis: z.number().int().nonnegative(),
});

/** Team created through the modern community API. */
export const teamSchema = z.object({
  id: z.uuid(),
  accountId: z.uuid(),
  name: z.string(),
  description: z.string().nullable(),
  access: z.enum(["private", "unlisted", "public"]),
  ownerUserId: z.uuid(),
  version: z.number().int().positive(),
});

/** Regional supply data with explicit provider freshness and provenance. */
export const regionalSupplySchema = z.object({
  regionKey: z.string(),
  timezone: z.string(),
  resolutionSeconds: z.number().int().positive(),
  source: z.string(),
  license: z.string(),
  lastSuccessfulAt: z.number(),
  stale: z.boolean(),
});

/** Search filters understood by the privacy-safe community catalog. */
export interface CommunitySearchFilters {
  /** Optional display-name term. */
  query?: string;
  /** Optional two-letter country code. */
  countryCode?: string;
}

export type CommunitySystem = z.infer<typeof communitySystemSchema>;
export type ComparisonEntry = z.infer<typeof comparisonEntrySchema>;
export type Team = z.infer<typeof teamSchema>;
export type RegionalSupply = z.infer<typeof regionalSupplySchema>;
