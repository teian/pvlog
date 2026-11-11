import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter, Route, Routes } from "react-router";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { HomePage } from "@/pages/HomePage";
import { SetupPage } from "@/pages/SetupPage";
import i18n from "@/shared/lib/i18n";

const accountId = "019505c8-7c85-7f0b-9bc3-2a3c4d5e6f70";
const systemId = "019505c8-7c85-7f0b-9bc3-2a3c4d5e6f71";
const sourceId = "019505c8-7c85-7f0b-9bc3-2a3c4d5e6f72";
const apiKey = "pvlog_one-time-secret";

function json(value: unknown, status = 200) {
  return new Response(JSON.stringify(value), {
    status,
    headers: { "content-type": "application/json" },
  });
}

function session(systemIds: string[], permissions = ["system_manage"]) {
  return {
    authenticated: true,
    user: { id: accountId, displayName: "Ada" },
    accountId,
    systemIds,
    permissions,
    connectors: [],
  };
}

function systemRecord(name: string) {
  return {
    id: systemId,
    accountId,
    name,
    description: "",
    commissioningDate: null,
    timezone: "Europe/Berlin",
    locationLabel: null,
    countryCode: null,
    latitudeMicrodegrees: null,
    longitudeMicrodegrees: null,
    statusIntervalSeconds: 300,
    declaredCapacityWatts: null,
    derivedCapacityWatts: null,
    capacityWatts: null,
    capacitySource: null,
    visibility: "private",
    lifecycle: "active",
    version: 1,
    createdAt: 1,
    updatedAt: 1,
  };
}

function dataSource() {
  return {
    id: sourceId,
    systemId,
    name: "smalog",
    profile: "smalog",
    state: "never_connected",
    serverUrl: "https://pvlog.example",
    credentialId: "019505c8-7c85-7f0b-9bc3-2a3c4d5e6f73",
    lastReadingAtEpochMillis: null,
    lastReceivedAtEpochMillis: null,
    readingsLastDay: 0,
    createdAtEpochMillis: 1,
    updatedAtEpochMillis: 1,
  };
}

function reading(test: boolean) {
  return {
    id: "019505c8-7c85-7f0b-9bc3-2a3c4d5e6f74",
    measuredAtEpochMillis: 1_800_000_000_000,
    receivedAtEpochMillis: 1_800_000_000_000,
    generationPowerWatts: test ? 100 : 4200,
    generationEnergyWh: null,
    consumptionPowerWatts: null,
    consumptionEnergyWh: null,
    dataSourceId: sourceId,
    dataSourceName: "smalog",
    test,
  };
}

const profiles = [
  {
    id: "smalog",
    displayName: "smalog",
    issuesCredential: true,
    snippetKinds: ["smalog-config", "environment"],
  },
  {
    id: "generic",
    displayName: "Generic uploader",
    issuesCredential: true,
    snippetKinds: ["curl"],
  },
];

interface Backend {
  /** Bodies of every state-changing request, by "METHOD path". */
  requests: Record<string, unknown[]>;
  /** What the first-data check answers, advanced by the test. */
  firstData: "none" | "test" | "received";
}

