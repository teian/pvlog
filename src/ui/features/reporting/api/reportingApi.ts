import { z } from "zod";

const id = z.uuid();
const overviewSchema = z.object({
  id,
  name: z.string(),
  timezone: z.string(),
  lifecycle: z.string(),
  inverterCount: z.number().int().nonnegative(),
  stringCount: z.number().int().nonnegative(),
  capacityWatts: z.number().int().nullable(),
});
const statisticsSchema = z.object({
  systemId: id,
  generationEnergyWh: z.number().int().nullable(),
  consumptionEnergyWh: z.number().int().nullable(),
  peakGenerationPowerWatts: z.number().int().nullable(),
  firstObservationAtEpochMillis: z.number().int().nullable(),
  lastObservationAtEpochMillis: z.number().int().nullable(),
  coverageBasisPoints: z.number().int().min(0).max(10_000),
  monthly: z.array(
    z.object({
      bucketStartEpochMillis: z.number().int(),
      generationEnergyWh: z.number().int().nullable(),
      consumptionEnergyWh: z.number().int().nullable(),
      coverageBasisPoints: z.number().int().min(0).max(10_000),
    }),
  ),
});
const seasonalSchema = z.object({
  systemId: id,
  seasons: z.array(
    z.object({
      season: z.enum(["winter", "spring", "summer", "autumn"]),
      generationEnergyWh: z.number().int(),
      measuredDays: z.number().int().nonnegative(),
      averageDailyEnergyWh: z.number().int(),
    }),
  ),
});
const weatherSchema = z.object({
  systemId: id,
  issuedAtEpochMillis: z.number().int().nullable(),
  attribution: z.string().nullable(),
  points: z.array(
    z.object({
      intervalStartEpochMillis: z.number().int(),
      intervalEndEpochMillis: z.number().int(),
      irradianceWattsPerSquareMetre: z.number().int().nullable(),
      ambientTemperatureMillicelsius: z.number().int().nullable(),
      windSpeedMillimetresPerSecond: z.number().int().nullable(),
      cloudCoverBasisPoints: z.number().int().min(0).max(10_000).nullable(),
      predictedEnergyWh: z.number().int().nullable(),
    }),
  ),
});

export type SystemOverview = z.infer<typeof overviewSchema>;
export type StatisticsReport = z.infer<typeof statisticsSchema>;
export type SeasonalReport = z.infer<typeof seasonalSchema>;
export type WeatherReport = z.infer<typeof weatherSchema>;

async function get<T>(path: string, schema: z.ZodType<T>): Promise<T> {
  const response = await fetch(path, { credentials: "same-origin" });
  if (!response.ok)
    throw new Error(`reporting_failed:${String(response.status)}`);
  return schema.parse(await response.json());
}

export const fetchSystemOverview = (systemId: string) =>
  get(`/api/v1/systems/${systemId}/overview`, overviewSchema);
export const fetchStatistics = (systemId: string) =>
  get(`/api/v1/systems/${systemId}/reporting/statistics`, statisticsSchema);
export const fetchSeasonal = (systemId: string) =>
  get(`/api/v1/systems/${systemId}/seasonal`, seasonalSchema);
export const fetchWeather = (systemId: string) =>
  get(`/api/v1/systems/${systemId}/weather-forecast`, weatherSchema);
