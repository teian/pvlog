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
  "forecast_power",
  "forecast_energy",
  "expected_energy",
  "generation_performance",
  "forecast_realization",
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

/** A point in a queried series, in canonical base units. */
export type SeriesPoint = z.infer<typeof seriesPointSchema>;

/** An interval where the requested field lacks reliable raw coverage. */
export type SeriesGap = z.infer<typeof seriesGapSchema>;

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
  /** Cancels the request when a newer query supersedes it. */
  signal?: AbortSignal;
}

/** Retrieves a bounded, resolution-aware time series for one system field. @param params - Query range, field, resolution, timezone, point budget, and optional abort signal. @returns The validated series result. */
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
    { credentials: "same-origin", signal: params.signal ?? null },
  );
  if (!response.ok) throw new Error(`series_failed:${String(response.status)}`);
  return seriesQueryResultSchema.parse(await response.json());
}

/** Parameters accepted by {@link requestAnalysisExport}. */
export interface AnalysisExportParams {
  /** System whose telemetry is exported. */ systemId: string;
  /** Inclusive UTC range start in epoch milliseconds. */
  startEpochMillis: number;
  /** Exclusive UTC range end in epoch milliseconds. */ endEpochMillis: number;
  /** Field to export. */ field: SeriesField;
  /** Requested resolution; the server may return a coarser one. */
  resolution: ResolutionParam;
  /** IANA timezone used for calendar bucket boundaries. */ timezone: string;
  /** Hard per-series point budget. */ maximumPoints: number;
  /** Export file format. */ format: "csv" | "json";
}

/** Outcome of an analysis export request: an immediate file, or a queued job for a later download. */
export type AnalysisExportResult =
  | { kind: "file"; blob: Blob; filename: string }
  | { kind: "queued"; jobId: string };

const queuedJobSchema = z.object({ jobId: z.uuid() });

function exportFilename(field: SeriesField, format: "csv" | "json"): string {
  return `${field}.${format}`;
}

/** Requests a synchronous or queued CSV/JSON export matching a chart's current query. @param params - Range, field, resolution, timezone, point budget, and format. @returns The downloadable file, or a queued job identifier. */
export async function requestAnalysisExport(
  params: AnalysisExportParams,
): Promise<AnalysisExportResult> {
  const response = await fetch(
    `/api/v1/systems/${params.systemId}/analysis-exports`,
    {
      method: "POST",
      credentials: "same-origin",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        startEpochMillis: params.startEpochMillis,
        endEpochMillis: params.endEpochMillis,
        fields: [params.field],
        resolution: params.resolution,
        timezone: params.timezone,
        maximumPoints: params.maximumPoints,
        format: params.format,
        asynchronous: false,
      }),
    },
  );
  if (response.status === 202) {
    const { jobId } = queuedJobSchema.parse(await response.json());
    return { kind: "queued", jobId };
  }
  if (!response.ok) throw new Error(`export_failed:${String(response.status)}`);
  return {
    kind: "file",
    blob: await response.blob(),
    filename: exportFilename(params.field, params.format),
  };
}
