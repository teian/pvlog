import {
  fetchSeasonal,
  fetchStatistics,
  fetchSystemOverview,
  fetchWeather,
} from "@/features/reporting/api/reportingApi";
import { useQueries, useQuery } from "@tanstack/react-query";

export const useSystemOverviews = (systemIds: string[]) =>
  useQueries({
    queries: systemIds.map((systemId) => ({
      queryKey: ["system-overview", systemId],
      queryFn: () => fetchSystemOverview(systemId),
      staleTime: 60_000,
    })),
  });

export const useStatisticsReport = (systemId: string) =>
  useQuery({
    queryKey: ["statistics-report", systemId],
    queryFn: () => fetchStatistics(systemId),
    enabled: systemId.length > 0,
  });

export const useSeasonalReport = (systemId: string) =>
  useQuery({
    queryKey: ["seasonal-report", systemId],
    queryFn: () => fetchSeasonal(systemId),
    enabled: systemId.length > 0,
  });

export const useWeatherReport = (systemId: string) =>
  useQuery({
    queryKey: ["weather-report", systemId],
    queryFn: () => fetchWeather(systemId),
    enabled: systemId.length > 0,
  });