function installBackend(): Backend {
  const backend: Backend = { requests: {}, firstData: "none" };
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: string | URL | Request, init?: RequestInit) => {
      const url = input instanceof Request ? input.url : input.toString();
      const method = init?.method ?? "GET";
      if (method !== "GET") {
        const key = `${method} ${url}`;
        (backend.requests[key] ??= []).push(
          typeof init?.body === "string" ? JSON.parse(init.body) : null,
        );
      }
      if (url === "/api/v1/session") return json(session([]));
      if (url === "/api/v1/systems" && method === "POST")
        return json(systemRecord("Roof"), 201);
      if (url === "/api/v1/data-source-profiles") return json(profiles);
      if (
        url === `/api/v1/systems/${systemId}/data-sources` &&
        method === "POST"
      )
        return json(
          {
            dataSource: dataSource(),
            configuration: {
              serverUrl: "https://pvlog.example",
              systemId,
              apiKey,
              snippets: [
                {
                  id: "smalog-config",
                  title: "config.toml",
                  language: "toml",
                  content: '[pvlog]\napi_key = "${PVLOG_API_KEY}"\n',
                },
              ],
            },
          },
          201,
        );
      if (url === `/api/v1/systems/${systemId}/first-data`)
        return json({
          systemId,
          received: backend.firstData === "received",
          reading:
            backend.firstData === "none"
              ? null
              : reading(backend.firstData === "test"),
        });
      if (url.endsWith("/test-reading") && method === "POST") {
        return json(
          {
            id: "019505c8-7c85-7f0b-9bc3-2a3c4d5e6f75",
            systemId,
            dataSourceId: sourceId,
            measuredAtEpochMillis: 1,
            receivedAtEpochMillis: 1,
            values: {},
            test: true,
          },
          201,
        );
      }
      throw new Error(`Unexpected request: ${method} ${url}`);
    }),
  );
  return backend;
}

function renderSetup() {
  render(
    <QueryClientProvider
      client={
        new QueryClient({ defaultOptions: { queries: { retry: false } } })
      }
    >
      <MemoryRouter initialEntries={["/setup"]}>
        <Routes>
          <Route element={<SetupPage />} path="/setup" />
          <Route element={<p>system page</p>} path="/systems/:systemId" />
        </Routes>
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

describe("guided system setup", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("en");
  });

  it("creates a system from its name alone", async () => {
    const backend = installBackend();
    const user = userEvent.setup();
    renderSetup();

    // Submitting nothing names the one field that is required.
    await user.click(
      await screen.findByRole("button", { name: "Create system and continue" }),
    );
    expect(screen.getByText("Enter a name for the system.")).toBeVisible();
    expect(backend.requests["POST /api/v1/systems"]).toBeUndefined();

    await user.type(screen.getByLabelText("System name"), "Roof");
    await user.click(
      screen.getByRole("button", { name: "Create system and continue" }),
    );

    expect(
      await screen.findByRole("group", {
        name: "Which logger sends the data?",
      }),
    ).toBeVisible();
    const [body] = backend.requests["POST /api/v1/systems"] ?? [];
    expect(body).toEqual({
      name: "Roof",
      clientTimezone: Intl.DateTimeFormat().resolvedOptions().timeZone,
    });
    expect(screen.getByRole("listitem", { current: "step" })).toHaveTextContent(
      "Data source",
    );
  });

  it("sends optional values only when they were entered", async () => {
    const backend = installBackend();
    const user = userEvent.setup();
    renderSetup();

    await user.type(await screen.findByLabelText("System name"), " Roof ");
    await user.type(screen.getByLabelText("Installed capacity (kWp)"), "9,9");
    await user.click(screen.getByText("More options"));
    await user.type(screen.getByLabelText("Seconds between readings"), "60");
    await user.click(
      screen.getByRole("button", { name: "Create system and continue" }),
    );

    await screen.findByRole("group", { name: "Which logger sends the data?" });
    expect(backend.requests["POST /api/v1/systems"]?.[0]).toMatchObject({
      name: "Roof",
      declaredCapacityWatts: 9900,
      statusIntervalSeconds: 60,
    });
  });

  it("finishes without a data source when connecting is postponed", async () => {
    const backend = installBackend();
    const user = userEvent.setup();
    renderSetup();

    await user.type(await screen.findByLabelText("System name"), "Roof");
    await user.click(
      screen.getByRole("button", { name: "Create system and continue" }),
    );
    await user.click(
      await screen.findByRole("button", { name: "Connect later" }),
    );

    expect(await screen.findByText("system page")).toBeVisible();
    expect(
      backend.requests[`POST /api/v1/systems/${systemId}/data-sources`],
    ).toBeUndefined();
  });

  it("shows the configuration once, then waits for and confirms the first reading", async () => {
    const backend = installBackend();
    const user = userEvent.setup();
    renderSetup();

    await user.type(await screen.findByLabelText("System name"), "Roof");
    await user.click(
      screen.getByRole("button", { name: "Create system and continue" }),
    );
    await user.click(
      await screen.findByRole("button", { name: "Create data source" }),
    );

    // Everything the logger needs, each with its own copy action.
    expect(await screen.findByLabelText("API key")).toHaveValue(apiKey);
    expect(screen.getByLabelText("System ID")).toHaveValue(systemId);
    expect(screen.getByLabelText("Server URL")).toHaveValue(
      "https://pvlog.example",
    );
    expect(screen.getByText("Copy this configuration now")).toBeVisible();
    for (const label of ["API key", "System ID", "Server URL", "config.toml"])
      expect(
        screen.getByRole("button", { name: `Copy ${label}` }),
      ).toBeVisible();
    expect(
      within(screen.getByRole("region", { name: "config.toml" })).getByText(
        /PVLOG_API_KEY/,
      ),
    ).toBeVisible();
    expect(
      backend.requests[`POST /api/v1/systems/${systemId}/data-sources`]?.[0],
    ).toEqual({ name: "smalog", profile: "smalog" });

    await user.click(
      screen.getByRole("button", { name: "I have copied the configuration" }),
    );

    // Nothing has arrived: a live waiting state, not a success.
    expect(await screen.findByRole("status")).toHaveTextContent(
      "Waiting for the first reading…",
    );
    expect(screen.queryByLabelText("API key")).not.toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Finish without data" }),
    ).toBeVisible();

    // A test reading is shown as a test and does not end the wait.
    backend.firstData = "test";
    await user.click(screen.getByRole("button", { name: "Send test reading" }));
    expect(await screen.findByText("Test reading received")).toBeVisible();
    expect(screen.getByRole("status")).toHaveTextContent(
      "Waiting for the first reading…",
    );

    // The logger delivers.
    backend.firstData = "received";
    await user.click(screen.getByRole("button", { name: "Send test reading" }));
    expect(
      await screen.findByText("Your system is receiving data"),
    ).toBeVisible();
    expect(screen.getByText(/4,200/)).toBeVisible();
    expect(screen.getByText("smalog")).toBeVisible();
    expect(screen.queryByRole("status")).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Open system" }));
    expect(await screen.findByText("system page")).toBeVisible();
  });
});

