import {
  bindChannel,
  fetchBindableEquipment,
  fetchChannels,
  retireChannel,
} from "@/features/measurementChannels/measurementChannelsApi";
import type { ChannelBinding } from "@/features/measurementChannels/measurementChannels.types";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

const channelKey = (systemId: string) =>
  ["system", systemId, "channels"] as const;

/** Loads a system's measurement channels. @param systemId - System whose channels to load. @returns Query state of the channels. */
export function useChannels(systemId: string) {
  return useQuery({
    queryKey: channelKey(systemId),
    queryFn: () => fetchChannels(systemId),
    staleTime: 0,
  });
}

/** Loads the equipment channels can be bound to. @param systemId - System whose equipment to load. @returns Query state of the inverters with their strings. */
export function useBindableEquipment(systemId: string) {
  return useQuery({
    queryKey: ["system", systemId, "bindable-equipment"],
    queryFn: () => fetchBindableEquipment(systemId),
    staleTime: 0,
  });
}

/** Binds or renames a channel and refreshes the checklist that counts unassigned channels. @param systemId - Owning system. @returns Binding mutation state. */
export function useBindChannel(systemId: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ key, binding }: { key: string; binding: ChannelBinding }) =>
      bindChannel(systemId, key, binding),
    onSuccess: async () => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: channelKey(systemId) }),
        queryClient.invalidateQueries({
          queryKey: ["system", systemId, "setup-status"],
        }),
      ]);
    },
  });
}

/** Retires a channel. @param systemId - Owning system. @returns Retire mutation state. */
export function useRetireChannel(systemId: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (key: string) => retireChannel(systemId, key),
    onSuccess: async () => {
      await Promise.all([
        queryClient.invalidateQueries({ queryKey: channelKey(systemId) }),
        queryClient.invalidateQueries({
          queryKey: ["system", systemId, "setup-status"],
        }),
      ]);
    },
  });
}
