import {
  commitHistoryImport,
  fetchHistoryImports,
  uploadHistoryImport,
} from "@/entities/dataSource/historyImportApi";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

const importsKey = (systemId: string, dataSourceId: string) =>
  ["system", systemId, "data-sources", dataSourceId, "imports"] as const;

/** Loads the imports of an import source. @param systemId - Owning system. @param dataSourceId - Import source. @returns Query state of the imports, newest first. */
export function useHistoryImports(systemId: string, dataSourceId: string) {
  return useQuery({
    queryKey: importsKey(systemId, dataSourceId),
    queryFn: () => fetchHistoryImports(systemId, dataSourceId),
  });
}

/** Uploads a file for a dry run. @param systemId - Owning system. @param dataSourceId - Import source. @returns Upload mutation resolving to the dry run. */
export function useUploadHistoryImport(systemId: string, dataSourceId: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (file: File) =>
      uploadHistoryImport(systemId, dataSourceId, file),
    onSuccess: () =>
      queryClient.invalidateQueries({
        queryKey: importsKey(systemId, dataSourceId),
      }),
  });
}

/** Commits a dry run; afterwards every view of the system's history is reloaded. @param systemId - Owning system. @param dataSourceId - Import source. @returns Commit mutation resolving to the committed import. */
export function useCommitHistoryImport(systemId: string, dataSourceId: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (importId: string) =>
      commitHistoryImport(systemId, dataSourceId, importId),
    onSuccess: () =>
      queryClient.invalidateQueries({ queryKey: ["system", systemId] }),
  });
}
