import {
  createDataSource,
  deleteDataSource,
  deleteTestReadings,
  fetchDataSources,
  fetchSourceProfiles,
  regenerateDataSourceCredential,
  renameDataSource,
  sendTestReading,
} from "@/entities/dataSource/dataSourceApi";
import type {
  DataSource,
  DataSourceConfiguration,
} from "@/entities/dataSource/dataSource.types";
import {
  useMutation,
  useQuery,
  useQueryClient,
  type QueryClient,
} from "@tanstack/react-query";

const listKey = (systemId: string) =>
  ["system", systemId, "data-sources"] as const;

/** Refreshes everything that depends on a system's data sources and readings. */
async function refresh(queryClient: QueryClient, systemId: string) {
  await Promise.all([
    queryClient.invalidateQueries({ queryKey: listKey(systemId) }),
    queryClient.invalidateQueries({
      queryKey: ["system", systemId, "setup-status"],
    }),
    queryClient.invalidateQueries({
      queryKey: ["system", systemId, "first-data"],
    }),
    queryClient.invalidateQueries({ queryKey: ["account", "api-keys"] }),
  ]);
}

/** Loads the uploader kinds once; they do not change at runtime. @returns Query state of the profile catalog. */
export function useSourceProfiles() {
  return useQuery({
    queryKey: ["data-source-profiles"],
    queryFn: fetchSourceProfiles,
    staleTime: Infinity,
  });
}

/** Loads a system's data sources. @param systemId - System whose sources to load. @param refetchIntervalMillis - Refresh interval for the connection state; false to load once. @returns Query state of the data sources. */
export function useDataSources(
  systemId: string,
  refetchIntervalMillis: number | false = false,
) {
  return useQuery({
    queryKey: listKey(systemId),
    queryFn: () => fetchDataSources(systemId),
    staleTime: 0,
    refetchInterval: refetchIntervalMillis,
  });
}

/**
 * Creates a data source while keeping its key out of the query cache.
 *
 * @param systemId - System to feed.
 * @param onIssued - Receives the one-time configuration; the caller holds it in component state only.
 * @returns Creation mutation resolving to the source without its secret.
 */
export function useCreateDataSource(
  systemId: string,
  onIssued: (
    dataSource: DataSource,
    configuration: DataSourceConfiguration | null,
  ) => void,
) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (input: { name: string; profile: string }) => {
      const issued = await createDataSource(systemId, input);
      onIssued(issued.dataSource, issued.configuration);
      return issued.dataSource;
    },
    onSuccess: () => refresh(queryClient, systemId),
  });
}

/**
 * Replaces a data source's key while keeping the new key out of the query cache.
 *
 * @param systemId - Owning system.
 * @param onIssued - Receives the new one-time configuration.
 * @returns Regeneration mutation resolving to the source without its secret.
 */
export function useRegenerateDataSourceCredential(
  systemId: string,
  onIssued: (
    dataSource: DataSource,
    configuration: DataSourceConfiguration | null,
  ) => void,
) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async (id: string) => {
      const issued = await regenerateDataSourceCredential(systemId, id);
      onIssued(issued.dataSource, issued.configuration);
      return issued.dataSource;
    },
    onSuccess: () => refresh(queryClient, systemId),
  });
}

/** Renames a data source. @param systemId - Owning system. @returns Rename mutation state. */
export function useRenameDataSource(systemId: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ id, name }: { id: string; name: string }) =>
      renameDataSource(systemId, id, name),
    onSuccess: () => refresh(queryClient, systemId),
  });
}

/** Deletes a data source. @param systemId - Owning system. @returns Deletion mutation state. */
export function useDeleteDataSource(systemId: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => deleteDataSource(systemId, id),
    onSuccess: () => refresh(queryClient, systemId),
  });
}

/** Sends a test reading through a data source. @param systemId - Owning system. @returns Test mutation state. */
export function useSendTestReading(systemId: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => sendTestReading(systemId, id),
    onSuccess: () => refresh(queryClient, systemId),
  });
}

/** Removes a system's test readings. @param systemId - System to clean. @returns Removal mutation state. */
export function useDeleteTestReadings(systemId: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: () => deleteTestReadings(systemId),
    onSuccess: () => refresh(queryClient, systemId),
  });
}
