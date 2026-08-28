import type { VersionedForecastSettings } from "@/features/forecasting/api/forecastApi";
import { z } from "zod";

/** Shape of the forecast settings editor, before coercion. */
export const forecastSettingsFormSchema = z.object({
  effectiveFrom: z.coerce.number().int(),
  effectiveTo: z.union([z.literal(""), z.coerce.number().int()]),
  modelIdentifier: z.string().min(1).max(64),
  modelRevision: z.coerce.number().int().min(1).max(65_535),
  soilingBasisPoints: z.coerce.number().int().min(0).max(10_000),
  shadingBasisPoints: z.coerce.number().int().min(0).max(10_000),
  mismatchBasisPoints: z.coerce.number().int().min(0).max(10_000),
  wiringBasisPoints: z.coerce.number().int().min(0).max(10_000),
  unavailabilityBasisPoints: z.coerce.number().int().min(0).max(10_000),
  calibrationBasisPoints: z.coerce.number().int().min(-5000).max(5000),
});

/** Raw values held by the forecast settings editor. */
export type ForecastSettingsFormValues = z.input<
  typeof forecastSettingsFormSchema
>;

/** Loss and calibration fields rendered as a uniform numeric grid. */
export const FORECAST_LOSS_FIELDS = [
  "soilingBasisPoints",
  "shadingBasisPoints",
  "mismatchBasisPoints",
  "wiringBasisPoints",
  "unavailabilityBasisPoints",
  "calibrationBasisPoints",
] as const;

/** Values shown before any settings exist for the scope. */
function blankSettings(): ForecastSettingsFormValues {
  return {
    effectiveFrom: Date.now(),
    effectiveTo: "",
    modelIdentifier: "pvwatts-compatible",
    modelRevision: 1,
    soilingBasisPoints: 0,
    shadingBasisPoints: 0,
    mismatchBasisPoints: 0,
    wiringBasisPoints: 0,
    unavailabilityBasisPoints: 0,
    calibrationBasisPoints: 0,
  };
}

/** Maps loaded settings onto editor values, falling back to a blank effective-now draft. @param versioned - Loaded settings and ETag, if any. @returns Default values for the settings editor. */
export function forecastSettingsDefaults(
  versioned?: VersionedForecastSettings,
): ForecastSettingsFormValues {
  const settings = versioned?.settings;
  if (settings === undefined) return blankSettings();
  const { losses } = settings;
  return {
    effectiveFrom: settings.effectiveFrom,
    effectiveTo: settings.effectiveTo ?? "",
    modelIdentifier: settings.modelIdentifier,
    modelRevision: settings.modelRevision,
    soilingBasisPoints: losses.soilingBasisPoints,
    shadingBasisPoints: losses.shadingBasisPoints,
    mismatchBasisPoints: losses.mismatchBasisPoints,
    wiringBasisPoints: losses.wiringBasisPoints,
    unavailabilityBasisPoints: losses.unavailabilityBasisPoints,
    calibrationBasisPoints: settings.calibrationBasisPoints,
  };
}
