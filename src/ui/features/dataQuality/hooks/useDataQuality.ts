import { fetchDataQuality } from "@/features/dataQuality/api/dataQualityApi";
import { useQuery } from "@tanstack/react-query";

/** Fetches data-quality issues for a range ending now, optionally re-polling while reconciliation is expected to be in progress. @param systemId - System to inspect. @param durationMillis - Range length ending now, in milliseconds. @param refetchIntervalMillis - Re-poll interval while truthy; omit to fetch once per range change. @returns The data-quality query state. */
export function useDataQuality(
  systemId: string,
  durationMillis: number,
  refetchIntervalMillis?: number,
) {
  return useQuery({
    queryKey: ["dataQuality", systemId, durationMillis],
    queryFn: ({ signal }) => {
      const endEpochMillis = Date.now();
      return fetchDataQuality(
        systemId,
        endEpochMillis - durationMillis,
        endEpochMillis,
        signal,
      );
    },
    staleTime: 30_000,
    refetchInterval: refetchIntervalMillis ?? false,
  });
}
