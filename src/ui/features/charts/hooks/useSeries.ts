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
  /** Shifts the queried window this far into the past; 0 means ending now. */
  endOffsetMillis?: number;
  /** Requested resolution; the server may return a coarser one. */
  resolution: ResolutionParam;
  /** IANA timezone used for calendar bucket boundaries. */ timezone: string;
  /** Hard per-series point budget. */ maximumPoints: number;
  /** Whether the query should run; defaults to true. */ enabled?: boolean;
}

/** Fetches a bounded, resolution-aware time series ending now (or offset into the past), with short-lived caching. @param params - Query field, range length, offset, resolution, timezone, and point budget. @returns The series query state. */
export function useSeries(params: UseSeriesParams) {
  const endOffsetMillis = params.endOffsetMillis ?? 0;
  return useQuery({
    queryKey: [
      "series",
      params.systemId,
      params.field,
      params.durationMillis,
      endOffsetMillis,
      params.resolution,
      params.timezone,
      params.maximumPoints,
    ],
    queryFn: ({ signal }) => {
      const endEpochMillis = Date.now() - endOffsetMillis;
      return fetchSeries({
        systemId: params.systemId,
        field: params.field,
        startEpochMillis: endEpochMillis - params.durationMillis,
        endEpochMillis,
        resolution: params.resolution,
        timezone: params.timezone,
        maximumPoints: params.maximumPoints,
        signal,
      });
    },
    staleTime: 30_000,
    enabled: params.enabled ?? true,
  });
}
