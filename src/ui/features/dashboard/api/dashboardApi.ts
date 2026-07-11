import { z } from "zod";

const dashboardSchema = z.object({
  observedAtEpochMillis: z.number().int(),
  ageSeconds: z.number().int().min(0),
  freshnessThresholdSeconds: z.number().int().positive(),
  generationWatts: z.number(),
  consumptionWatts: z.number().nullable(),
  gridWatts: z.number().nullable(),
  batteryBasisPoints: z.number().int().nullable(),
  coverageBasisPoints: z.number().int().min(0).max(10_000),
  recentAlerts: z.array(
    z.object({
      id: z.string(),
      title: z.string(),
      state: z.enum(["open", "resolved"]),
      openedAtEpochMillis: z.number().int(),
    }),
  ),
  ingestion: z.object({
    acceptedToday: z.number().int().min(0),
    rejectedToday: z.number().int().min(0),
    lagSeconds: z.number().int().min(0),
  }),
});

/** Operational dashboard data validated from the modern API. */
export type Dashboard = z.infer<typeof dashboardSchema>;

/** Retrieves the current system-wide dashboard projection. @returns Validated operational state. */
export async function fetchDashboard(): Promise<Dashboard> {
  const response = await fetch("/api/v1/dashboard", {
    credentials: "same-origin",
  });
  if (!response.ok)
    throw new Error(`dashboard_failed:${String(response.status)}`);
  return dashboardSchema.parse(await response.json());
}
