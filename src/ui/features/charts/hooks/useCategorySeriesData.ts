import type {
  ResolutionParam,
  SeriesField,
} from "@/features/charts/api/chartsApi";
import { useSeries } from "@/features/charts/hooks/useSeries";
import {
  computeSeriesSummary,
  convertSeriesValue,
} from "@/features/charts/utils/formatSeriesValue";

/** Parameters accepted by {@link useCategorySeriesData}. */
export interface UseCategorySeriesDataParams {
  /** System whose telemetry is queried. */ systemId: string;
  /** Single field to query. */ field: SeriesField;
  /** Range length ending now, in milliseconds. */ durationMillis: number;
  /** Requested resolution; the server may return a coarser one. */
  resolution: ResolutionParam;
  /** IANA timezone used for calendar bucket boundaries. */ timezone: string;
  /** Hard per-series point budget. */ maximumPoints: number;
  /** Whether the previous-period comparison query should run. */
  compareEnabled: boolean;
}

/** Fetches a category's current and (optionally) previous-period series and derives display-unit values and summary statistics. @param params - Query field, range, and comparison toggle. @returns The current/previous query states plus derived series, points, and summaries. */
export function useCategorySeriesData({
  systemId,
  field,
  durationMillis,
  resolution,
  timezone,
  maximumPoints,
  compareEnabled,
}: UseCategorySeriesDataParams) {
  const query = useSeries({
    systemId,
    durationMillis,
    field,
    resolution,
    timezone,
    maximumPoints,
  });
  const previousQuery = useSeries({
    systemId,
    durationMillis,
    endOffsetMillis: durationMillis,
    field,
    resolution,
    timezone,
    maximumPoints,
    enabled: compareEnabled,
  });
  const series = query.data?.series[0];
  const points = series?.points ?? [];
  const values = series
    ? points.map((point) => convertSeriesValue(series.unit, point.value))
    : [];
  const summary = computeSeriesSummary(values);
  const previousSeries = previousQuery.data?.series[0];
  const previousValues = previousSeries
    ? previousSeries.points.map((point) =>
        convertSeriesValue(previousSeries.unit, point.value),
      )
    : [];
  const previousSummary = computeSeriesSummary(previousValues);

  return {
    query,
    series,
    points,
    values,
    summary,
    previousQuery,
    previousSummary,
  };
}
