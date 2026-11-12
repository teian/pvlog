import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ReactElement } from "react";
import { MemoryRouter } from "react-router";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { SetupChecklist } from "@/features/systemSetup";
import { DashboardPage } from "@/pages/DashboardPage";
import { StatisticsPage } from "@/pages/ReportingPages";
import i18n from "@/shared/lib/i18n";

const accountId = "019505c8-7c85-7f0b-9bc3-2a3c4d5e6f70";
const systemId = "019505c8-7c85-7f0b-9bc3-2a3c4d5e6f71";

interface Item {
  key: string;
  state: string;
  reason?: string;
  count?: number;
}

const allDone: Item[] = [
  "location",
  "capacity",
  "data_source",
  "first_data",
  "channels",
  "equipment",
  "tariff",
  "forecast",
].map((key) => ({ key, state: "done" }));

function withItems(...open: Item[]): Item[] {
  return allDone.map(
    (item) => open.find((entry) => entry.key === item.key) ?? item,
  );
}

interface Scenario {
  systemIds?: string[];
  permissions?: string[];
  dataState?: string;
  items?: Item[];
}

function installBackend({
  systemIds = [systemId],
  permissions = ["system_manage"],
  dataState = "none",
  items = allDone,
}: Scenario) {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: string | URL | Request) => {
      const url = input instanceof Request ? input.url : input.toString();
      const body = (() => {
        if (url === "/api/v1/session")
          return {
            authenticated: true,
            user: { id: accountId, displayName: "Ada" },
            accountId,
            systemIds,
            permissions,
            connectors: [],
          };
        if (url === "/api/v1/dashboard")
          return {
            dataState,
            systemCount: systemIds.length,
            observedAtEpochMillis: 0,
            ageSeconds: 0,
            freshnessThresholdSeconds: 900,
            generationWatts: 0,
            consumptionWatts: null,
            gridWatts: null,
            batteryBasisPoints: null,
            coverageBasisPoints: 0,
            recentAlerts: [],
            ingestion: { acceptedToday: 0, rejectedToday: 0, lagSeconds: 0 },
          };
        if (url === `/api/v1/systems/${systemId}/setup-status`)
          return {
            systemId,
            dataState,
            lastReceivedAtEpochMillis: null,
            items,
          };
        if (url === `/api/v1/systems/${systemId}/overview`)
          return {
            id: systemId,
            name: "Roof",
            timezone: "Europe/Berlin",
            lifecycle: "active",
            inverterCount: 0,
            stringCount: 0,
            capacityWatts: null,
            dataState,
          };
        if (url === `/api/v1/systems/${systemId}/reporting/statistics`)
          return {
            systemId,
            generationEnergyWh: null,
            consumptionEnergyWh: null,
            peakGenerationPowerWatts: null,
            firstObservationAtEpochMillis: null,
            lastObservationAtEpochMillis: null,
            coverageBasisPoints: 0,
            monthly: [],
          };
        throw new Error(`Unexpected request: ${url}`);
      })();
      return new Response(JSON.stringify(body), {
        status: 200,
        headers: { "content-type": "application/json" },
      });
    }),
  );
}

function renderWithApp(element: ReactElement) {
  render(
    <QueryClientProvider
      client={
        new QueryClient({ defaultOptions: { queries: { retry: false } } })
      }
    >
      <MemoryRouter>{element}</MemoryRouter>
    </QueryClientProvider>,
  );
}

beforeEach(async () => {
  window.localStorage.clear();
  await i18n.changeLanguage("en");
});

