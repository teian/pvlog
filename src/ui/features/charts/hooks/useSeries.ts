import {
  fetchSeries,
  type ResolutionParam,
  type SeriesField,
} from "@/features/charts/api/chartsApi";
import { useQuery } from "@tanstack/react-query";

/** Parameters accepted by {@link useSeries}. */
export interface UseSeriesParams {
  /** System whose telemetry is queried. */ systemId: string;
  /** Single field to query. */ field: SeriesField;
  /** Range length ending now, in milliseconds. */ durationMillis: number;
  /** Requested resolution; the server may return a coarser one. */
  resolution: ResolutionParam;
  /** IANA timezone used for calendar bucket boundaries. */ timezone: string;
  /** Hard per-series point budget. */ maximumPoints: number;
}

/** Fetches a bounded, resolution-aware time series ending now, with short-lived caching. @param params - Query field, range length, resolution, timezone, and point budget. @returns The series query state. */
export function useSeries(params: UseSeriesParams) {
  return useQuery({
    queryKey: [
      "series",
      params.systemId,
      params.field,
      params.durationMillis,
      params.resolution,
      params.timezone,
      params.maximumPoints,
    ],
    queryFn: () => {
      const endEpochMillis = Date.now();
      return fetchSeries({
        systemId: params.systemId,
        field: params.field,
        startEpochMillis: endEpochMillis - params.durationMillis,
        endEpochMillis,
        resolution: params.resolution,
        timezone: params.timezone,
        maximumPoints: params.maximumPoints,
      });
    },
    staleTime: 30_000,
  });
}
