import { render, screen, within } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router";
import { describe, expect, it, vi } from "vitest";

import { HomePage } from "@/pages/HomePage";
import i18n from "@/shared/lib/i18n";

describe("HomePage", () => {
  it("switches the application identity from English to German", async () => {
    await i18n.changeLanguage("en");
    vi.stubGlobal(
      "fetch",
      vi.fn(
        async (input: string) =>
          new Response(
            JSON.stringify(
              input === "/api/v1/session"
                ? {
                    authenticated: true,
                    user: {
                      id: "019505c8-7c85-7f0b-9bc3-2a3c4d5e6f70",
                      displayName: "Ada",
                    },
                    accountId: "019505c8-7c85-7f0b-9bc3-2a3c4d5e6f70",
                    systemIds: [],
                    permissions: [],
                    connectors: [],
                  }
                : {
                    observedAtEpochMillis: Date.now(),
                    ageSeconds: 0,
                    freshnessThresholdSeconds: 60,
                    generationWatts: 4200,
                    consumptionWatts: null,
                    gridWatts: null,
                    batteryBasisPoints: null,
                    coverageBasisPoints: 10000,
                    recentAlerts: [],
                    ingestion: {
                      acceptedToday: 1,
                      rejectedToday: 0,
                      lagSeconds: 0,
                    },
                  },
            ),
            { status: 200 },
          ),
      ),
    );
    render(
      <QueryClientProvider client={new QueryClient()}>
        <MemoryRouter>
          <HomePage />
        </MemoryRouter>
      </QueryClientProvider>,
    );

    expect(await screen.findByRole("heading", { level: 1 })).toHaveTextContent(
      "All Systems",
    );
    expect(
      await screen.findByText("Live data received 0 seconds ago."),
    ).toBeVisible();

    await i18n.changeLanguage("de");
    const navigation = screen.getByRole("navigation");
    expect(
      within(navigation).getByRole("link", { name: "Übersicht" }),
    ).toBeVisible();
    for (const label of ["Anlagen", "Statistik", "Jahreszeiten", "Wetter"]) {
      expect(within(navigation).getAllByText(label)[0]).toBeVisible();
    }
    expect(
      within(navigation).getByRole("link", { name: "Verwalten" }),
    ).toBeVisible();
    expect(screen.getByRole("link", { name: "Verwaltung" })).toBeVisible();
    expect(screen.queryByText("Administration")).not.toBeInTheDocument();
  });
});
