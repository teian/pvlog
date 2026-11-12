import {
  adoptDiscoveryProposal,
  dismissDiscoveryProposal,
  fetchDiscoveryProposals,
  resetDiscoveryProposal,
} from "@/entities/equipmentDiscovery/equipmentDiscoveryApi";
import type { DiscoveryAdoption } from "@/entities/equipmentDiscovery/equipmentDiscovery.types";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

const proposalsKey = (systemId: string) =>
  ["system", systemId, "discovery-proposals"] as const;

/** Loads the open and the dismissed discovery proposals of a system. @param systemId - System whose proposals to load. @returns Query state of the proposals. */
export function useDiscoveryProposals(systemId: string) {
  return useQuery({
    queryKey: proposalsKey(systemId),
    queryFn: () => fetchDiscoveryProposals(systemId),
    staleTime: 0,
  });
}

/** Adopts a proposal and refreshes everything of the system, because equipment, measurement channels and the checklist change with it. @param systemId - Owning system. @returns Adoption mutation state. */
export function useAdoptDiscoveryProposal(systemId: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({
      proposalId,
      adoption,
    }: {
      proposalId: string;
      adoption: DiscoveryAdoption;
    }) => adoptDiscoveryProposal(systemId, proposalId, adoption),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ["system", systemId] });
    },
  });
}

/** Dismisses a proposal. @param systemId - Owning system. @returns Dismissal mutation state. */
export function useDismissDiscoveryProposal(systemId: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (proposalId: string) =>
      dismissDiscoveryProposal(systemId, proposalId),
    onSuccess: async () => {
      await queryClient.invalidateQueries({
        queryKey: proposalsKey(systemId),
      });
    },
  });
}

/** Opens a dismissed proposal again. @param systemId - Owning system. @returns Reset mutation state. */
export function useResetDiscoveryProposal(systemId: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (proposalId: string) =>
      resetDiscoveryProposal(systemId, proposalId),
    onSuccess: async () => {
      await queryClient.invalidateQueries({
        queryKey: proposalsKey(systemId),
      });
    },
  });
}
