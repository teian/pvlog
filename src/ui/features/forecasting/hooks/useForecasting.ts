import {
  fetchForecastCompleteness,
  fetchForecastSettings,
  fetchPerformanceSeries,
  fetchYieldSeries,
  updateForecastSettings,
} from "@/features/forecasting/api/forecastApi";
import type {
  ForecastRange,
  ForecastSettingsInput,
} from "@/features/forecasting/types/forecast.types";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

const rootKey = (accountId: string, systemId: string) => [
  "forecasting",
  accountId,
  systemId,
];

/** Loads effective settings with a short-lived cache. @param accountId - Owning account. @param systemId - Target system. @returns Query state. */
export function useForecastSettings(accountId: string, systemId: string) {
  return useQuery({
    queryKey: [...rootKey(accountId, systemId), "settings"],
    queryFn: () => fetchForecastSettings(accountId, systemId),
    staleTime: 30_000,
    enabled: Boolean(accountId && systemId),
  });
}

/** Loads effective input readiness. @param accountId - Owning account. @param systemId - Target system. @returns Query state. */
export function useForecastCompleteness(accountId: string, systemId: string) {
  return useQuery({
    queryKey: [...rootKey(accountId, systemId), "completeness"],
    queryFn: () => fetchForecastCompleteness(accountId, systemId),
    staleTime: 30_000,
    enabled: Boolean(accountId && systemId),
  });
}

/** Updates settings and invalidates all derived forecast resources. @param accountId - Owning account. @param systemId - Target system. @returns Mutation state. */
export function useUpdateForecastSettings(accountId: string, systemId: string) {
  const client = useQueryClient();
  return useMutation({
    mutationFn: ({
      input,
      etag,
    }: {
      input: ForecastSettingsInput;
      etag: string;
    }) => updateForecastSettings(accountId, systemId, input, etag),
    onSuccess: async () =>
      client.invalidateQueries({ queryKey: rootKey(accountId, systemId) }),
  });
}

/** Loads a bounded modeled yield series. @param accountId - Owning account. @param systemId - Target system. @param range - Query range. @param basis - Forecast or expected basis. @returns Query state. */
export function useYieldSeries(
  accountId: string,
  systemId: string,
  range: ForecastRange,
  basis: "forecast" | "expected",
) {
  return useQuery({
    queryKey: [...rootKey(accountId, systemId), "yield", basis, range],
    queryFn: () => fetchYieldSeries(accountId, systemId, range, basis),
    staleTime: 60_000,
    enabled: Boolean(accountId && systemId),
  });
}

/** Loads aligned actual and modeled energy. @param accountId - Owning account. @param systemId - Target system. @param range - Query range. @param metric - Performance metric. @returns Query state. */
export function usePerformanceSeries(
  accountId: string,
  systemId: string,
  range: ForecastRange,
  metric: "generation_performance" | "forecast_realization",
) {
  return useQuery({
    queryKey: [...rootKey(accountId, systemId), "performance", metric, range],
    queryFn: () => fetchPerformanceSeries(accountId, systemId, range, metric),
    staleTime: 60_000,
    enabled: Boolean(accountId && systemId),
  });
}
