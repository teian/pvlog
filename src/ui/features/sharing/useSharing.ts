import { fetchSharing, updateSharing } from "@/features/sharing/sharingApi";
import type { SharingInput } from "@/features/sharing/sharing.types";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

const sharingKey = (systemId: string) =>
  ["system", systemId, "sharing"] as const;

/** Loads a system's sharing state. @param systemId - System to inspect. @returns Query state of the sharing state. */
export function useSharing(systemId: string) {
  return useQuery({
    queryKey: sharingKey(systemId),
    queryFn: () => fetchSharing(systemId),
    staleTime: 0,
  });
}

/** Changes a system's sharing. @param systemId - System to change. @returns Update mutation state. */
export function useUpdateSharing(systemId: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (input: SharingInput) => updateSharing(systemId, input),
    onSuccess: (state) => {
      queryClient.setQueryData(sharingKey(systemId), state);
    },
  });
}
