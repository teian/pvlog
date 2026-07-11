import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { DashboardPage } from "@/pages/DashboardPage";
import i18n from "@/shared/lib/i18n";

describe("DashboardPage", () => {
  it("withholds stale telemetry from the live state", async () => {
    await i18n.changeLanguage("en");
    vi.stubGlobal(
      "fetch",
      vi.fn(
        async () =>
          new Response(
            JSON.stringify({
              observedAtEpochMillis: Date.now() - 120_000,
              ageSeconds: 120,
              freshnessThresholdSeconds: 60,
              generationWatts: 4200,
              consumptionWatts: 1000,
              gridWatts: 20,
              batteryBasisPoints: 5000,
              coverageBasisPoints: 9750,
              recentAlerts: [
                {
                  id: "1",
                  title: "Low generation",
                  state: "open",
                  openedAtEpochMillis: 1,
                },
              ],
              ingestion: { acceptedToday: 20, rejectedToday: 1, lagSeconds: 4 },
            }),
            { status: 200 },
          ),
      ),
    );
    render(
      <QueryClientProvider client={new QueryClient()}>
        <DashboardPage />
      </QueryClientProvider>,
    );
    expect(await screen.findByText("Live data is stale")).toBeVisible();
    expect(
      screen.getByText(
        "Last data arrived 120 seconds ago; live values are withheld.",
      ),
    ).toBeVisible();
    expect(screen.getByText("Low generation")).toBeVisible();
  });
});
