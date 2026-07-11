import { fetchDashboard } from "@/features/dashboard/api/dashboardApi";
import { useQuery } from "@tanstack/react-query";

/** Fetches live operational state with short bounded staleness. @returns Dashboard query state. */
export function useDashboard() {
  return useQuery({
    queryKey: ["dashboard"],
    queryFn: fetchDashboard,
    staleTime: 15_000,
    refetchInterval: 30_000,
  });
}
