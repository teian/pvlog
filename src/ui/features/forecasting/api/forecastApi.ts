import {
  forecastInputCompletenessSchema,
  forecastSettingsSchema,
  performanceSeriesSchema,
  type ForecastRange,
  type ForecastSettings,
  type ForecastSettingsInput,
  type PerformanceSeries,
  type YieldSeries,
  yieldSeriesSchema,
} from "@/features/forecasting/types/forecast.types";
import { sessionJsonRequest } from "@/shared/api/sessionRequest";

const systemPath = (accountId: string, systemId: string) =>
  `/api/v1/accounts/${accountId}/systems/${systemId}`;

/** Settings together with the validator required for safe updates. */
export interface VersionedForecastSettings {
  /** Effective settings. */ settings: ForecastSettings;
  /** HTTP entity validator. */ etag: string;
}

/** Loads effective system forecast settings and their ETag. @param accountId - Owning account. @param systemId - Target system. @returns Validated effective settings and validator. */
export async function fetchForecastSettings(
  accountId: string,
  systemId: string,
): Promise<VersionedForecastSettings> {
  const response = await fetch(
    `${systemPath(accountId, systemId)}/forecast-settings`,
    { credentials: "same-origin" },
  );
  if (!response.ok)
    throw new Error(`forecast_settings_failed:${String(response.status)}`);
  return {
    settings: forecastSettingsSchema.parse(await response.json()),
    etag: response.headers.get("etag") ?? "*",
  };
}

/** Replaces effective system forecast settings using optimistic concurrency. @param accountId - Owning account. @param systemId - Target system. @param input - Effective-dated settings. @param etag - Previously read validator. @returns Validated updated settings. */
export async function updateForecastSettings(
  accountId: string,
  systemId: string,
  input: ForecastSettingsInput,
  etag: string,
): Promise<ForecastSettings> {
  return forecastSettingsSchema.parse(
    await sessionJsonRequest(
      `${systemPath(accountId, systemId)}/forecast-settings`,
      {
        method: "PUT",
        headers: { "if-match": etag },
        body: JSON.stringify(input),
      },
    ),
  );
}

/** Loads effective forecast-input readiness for a system. @param accountId - Owning account. @param systemId - Target system. @returns Validated completeness details. */
export async function fetchForecastCompleteness(
  accountId: string,
  systemId: string,
) {
  return forecastInputCompletenessSchema.parse(
    await sessionJsonRequest(
      `${systemPath(accountId, systemId)}/forecast-input-completeness`,
    ),
  );
}

function rangeQuery(range: ForecastRange): URLSearchParams {
  return new URLSearchParams({
    startEpochMillis: String(range.startEpochMillis),
    endEpochMillis: String(range.endEpochMillis),
    resolution: range.resolution,
    maximumPoints: String(range.maximumPoints),
  });
}

/** Loads a bounded modeled yield series without conflating it with telemetry. @param accountId - Owning account. @param systemId - Target system. @param range - Query range. @param basis - Forecast or historical expectation. @returns Validated modeled series. */
export async function fetchYieldSeries(
  accountId: string,
  systemId: string,
  range: ForecastRange,
  basis: "forecast" | "expected",
): Promise<YieldSeries> {
  const query = rangeQuery(range);
  query.set("basis", basis);
  query.set("includePartial", "true");
  return yieldSeriesSchema.parse(
    await sessionJsonRequest(
      `${systemPath(accountId, systemId)}/yield-series?${query.toString()}`,
    ),
  );
}

/** Loads actual generation aligned with modeled energy. @param accountId - Owning account. @param systemId - Target system. @param range - Query range. @param metric - Expected performance or issued-forecast realization. @returns Validated measured-versus-modeled series. */
export async function fetchPerformanceSeries(
  accountId: string,
  systemId: string,
  range: ForecastRange,
  metric: "generation_performance" | "forecast_realization",
): Promise<PerformanceSeries> {
  const query = rangeQuery(range);
  query.set("metric", metric);
  return performanceSeriesSchema.parse(
    await sessionJsonRequest(
      `${systemPath(accountId, systemId)}/yield-performance?${query.toString()}`,
    ),
  );
}

/** Requests an analysis export matching a forecast or performance view. @param systemId - Target system. @param range - Export range. @param field - Modeled field. @param format - File format. @returns Downloadable response blob. */
export async function requestForecastExport(
  systemId: string,
  range: ForecastRange,
  field:
    | "forecast_power"
    | "expected_energy"
    | "generation_performance"
    | "forecast_realization",
  format: "csv" | "json",
): Promise<Blob> {
  const response = await fetch(`/api/v1/systems/${systemId}/analysis-exports`, {
    method: "POST",
    credentials: "same-origin",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({
      startEpochMillis: range.startEpochMillis,
      endEpochMillis: range.endEpochMillis,
      fields: [field],
      resolution: range.resolution === "15m" ? "15m" : range.resolution,
      timezone: Intl.DateTimeFormat().resolvedOptions().timeZone,
      maximumPoints: range.maximumPoints,
      format,
      asynchronous: false,
    }),
  });
  if (!response.ok)
    throw new Error(`forecast_export_failed:${String(response.status)}`);
  return response.blob();
}
