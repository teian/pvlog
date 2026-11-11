import {
  createSystem,
  fetchFirstData,
  fetchSetupStatus,
  fetchSystem,
  updateSystem,
} from "@/entities/system/systemApi";
import type {
  CreateSystemInput,
  UpdateSystemInput,
} from "@/entities/system/system.types";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

/** Query keys of the system entity, shared so mutations can refresh them. */
export const systemKeys = {
  record: (systemId: string) => ["system", systemId, "record"] as const,
  setupStatus: (systemId: string) =>
    ["system", systemId, "setup-status"] as const,
  firstData: (systemId: string) => ["system", systemId, "first-data"] as const,
};

/** Loads one system record. @param systemId - System to load. @returns Query state of the system record. */
export function useSystem(systemId: string) {
  return useQuery({
    queryKey: systemKeys.record(systemId),
    queryFn: () => fetchSystem(systemId),
    staleTime: 0,
  });
}

/** Loads a system's setup status. @param systemId - System to inspect. @returns Query state of the setup status. */
export function useSetupStatus(systemId: string) {
  return useQuery({
    queryKey: systemKeys.setupStatus(systemId),
    queryFn: () => fetchSetupStatus(systemId),
    staleTime: 0,
  });
}

/** Watches for a system's first data. @param systemId - System to watch. @param pollIntervalMillis - Refetch interval while no real reading arrived; false to load once. @returns Query state of the first-data check. */
export function useFirstData(
  systemId: string,
  pollIntervalMillis: number | false = false,
) {
  return useQuery({
    queryKey: systemKeys.firstData(systemId),
    queryFn: () => fetchFirstData(systemId),
    staleTime: 0,
    refetchInterval: (query) =>
      query.state.data?.received ? false : pollIntervalMillis,
  });
}

/** Creates a system and refreshes the session's system list. @returns Creation mutation state. */
export function useCreateSystem() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (input: CreateSystemInput) => createSystem(input),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ["session"] });
    },
  });
}

/** Updates a system and refreshes everything derived from it. @param systemId - System to change. @returns Update mutation state. */
export function useUpdateSystem(systemId: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({
      version,
      input,
    }: {
      version: number;
      input: UpdateSystemInput;
    }) => updateSystem(systemId, version, input),
    onSuccess: async (record) => {
      queryClient.setQueryData(systemKeys.record(systemId), record);
      await Promise.all([
        queryClient.invalidateQueries({
          queryKey: systemKeys.setupStatus(systemId),
        }),
        queryClient.invalidateQueries({ queryKey: ["system-management"] }),
      ]);
    },
  });
}
