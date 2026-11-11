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

/**
 * Orders systems so that active ones come first.
 *
 * The session lists systems by identifier, which puts an archived system in
 * front of everything created after it. Reports default to the first entry, so
 * without this an archived system would shadow the account's live data.
 *
 * @param systemIds - Systems visible to the current session, in session order.
 * @returns The same identifiers, archived ones moved to the end.
 */
export const useOrderedSystemIds = (systemIds: string[]) => {
  const overviews = useSystemOverviews(systemIds);
  const archived = new Set(
    overviews.flatMap((query) =>
      query.data?.lifecycle === "archived" ? [query.data.id] : [],
    ),
  );
  return [
    ...systemIds.filter((systemId) => !archived.has(systemId)),
    ...systemIds.filter((systemId) => archived.has(systemId)),
  ];
};

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
