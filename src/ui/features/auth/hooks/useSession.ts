import { fetchSession } from "@/features/auth/api/authApi";
import { useQuery } from "@tanstack/react-query";

/** Bootstraps the current browser session with dynamic caching. @returns The session query state. */
export function useSession() {
  return useQuery({
    queryKey: ["session"],
    queryFn: fetchSession,
    staleTime: 0,
    retry: false,
  });
}
