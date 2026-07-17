import {
  deleteManagedSystem,
  fetchManagedSystem,
  saveManagedSystem,
} from "@/features/systemManagement/api/systemManagementApi";
import type {
  ManagedSystem,
  SystemWizardDraft,
} from "@/features/systemManagement/types/systemManagement.types";
import { useMutation, useQueries, useQueryClient } from "@tanstack/react-query";

/** Loads all systems visible to the current management session. @param systemIds - Authorized systems. @returns Parallel system queries. */
export function useManagedSystems(systemIds: string[]) {
  return useQueries({
    queries: systemIds.map((systemId) => ({
      queryKey: ["system-management", systemId],
      queryFn: () => fetchManagedSystem(systemId),
      staleTime: 0,
    })),
  });
}

/** Persists one create/edit wizard and refreshes session/system state. @returns Mutation state. */
export function useSaveManagedSystem() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({
      draft,
      current,
    }: {
      draft: SystemWizardDraft;
      current?: ManagedSystem;
    }) => saveManagedSystem(draft, current),
    onSuccess: async () => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ["session"] }),
        queryClient.invalidateQueries({ queryKey: ["system-management"] }),
      ]);
    },
  });
}

/** Deletes one system and refreshes session/system state. @returns Mutation state. */
export function useDeleteManagedSystem() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: deleteManagedSystem,
    onSuccess: async () => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: ["session"] }),
        queryClient.invalidateQueries({ queryKey: ["system-management"] }),
      ]);
    },
  });
}
