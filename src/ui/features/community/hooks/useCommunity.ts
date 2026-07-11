import {
  addFavourite,
  compareSystems,
  fetchCommunitySystems,
  fetchFavourites,
  fetchLadder,
  removeFavourite,
} from "@/features/community/api/communityApi";
import type { CommunitySearchFilters } from "@/features/community/types/community.types";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

/** Searches visible community systems for the supplied filters. @param filters - Current search filters. @returns The community search query state. */
export function useCommunitySearch(filters: CommunitySearchFilters) {
  return useQuery({
    queryKey: ["community", "systems", filters],
    queryFn: () => fetchCommunitySystems(filters),
    retry: false,
  });
}

/** Lists the active user's favourites. @returns The favourites query state. */
export function useFavourites() {
  return useQuery({
    queryKey: ["community", "favourites"],
    queryFn: fetchFavourites,
    retry: false,
  });
}

/** Loads the public normalized-generation ladder. @returns The ladder query state. */
export function useLadder() {
  return useQuery({
    queryKey: ["community", "ladder"],
    queryFn: fetchLadder,
    retry: false,
  });
}

/** Mutates favourites and refreshes every affected community view. @returns The favourite mutation state. */
export function useFavouriteMutation() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async ({
      systemId,
      favourite,
    }: {
      systemId: string;
      favourite: boolean;
    }) => {
      if (favourite) await addFavourite(systemId);
      else await removeFavourite(systemId);
    },
    onSuccess: async () =>
      queryClient.invalidateQueries({ queryKey: ["community"] }),
  });
}

/** Compares a selected set of visible systems. @returns The comparison mutation state. */
export function useSystemComparison() {
  return useMutation({ mutationFn: compareSystems });
}
