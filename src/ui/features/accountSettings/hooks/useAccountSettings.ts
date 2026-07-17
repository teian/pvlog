import {
  changeAccountPassword,
  fetchAccountProfile,
  updateAccountProfile,
} from "@/features/accountSettings/api/accountSettingsApi";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

const profileQueryKey = ["account", "profile"] as const;

/** Loads the authenticated user's safe profile. @returns Profile query state. */
export function useAccountProfile() {
  return useQuery({ queryKey: profileQueryKey, queryFn: fetchAccountProfile });
}

/** Updates the display name and refreshes profile and session identity caches. @returns Profile mutation state. */
export function useUpdateAccountProfile() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: updateAccountProfile,
    onSuccess: async (profile) => {
      queryClient.setQueryData(profileQueryKey, profile);
      await queryClient.invalidateQueries({ queryKey: ["session"] });
    },
  });
}

/** Changes the local password without retaining password values in query state. @returns Password mutation state. */
export function useChangeAccountPassword() {
  return useMutation({ mutationFn: changeAccountPassword });
}
