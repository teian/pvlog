import { z } from "zod";

/** Series field identifiers accepted by the modern analytics query API. */
export const seriesFieldSchema = z.enum([
  "generation_power",
  "generation_energy",
  "consumption_power",
  "consumption_energy",
  "grid_power",
  "battery_power",
  "battery_state_of_charge",
  "temperature",
  "extended",
]);

/** A queryable time-series field. */
export type SeriesField = z.infer<typeof seriesFieldSchema>;

/** Canonical unit reported for a queried series. */
export const seriesUnitSchema = z.enum([
  "watts",
  "watt_hours",
  "basis_points",
  "milli_degrees_celsius",
  "integer",
]);

/** Canonical unit reported for a queried series. */
export type SeriesUnit = z.infer<typeof seriesUnitSchema>;

/** Resolution accepted by the series query endpoint. */
export const resolutionParamSchema = z.enum([
  "auto",
  "raw",
  "15m",
  "hour",
  "day",
  "month",
  "year",
]);

/** Resolution accepted by the series query endpoint. */
export type ResolutionParam = z.infer<typeof resolutionParamSchema>;

const seriesPointSchema = z.object({
  timestampEpochMillis: z.number().int(),
  value: z.number().int(),
  coverageBasisPoints: z.number().int().min(0).max(10_000),
  qualityFlags: z.number().int().min(0),
  provenance: z.string().nullable().optional(),
});

const seriesGapSchema = z.object({
  startEpochMillis: z.number().int(),
  endEpochMillis: z.number().int(),
  kind: z.enum(["missing", "suspect", "incomplete_coverage"]),
});

const seriesSchema = z.object({
  field: seriesFieldSchema,
  unit: seriesUnitSchema,
  points: z.array(seriesPointSchema),
  gaps: z.array(seriesGapSchema),
});

const seriesQueryResultSchema = z.object({
  actualResolution: z.enum([
    "raw",
    "fifteen_minutes",
    "hourly",
    "daily",
    "monthly",
    "yearly",
  ]),
  timezone: z.string(),
  series: z.array(seriesSchema),
});

/** Validated response from the bounded system series query endpoint. */
export type SeriesQueryResult = z.infer<typeof seriesQueryResultSchema>;

/** Parameters accepted by {@link fetchSeries}. */
export interface FetchSeriesParams {
  /** System whose telemetry is queried. */ systemId: string;
  /** Inclusive UTC range start in epoch milliseconds. */
  startEpochMillis: number;
  /** Exclusive UTC range end in epoch milliseconds. */ endEpochMillis: number;
  /** Single field to query. */ field: SeriesField;
  /** Requested resolution; the server may return a coarser one. */
  resolution: ResolutionParam;
  /** IANA timezone used for calendar bucket boundaries. */ timezone: string;
  /** Hard per-series point budget. */ maximumPoints: number;
}

/** Retrieves a bounded, resolution-aware time series for one system field. @param params - Query range, field, resolution, timezone, and point budget. @returns The validated series result. */
export async function fetchSeries(
  params: FetchSeriesParams,
): Promise<SeriesQueryResult> {
  const query = new URLSearchParams({
    startEpochMillis: String(params.startEpochMillis),
    endEpochMillis: String(params.endEpochMillis),
    fields: params.field,
    resolution: params.resolution,
    timezone: params.timezone,
    maximumPoints: String(params.maximumPoints),
  });
  const response = await fetch(
    `/api/v1/systems/${params.systemId}/series?${query.toString()}`,
    { credentials: "same-origin" },
  );
  if (!response.ok) throw new Error(`series_failed:${String(response.status)}`);
  return seriesQueryResultSchema.parse(await response.json());
}
