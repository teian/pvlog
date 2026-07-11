import { correctObservation } from "@/features/dataQuality/api/dataQualityApi";
import { useMutation, useQueryClient } from "@tanstack/react-query";

/** Submits an optimistic observation correction or deletion and invalidates cached data-quality/series queries so reconciliation is reflected once complete. @returns The correction mutation state. */
export function useCorrectObservation() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: correctObservation,
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ["dataQuality"] });
      await queryClient.invalidateQueries({ queryKey: ["series"] });
    },
  });
}
