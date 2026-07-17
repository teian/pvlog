import {
  fetchInverterCatalog,
  fetchSolarModuleCatalog,
  saveEquipmentConfiguration,
} from "@/features/equipmentCatalog/api/equipmentCatalogApi";
import type { EquipmentCatalogQuery } from "@/features/equipmentCatalog/types/equipmentCatalog.types";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

const CATALOG_STALE_TIME_MS = 60 * 60 * 1000;

function normalizedQuery(query: EquipmentCatalogQuery): EquipmentCatalogQuery {
  return {
    search: query.search?.trim().slice(0, 80),
    manufacturer: query.manufacturer?.trim().slice(0, 80),
    offset: Math.max(0, query.offset ?? 0),
    limit: Math.min(100, Math.max(1, query.limit ?? 25)),
  };
}

/** Loads a cached inverter catalog search. @param query - Search, filter and pagination state. @returns TanStack Query loading, empty, data and error state. */
export function useInverterCatalog(query: EquipmentCatalogQuery) {
  const normalized = normalizedQuery(query);
  return useQuery({
    queryKey: ["equipmentCatalog", "inverters", normalized],
    queryFn: () => fetchInverterCatalog(normalized),
    staleTime: CATALOG_STALE_TIME_MS,
    retry: false,
  });
}

/** Loads a cached solar-module catalog search. @param query - Search, filter and pagination state. @returns TanStack Query loading, empty, data and error state. */
export function useSolarModuleCatalog(query: EquipmentCatalogQuery) {
  const normalized = normalizedQuery(query);
  return useQuery({
    queryKey: ["equipmentCatalog", "solarModules", normalized],
    queryFn: () => fetchSolarModuleCatalog(normalized),
    staleTime: CATALOG_STALE_TIME_MS,
    retry: false,
  });
}

/** Saves confirmed equipment and refreshes configured inverters. @param accountId - Owning account. @param systemId - Owning system. @returns The mutation state. */
export function useSaveEquipmentConfiguration(
  accountId: string,
  systemId: string,
) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (input: unknown) => saveEquipmentConfiguration(systemId, input),
    onSuccess: async () =>
      queryClient.invalidateQueries({
        queryKey: ["administration", "inverters", accountId, systemId],
      }),
  });
}
