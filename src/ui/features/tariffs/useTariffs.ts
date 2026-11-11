import {
  deleteTariff,
  fetchTariffs,
  saveTariff,
} from "@/features/tariffs/tariffsApi";
import type { TariffInput } from "@/features/tariffs/tariffs.types";
import {
  useMutation,
  useQuery,
  useQueryClient,
  type QueryClient,
} from "@tanstack/react-query";

const tariffKey = (systemId: string) =>
  ["system", systemId, "tariffs"] as const;

/** Refreshes the tariff list and the checklist that reports a missing tariff. */
async function refresh(queryClient: QueryClient, systemId: string) {
  await Promise.all([
    queryClient.invalidateQueries({ queryKey: tariffKey(systemId) }),
    queryClient.invalidateQueries({
      queryKey: ["system", systemId, "setup-status"],
    }),
  ]);
}

/** Loads a system's tariffs. @param systemId - System whose tariffs to load. @returns Query state of the tariffs. */
export function useTariffs(systemId: string) {
  return useQuery({
    queryKey: tariffKey(systemId),
    queryFn: () => fetchTariffs(systemId),
    staleTime: 0,
  });
}

/** Creates or replaces a tariff. @param systemId - Owning system. @returns Save mutation state. */
export function useSaveTariff(systemId: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ input, id }: { input: TariffInput; id?: string }) =>
      saveTariff(systemId, input, id),
    onSuccess: () => refresh(queryClient, systemId),
  });
}

/** Deletes a tariff. @param systemId - Owning system. @returns Deletion mutation state. */
export function useDeleteTariff(systemId: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => deleteTariff(systemId, id),
    onSuccess: () => refresh(queryClient, systemId),
  });
}
