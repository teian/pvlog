import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter, Route, Routes } from "react-router";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { SystemSettingsPage } from "@/pages/SystemSettingsPage";
import i18n from "@/shared/lib/i18n";

const accountId = "019505c8-7c85-7f0b-9bc3-2a3c4d5e6f70";
const systemId = "019505c8-7c85-7f0b-9bc3-2a3c4d5e6f71";
const sourceId = "019505c8-7c85-7f0b-9bc3-2a3c4d5e6f72";

function json(value: unknown, status = 200) {
  return new Response(value === null ? null : JSON.stringify(value), {
    status,
    headers: { "content-type": "application/json" },
  });
}

function systemRecord(overrides: Record<string, unknown> = {}) {
  return {
    id: systemId,
    accountId,
    name: "Roof",
    description: "Garage",
    commissioningDate: "2019-04-15",
    timezone: "Europe/Berlin",
    locationLabel: "Leipzig, Germany",
    countryCode: "DE",
    latitudeMicrodegrees: 51_340_000,
    longitudeMicrodegrees: 12_370_000,
    statusIntervalSeconds: 300,
    declaredCapacityWatts: 9_900,
    derivedCapacityWatts: null,
    capacityWatts: 9_900,
    capacitySource: "declared",
    visibility: "private",
    lifecycle: "active",
    version: 3,
    createdAt: 1,
    updatedAt: 1,
    ...overrides,
  };
}

function source(state: string, name = "Roof logger") {
  const seen = state === "never_connected" ? null : 1_800_000_000_000;
  return {
    id: sourceId,
    systemId,
    name,
    profile: "smalog",
    state,
    serverUrl: "https://pvlog.example",
    credentialId: "019505c8-7c85-7f0b-9bc3-2a3c4d5e6f73",
    lastReadingAtEpochMillis: seen,
    lastReceivedAtEpochMillis: seen,
    readingsLastDay: 0,
    createdAtEpochMillis: 1,
    updatedAtEpochMillis: 1,
  };
}

interface RecordedRequest {
  method: string;
  url: string;
  body: unknown;
  ifMatch: string | null;
}

interface Backend {
  requests: RecordedRequest[];
  record: Record<string, unknown>;
  sources: Record<string, unknown>[];
}

function installBackend(permissions: string[], state = "stale"): Backend {
  const backend: Backend = {
    requests: [],
    record: systemRecord(),
    sources: [source(state)],
  };
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: string | URL | Request, init?: RequestInit) => {
      const url = input instanceof Request ? input.url : input.toString();
      const method = init?.method ?? "GET";
      if (method !== "GET")
        backend.requests.push({
          method,
          url,
          body: typeof init?.body === "string" ? JSON.parse(init.body) : null,
          ifMatch: new Headers(init?.headers).get("if-match"),
        });
      if (url === "/api/v1/session")
        return json({
          authenticated: true,
          user: { id: accountId, displayName: "Ada" },
          accountId,
          systemIds: [systemId],
          permissions,
          connectors: [],
        });
      if (url === `/api/v1/systems/${systemId}`) {
        if (method === "PUT")
          backend.record = systemRecord({
            ...(backend.requests.at(-1)?.body as object),
            version: 4,
          });
        return json(backend.record);
      }
      if (url === "/api/v1/data-source-profiles")
        return json([
          {
            id: "smalog",
            displayName: "smalog",
            issuesCredential: true,
            snippetKinds: ["smalog-config"],
          },
        ]);
      const collection = `/api/v1/systems/${systemId}/data-sources`;
      if (url === collection && method === "GET") return json(backend.sources);
      if (url === `${collection}/${sourceId}/credential`)
        return json({
          dataSource: source("never_connected"),
          configuration: {
            serverUrl: "https://pvlog.example",
            systemId,
            apiKey: "pvlog_regenerated",
            snippets: [],
          },
        });
      if (url === `${collection}/${sourceId}` && method === "DELETE") {
        backend.sources = [];
        return json(null, 204);
      }
      if (url.startsWith("/api/v1/geocoding/search?"))
        return json([
          {
            displayName: "Halle, Germany",
            latitude: 51.48,
            longitude: 11.97,
            countryCode: "DE",
            attribution: "© OpenStreetMap contributors",
          },
        ]);
      throw new Error(`Unexpected request: ${method} ${url}`);
    }),
  );
  return backend;
}

