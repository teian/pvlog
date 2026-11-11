import {
  createSystem,
  deleteSystem,
  fetchFirstData,
  fetchSetupStatus,
  fetchSystem,
  setSystemArchived,
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

/** Archives or restores a system. @param systemId - System to change. @returns Lifecycle mutation state. */
export function useSetSystemArchived(systemId: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({
      version,
      action,
    }: {
      version: number;
      action: "archive" | "restore";
    }) => setSystemArchived(systemId, version, action),
    onSuccess: (record) => {
      queryClient.setQueryData(systemKeys.record(systemId), record);
    },
  });
}

/** Permanently deletes a system and refreshes the session's system list. @param systemId - System to delete. @returns Deletion mutation state. */
export function useDeleteSystem(systemId: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (version: number) => deleteSystem(systemId, version),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ["session"] });
    },
  });
}