describe("first sign-in", () => {
  function renderHome(systemIds: string[], permissions: string[]) {
    vi.stubGlobal(
      "fetch",
      vi.fn(async (input: string | URL | Request) => {
        const url = input instanceof Request ? input.url : input.toString();
        if (url === "/api/v1/session")
          return json(session(systemIds, permissions));
        return json({
          dataState: "none",
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
        });
      }),
    );
    render(
      <QueryClientProvider
        client={
          new QueryClient({ defaultOptions: { queries: { retry: false } } })
        }
      >
        <MemoryRouter initialEntries={["/"]}>
          <Routes>
            <Route element={<p>guided setup</p>} path="/setup" />
            <Route element={<HomePage />} path="*" />
          </Routes>
        </MemoryRouter>
      </QueryClientProvider>,
    );
  }

  it("opens the setup for someone who may manage systems and has none", async () => {
    renderHome([], ["system_manage"]);
    expect(await screen.findByText("guided setup")).toBeVisible();
  });

  it("keeps the dashboard for someone who cannot create a system", async () => {
    renderHome([], ["system_read"]);
    expect(await screen.findByRole("heading", { level: 1 })).toBeVisible();
    expect(screen.queryByText("guided setup")).not.toBeInTheDocument();
  });

  it("keeps the dashboard for someone who already has a system", async () => {
    renderHome([systemId], ["system_manage"]);
    await waitFor(() => {
      expect(screen.getByRole("heading", { level: 1 })).toBeVisible();
    });
    expect(screen.queryByText("guided setup")).not.toBeInTheDocument();
  });
});