describe("dashboard without data", () => {
  it("shows the next step instead of a live indicator and zero values", async () => {
    installBackend({
      items: withItems(
        { key: "data_source", state: "recommended" },
        { key: "first_data", state: "blocked" },
      ),
    });
    renderWithApp(<DashboardPage />);

    expect(
      await screen.findByRole("heading", { name: "No data yet" }),
    ).toBeVisible();
    expect(
      await screen.findByRole("link", { name: "Connect a data source" }),
    ).toHaveAttribute("href", `/systems/${systemId}/settings/data-sources`);
    expect(screen.queryByText(/Live data received/)).not.toBeInTheDocument();
    expect(screen.queryByText(/Last data arrived/)).not.toBeInTheDocument();
    expect(screen.queryByText("0 W")).not.toBeInTheDocument();
    expect(screen.queryByText("Generation")).not.toBeInTheDocument();
  });

  it("says a connected source has not delivered yet", async () => {
    installBackend({
      items: withItems({ key: "first_data", state: "recommended" }),
    });
    renderWithApp(<DashboardPage />);

    expect(
      await screen.findByRole("link", { name: "Check the data source" }),
    ).toHaveAttribute("href", `/systems/${systemId}/settings/data-sources`);
  });

  it("offers to add a system to someone who may manage systems", async () => {
    installBackend({ systemIds: [] });
    renderWithApp(<DashboardPage />);

    expect(
      await screen.findByRole("heading", { name: "No system yet" }),
    ).toBeVisible();
    expect(screen.getByRole("link", { name: "Add system" })).toHaveAttribute(
      "href",
      "/setup",
    );
    expect(screen.queryByText(/Live data received/)).not.toBeInTheDocument();
  });

  it("explains the empty account without an action nobody may take", async () => {
    installBackend({ systemIds: [], permissions: ["system_read"] });
    renderWithApp(<DashboardPage />);

    expect(
      await screen.findByText(/No system has been shared with you yet/),
    ).toBeVisible();
    expect(
      screen.queryByRole("link", { name: "Add system" }),
    ).not.toBeInTheDocument();
  });
});

describe("reports without data", () => {
  it("explains that no system exists", async () => {
    installBackend({ systemIds: [] });
    renderWithApp(<StatisticsPage />);

    expect(
      await screen.findByRole("heading", { name: "No system yet" }),
    ).toBeVisible();
    expect(
      await within(screen.getByRole("main")).findByRole("link", {
        name: "Add system",
      }),
    ).toHaveAttribute("href", "/setup");
  });

  it("shows the no-data state instead of empty totals", async () => {
    installBackend({
      items: withItems({ key: "data_source", state: "recommended" }),
    });
    renderWithApp(<StatisticsPage />);

    expect(
      await screen.findByRole("heading", { name: "No data yet" }),
    ).toBeVisible();
    expect(screen.queryByText("Total generation")).not.toBeInTheDocument();
  });
});

describe("setup checklist", () => {
  it("links every open item to the place where it is completed", async () => {
    installBackend({
      items: withItems(
        {
          key: "data_source",
          state: "recommended",
          reason: "data_source_missing",
        },
        {
          key: "channels",
          state: "optional",
          reason: "unassigned_channels",
          count: 2,
        },
        { key: "tariff", state: "optional", reason: "tariff_missing" },
      ),
    });
    renderWithApp(<SetupChecklist systemId={systemId} />);

    const list = within(
      await screen.findByRole("list", { name: "Finish setting up" }),
    );
    expect(list.getAllByRole("listitem")).toHaveLength(3);
    expect(list.getByText("Next step")).toBeVisible();
    expect(
      list.getByText(
        "2 measurement channels deliver values but are not assigned to any equipment.",
      ),
    ).toBeVisible();
    for (const [name, section] of [
      ["Open: Data source", "data-sources"],
      ["Open: Measurement channels", "channels"],
      ["Open: Tariff", "tariffs"],
    ] as const)
      expect(list.getByRole("link", { name })).toHaveAttribute(
        "href",
        `/systems/${systemId}/settings/${section}`,
      );
  });

  it("is hidden once every item is done", async () => {
    installBackend({ dataState: "current" });
    renderWithApp(<SetupChecklist systemId={systemId} />);

    await vi.waitFor(() => {
      expect(fetch).toHaveBeenCalled();
    });
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(screen.queryByRole("list")).not.toBeInTheDocument();
    expect(screen.queryByText("Finish setting up")).not.toBeInTheDocument();
  });

  it("is hidden once the remaining items are dismissed, and stays hidden", async () => {
    installBackend({
      dataState: "current",
      items: withItems({
        key: "tariff",
        state: "optional",
        reason: "tariff_missing",
      }),
    });
    const user = userEvent.setup();
    const { unmount } = render(
      <QueryClientProvider client={new QueryClient()}>
        <MemoryRouter>
          <SetupChecklist systemId={systemId} />
        </MemoryRouter>
      </QueryClientProvider>,
    );

    await user.click(
      await screen.findByRole("button", { name: "Hide: Tariff" }),
    );
    expect(screen.queryByText("Finish setting up")).not.toBeInTheDocument();

    unmount();
    renderWithApp(<SetupChecklist systemId={systemId} />);
    await vi.waitFor(() => {
      expect(fetch).toHaveBeenCalledTimes(2);
    });
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(screen.queryByText("Finish setting up")).not.toBeInTheDocument();
  });
});
