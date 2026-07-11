import { logout } from "@/features/auth/api/authApi";
import { useMutation, useQueryClient } from "@tanstack/react-query";

/** Revokes the server session and drops all browser-session cache entries. @returns The logout mutation state. */
export function useLogout() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: logout,
    onSuccess: () => {
      queryClient.removeQueries({ queryKey: ["session"] });
      if (typeof window !== "undefined")
        window.sessionStorage.removeItem("pvlog.csrf-token");
    },
  });
}
