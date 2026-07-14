import { z } from "zod";

/** Stable reasons explaining why modeled yield is partial or unavailable. */
export const forecastCompletenessReasonSchema = z.enum([
  "missing_system_location",
  "missing_module_identity",
  "missing_orientation",
  "missing_tilt",
  "missing_module_capacity",
  "missing_module_specification",
  "missing_forecast_settings",
  "missing_weather_input",
  "unsupported_weather_input",
  "incompatible_input_run",
  "partial_effective_capacity",
  "insufficient_weather_coverage",
  "insufficient_actual_coverage",
  "missing_actual_telemetry",
  "non_positive_expected_energy",
  "no_effective_equipment",
]);

/** Stable reason explaining why modeled yield is partial or unavailable. */
export type ForecastCompletenessReason = z.infer<
  typeof forecastCompletenessReasonSchema
>;

const forecastScopeSchema = z.discriminatedUnion("kind", [
  z.object({ kind: z.literal("account"), account_id: z.string() }),
  z.object({
    kind: z.literal("system"),
    account_id: z.string(),
    system_id: z.string(),
  }),
  z.object({
    kind: z.literal("inverter"),
    account_id: z.string(),
    system_id: z.string(),
    inverter_id: z.string(),
  }),
  z.object({
    kind: z.literal("string"),
    account_id: z.string(),
    system_id: z.string(),
    inverter_id: z.string(),
    string_id: z.string(),
  }),
]);

const lossesSchema = z.object({
  soilingBasisPoints: z.number().int().min(0).max(10_000),
  shadingBasisPoints: z.number().int().min(0).max(10_000),
  mismatchBasisPoints: z.number().int().min(0).max(10_000),
  wiringBasisPoints: z.number().int().min(0).max(10_000),
  unavailabilityBasisPoints: z.number().int().min(0).max(10_000),
});

/** Effective-dated loss and calibration input accepted by the API. */
export const forecastSettingsInputSchema = z.object({
  effectiveFrom: z.number().int(),
  effectiveTo: z.number().int().nullable(),
  modelIdentifier: z.string().min(1).max(64),
  modelRevision: z.number().int().min(1).max(65_535),
  losses: lossesSchema,
  calibrationBasisPoints: z.number().int().min(-5000).max(5000),
});

/** Effective-dated loss and calibration input accepted by the API. */
export type ForecastSettingsInput = z.infer<typeof forecastSettingsInputSchema>;

/** Effective settings returned for a forecast scope. */
export const forecastSettingsSchema = forecastSettingsInputSchema.extend({
  scope: forecastScopeSchema,
  version: z.number().int().min(1),
});

/** Effective settings returned for a forecast scope. */
export type ForecastSettings = z.infer<typeof forecastSettingsSchema>;

/** Forecast-input readiness and included nameplate capacity. */
export const forecastInputCompletenessSchema = z.object({
  scope: forecastScopeSchema,
  effectiveAt: z.number().int(),
  includedCapacityWatts: z.number().int().min(0),
  totalEffectiveCapacityWatts: z.number().int().min(0),
  complete: z.boolean(),
  reasons: z.array(forecastCompletenessReasonSchema),
  version: z.number().int().min(1),
});

/** Forecast-input readiness and included nameplate capacity. */
export type ForecastInputCompleteness = z.infer<
  typeof forecastInputCompletenessSchema
>;

const provenanceSchema = z.object({
  providerId: z.string().min(1),
  adapter: z.string().min(1),
  sourceUrl: z.url(),
  licenseIdentifier: z.string().min(1),
  attribution: z.string().min(1),
  fetchedAt: z.number().int(),
});

const completenessSchema = z.union([
  z.literal("complete"),
  z.object({
    partial: z.object({ reasons: z.array(forecastCompletenessReasonSchema) }),
  }),
  z.object({
    unavailable: z.object({
      reasons: z.array(forecastCompletenessReasonSchema),
    }),
  }),
]);

const yieldPointSchema = z.object({
  intervalStart: z.number().int(),
  intervalEnd: z.number().int(),
  centralPowerWatts: z.number().int().nullable(),
  lowerPowerWatts: z.number().int().nullable(),
  upperPowerWatts: z.number().int().nullable(),
  centralEnergyWattHours: z.number().int().nullable(),
  lowerEnergyWattHours: z.number().int().nullable(),
  upperEnergyWattHours: z.number().int().nullable(),
  coverageBasisPoints: z.number().int().min(0).max(10_000),
  completeness: completenessSchema,
});

/** Modeled forecast or historical expected-generation series. */
export const yieldSeriesSchema = z.object({
  scope: forecastScopeSchema,
  basis: z.enum(["forecast", "expected"]),
  resolution: z.enum(["fifteen_minutes", "hour", "day"]),
  issueTime: z.number().int().nullable(),
  weatherRunId: z.string(),
  calculationRunId: z.string(),
  modelIdentifier: z.string(),
  modelRevision: z.number().int().min(1),
  configurationDigest: z.string().regex(/^[0-9a-f]{64}$/u),
  freshness: z.enum(["fresh", "stale", "unavailable"]),
  provenance: provenanceSchema,
  includedCapacityWatts: z.number().int().min(0),
  totalEffectiveCapacityWatts: z.number().int().min(0),
  completeness: completenessSchema,
  unavailableReasons: z.array(forecastCompletenessReasonSchema),
  points: z.array(yieldPointSchema).max(10_000),
});

/** Modeled forecast or historical expected-generation series. */
export type YieldSeries = z.infer<typeof yieldSeriesSchema>;

const performancePointSchema = z.object({
  intervalStart: z.number().int(),
  intervalEnd: z.number().int(),
  actualEnergyWattHours: z.number().int().nullable(),
  modeledEnergyWattHours: z.number().int().nullable(),
  ratioBasisPoints: z.number().int().min(0).nullable(),
  actualCoverageBasisPoints: z.number().int().min(0).max(10_000),
  modeledCoverageBasisPoints: z.number().int().min(0).max(10_000),
  unavailableReason: forecastCompletenessReasonSchema.nullable(),
});

/** Actual generation aligned with expected or issued-forecast energy. */
export const performanceSeriesSchema = z.object({
  scope: forecastScopeSchema,
  metric: z.enum(["generation_performance", "forecast_realization"]),
  basis: z.enum(["expected", "forecast"]),
  resolution: z.enum(["fifteen_minutes", "hour", "day"]),
  issueTime: z.number().int().nullable(),
  weatherRunId: z.string(),
  calculationRunId: z.string(),
  modelIdentifier: z.string(),
  modelRevision: z.number().int().min(1),
  configurationDigest: z.string().regex(/^[0-9a-f]{64}$/u),
  freshness: z.enum(["fresh", "stale", "unavailable"]),
  provenance: provenanceSchema,
  points: z.array(performancePointSchema).max(10_000),
});

/** Actual generation aligned with expected or issued-forecast energy. */
export type PerformanceSeries = z.infer<typeof performanceSeriesSchema>;

/** Common bounded range parameters for modeled queries. */
export interface ForecastRange {
  /** Inclusive range start. */ startEpochMillis: number;
  /** Exclusive range end. */ endEpochMillis: number;
  /** Requested result buckets. */ resolution: "15m" | "hour" | "day";
  /** Maximum returned points. */ maximumPoints: number;
}