function renderSettings(section: string) {
  render(
    <QueryClientProvider
      client={
        new QueryClient({ defaultOptions: { queries: { retry: false } } })
      }
    >
      <MemoryRouter
        initialEntries={[`/systems/${systemId}/settings/${section}`]}
      >
        <Routes>
          <Route
            element={<SystemSettingsPage />}
            path="/systems/:systemId/settings/:section?"
          />
        </Routes>
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

describe("system settings", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("en");
  });

  it("offers each section under its own address", async () => {
    installBackend(["system_manage"]);
    renderSettings("general");
    const sections = within(
      await screen.findByRole("navigation", { name: "Settings sections" }),
    );
    expect(sections.getByRole("link", { name: "General" })).toHaveAttribute(
      "href",
      `/systems/${systemId}/settings/general`,
    );
    expect(
      sections.getByRole("link", { name: "Data sources" }),
    ).toHaveAttribute("href", `/systems/${systemId}/settings/data-sources`);
  });

  it("edits every general field and keeps the stored location", async () => {
    const backend = installBackend(["system_manage"]);
    const user = userEvent.setup();
    renderSettings("general");

    const name = await screen.findByLabelText("System name");
    expect(name).toHaveValue("Roof");
    expect(screen.getByLabelText("Location")).toHaveValue("Leipzig, Germany");
    expect(screen.getByLabelText("Timezone")).toHaveValue("Europe/Berlin");
    expect(screen.getByLabelText("Seconds between readings")).toHaveValue(
      "300",
    );
    expect(screen.getByLabelText("Commissioning date")).toHaveValue(
      "2019-04-15",
    );
    expect(screen.getByLabelText("Installed capacity (kWp)")).toHaveValue(
      "9.9",
    );

    await user.clear(name);
    await user.type(name, "South roof");
    await user.clear(screen.getByLabelText("Seconds between readings"));
    await user.type(screen.getByLabelText("Seconds between readings"), "60");
    await user.click(screen.getByRole("button", { name: "Save changes" }));

    expect(await screen.findByText("Changes saved.")).toBeVisible();
    const [request] = backend.requests;
    expect(request?.ifMatch).toBe('"3"');
    expect(request?.body).toEqual({
      name: "South roof",
      timezone: "Europe/Berlin",
      visibility: "private",
      description: "Garage",
      commissioningDate: "2019-04-15",
      statusIntervalSeconds: 60,
      declaredCapacityWatts: 9900,
      locationLabel: "Leipzig, Germany",
      countryCode: "DE",
      latitudeMicrodegrees: 51_340_000,
      longitudeMicrodegrees: 12_370_000,
    });
  });

  it("saves a newly chosen location with its coordinates and country", async () => {
    const backend = installBackend(["system_manage"]);
    const user = userEvent.setup();
    renderSettings("general");

    const location = await screen.findByLabelText("Location");
    await user.clear(location);
    await user.type(location, "Halle");
    await user.click(await screen.findByRole("option", { name: /Halle/ }));
    await user.click(screen.getByRole("button", { name: "Save changes" }));

    await screen.findByText("Changes saved.");
    expect(backend.requests[0]?.body).toMatchObject({
      locationLabel: "Halle, Germany",
      countryCode: "DE",
      latitudeMicrodegrees: 51_480_000,
      longitudeMicrodegrees: 11_970_000,
    });
  });

  it("names an invalid field instead of saving", async () => {
    const backend = installBackend(["system_manage"]);
    const user = userEvent.setup();
    renderSettings("general");

    await user.clear(await screen.findByLabelText("System name"));
    await user.click(screen.getByRole("button", { name: "Save changes" }));

    expect(screen.getByText("Enter a name for the system.")).toBeVisible();
    expect(backend.requests).toHaveLength(0);
  });

  it("shows the sections read-only without management permission", async () => {
    installBackend(["system_read"]);
    renderSettings("general");

    expect(await screen.findByLabelText("System name")).toBeDisabled();
    expect(
      screen.getByText("You can view these settings but not change them."),
    ).toBeVisible();
    expect(
      screen.queryByRole("button", { name: "Save changes" }),
    ).not.toBeInTheDocument();
  });
});

describe("data sources section", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("en");
  });

  it("marks a stale source and links to troubleshooting", async () => {
    installBackend(["system_manage"], "stale");
    renderSettings("data-sources");

    const row = (await screen.findByText("Roof logger")).closest("tr");
    expect(row).not.toBeNull();
    const cells = within(row as HTMLElement);
    expect(cells.getByText("No recent data")).toBeVisible();
    expect(cells.getByRole("link", { name: "Troubleshooting" })).toBeVisible();
    expect(cells.queryByText("No reading yet")).not.toBeInTheDocument();
  });

  it("warns before regenerating and then shows the new key once", async () => {
    const backend = installBackend(["system_manage"]);
    const user = userEvent.setup();
    renderSettings("data-sources");

    await user.click(
      await screen.findByRole("button", { name: "Regenerate key" }),
    );
    const dialog = await screen.findByRole("alertdialog");
    expect(within(dialog).getByText(/stops working immediately/)).toBeVisible();
    expect(backend.requests).toHaveLength(0);

    await user.click(
      within(dialog).getByRole("button", { name: "Regenerate key" }),
    );
    expect(await screen.findByRole("textbox", { name: "API key" })).toHaveValue(
      "pvlog_regenerated",
    );
    expect(backend.requests[0]?.url).toBe(
      `/api/v1/systems/${systemId}/data-sources/${sourceId}/credential`,
    );

    // Closing the dialog drops the key for good.
    await user.click(screen.getByRole("button", { name: "Close" }));
    expect(
      screen.queryByRole("textbox", { name: "API key" }),
    ).not.toBeInTheDocument();
  });

  it("confirms before deleting a source", async () => {
    const backend = installBackend(["system_manage"]);
    const user = userEvent.setup();
    renderSettings("data-sources");

    await user.click(await screen.findByRole("button", { name: "Delete" }));
    const dialog = await screen.findByRole("alertdialog");
    expect(
      within(dialog).getByText(/already delivered are kept/),
    ).toBeVisible();
    await user.click(
      within(dialog).getByRole("button", { name: "Delete data source" }),
    );

    expect(await screen.findByText("No data source yet")).toBeVisible();
    expect(backend.requests[0]).toMatchObject({
      method: "DELETE",
      url: `/api/v1/systems/${systemId}/data-sources/${sourceId}`,
    });
  });

  it("offers no actions without management permission", async () => {
    installBackend(["system_read"]);
    renderSettings("data-sources");

    expect(await screen.findByText("Roof logger")).toBeVisible();
    for (const name of [
      "Regenerate key",
      "Delete",
      "Rename",
      "Add data source",
    ])
      expect(screen.queryByRole("button", { name })).not.toBeInTheDocument();
    expect(
      screen.queryByRole("columnheader", { name: "Actions" }),
    ).not.toBeInTheDocument();
  });
});
