import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router";
import { describe, expect, it, vi } from "vitest";

import { StatisticsPage } from "@/pages/ReportingPages";
import i18n from "@/shared/lib/i18n";

const ARCHIVED_SYSTEM = "01a0492f-48fa-7133-a882-f8fd05fc765a";
const ACTIVE_SYSTEM = "01a04a33-cba5-76c2-9405-20b0d16e619b";

function overview(id: string, name: string, lifecycle: string) {
  return {
    id,
    name,
    timezone: "Europe/Berlin",
    lifecycle,
    inverterCount: 2,
    stringCount: 3,
    capacityWatts: 19_520,
  };
}

function statistics(systemId: string, generationEnergyWh: number) {
  return {
    systemId,
    generationEnergyWh,
    consumptionEnergyWh: 5_069_566,
    peakGenerationPowerWatts: 8989,
    firstObservationAtEpochMillis: 1_724_882_400_000,
    lastObservationAtEpochMillis: 1_787_954_100_000,
    coverageBasisPoints: 9959,
    monthly: [],
  };
}

/** Serves the session, both overviews, and per-system statistics. */
function stubApi() {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: string) => {
      const body = input.startsWith("/api/v1/session")
        ? {
            authenticated: true,
            user: { id: ACTIVE_SYSTEM, displayName: "Ada" },
            accountId: ACTIVE_SYSTEM,
            // Session order puts the older, archived system first.
            systemIds: [ARCHIVED_SYSTEM, ACTIVE_SYSTEM],
            permissions: ["telemetry_read"],
            connectors: [],
          }
        : input.includes(`${ARCHIVED_SYSTEM}/overview`)
          ? overview(ARCHIVED_SYSTEM, "Korb", "archived")
          : input.includes(`${ACTIVE_SYSTEM}/overview`)
            ? overview(ACTIVE_SYSTEM, "Testanlage Nord", "active")
            : input.includes(ARCHIVED_SYSTEM)
              ? statistics(ARCHIVED_SYSTEM, 1000)
              : statistics(ACTIVE_SYSTEM, 27_898_065);
      return new Response(JSON.stringify(body), { status: 200 });
    }),
  );
}

function renderStatistics(initialEntry: string) {
  return render(
    <QueryClientProvider client={new QueryClient()}>
      <MemoryRouter initialEntries={[initialEntry]}>
        <StatisticsPage />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

describe("reporting system selector", () => {
  it("prefers the active system over an archived one listed first", async () => {
    await i18n.changeLanguage("en");
    stubApi();
    renderStatistics("/statistics");

    expect(await screen.findByText("27,898.1 kWh")).toBeVisible();
    expect(await screen.findByLabelText("System")).toHaveTextContent(
      "Testanlage Nord",
    );
  });

  it("switches the report when another system is chosen", async () => {
    await i18n.changeLanguage("en");
    stubApi();
    renderStatistics("/statistics");

    const trigger = await screen.findByLabelText("System");
    await waitFor(() => {
      expect(trigger).toHaveTextContent("Testanlage Nord");
    });
    await userEvent.click(trigger);
    await userEvent.click(
      await screen.findByRole("option", { name: "Korb (archived)" }),
    );

    await waitFor(() => {
      expect(screen.getByText("1 kWh")).toBeVisible();
    });
  });

  it("honours an explicit system search parameter", async () => {
    await i18n.changeLanguage("en");
    stubApi();
    renderStatistics(`/statistics?system=${ARCHIVED_SYSTEM}`);

    expect(await screen.findByText("1 kWh")).toBeVisible();
  });
});
