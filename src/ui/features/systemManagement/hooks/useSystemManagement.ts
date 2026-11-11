import {
  fetchInverters,
  saveInverters,
} from "@/features/systemManagement/api/systemManagementApi";
import type {
  ManagedInverter,
  SystemInverterDraft,
} from "@/features/systemManagement/types/systemManagement.types";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

const inverterKey = (systemId: string) =>
  ["system", systemId, "inverters"] as const;

/** Loads a system's inverters with their strings. @param systemId - System whose equipment to load. @returns Query state of the equipment. */
export function useInverters(systemId: string) {
  return useQuery({
    queryKey: inverterKey(systemId),
    queryFn: () => fetchInverters(systemId),
    staleTime: 0,
  });
}

/** Saves a system's equipment and refreshes everything derived from it. @param systemId - System whose equipment to save. @returns Save mutation state. */
export function useSaveInverters(systemId: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({
      drafts,
      current,
    }: {
      drafts: SystemInverterDraft[];
      current: ManagedInverter[];
    }) => saveInverters(systemId, drafts, current),
    onSettled: async () => {
      // Also after a failure: some inverters may already have been stored.
      await queryClient.invalidateQueries({ queryKey: ["system", systemId] });
    },
  });
}
