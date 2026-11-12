import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter, Route, Routes } from "react-router";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { SystemSettingsPage } from "@/pages/SystemSettingsPage";
import i18n from "@/shared/lib/i18n";

const accountId = "019505c8-7c85-7f0b-9bc3-2a3c4d5e6f70";
const systemId = "019505c8-7c85-7f0b-9bc3-2a3c4d5e6f71";
const inverterId = "019505c8-7c85-7f0b-9bc3-2a3c4d5e6f72";
const stringId = "019505c8-7c85-7f0b-9bc3-2a3c4d5e6f73";

function json(value: unknown, status = 200) {
  return new Response(value === null ? null : JSON.stringify(value), {
    status,
    headers: { "content-type": "application/json" },
  });
}

function channel(overrides: Record<string, unknown>) {
  return {
    id: "019505c8-7c85-7f0b-9bc3-2a3c4d5e6f80",
    channelKey: "mppt-1",
    displayName: "mppt-1",
    dataType: "integer",
    unit: "W",
    scale: 0,
    aggregation: "mean",
    targetKind: "unassigned",
    targetId: null,
    discoveredAtEpochMillis: 1_800_000_000_000,
    lastValueAtEpochMillis: 1_800_000_000_000,
    lastValue: 3100,
    ...overrides,
  };
}

interface Backend {
  requests: { method: string; url: string; body: unknown }[];
  channels: Record<string, unknown>[];
}

function installBackend(permissions: string[]): Backend {
  const backend: Backend = {
    requests: [],
    channels: [
      channel({}),
      channel({
        id: "019505c8-7c85-7f0b-9bc3-2a3c4d5e6f81",
        channelKey: "meter",
        displayName: "House meter",
        targetKind: "plant",
        lastValue: null,
        lastValueAtEpochMillis: null,
      }),
    ],
  };
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: string | URL | Request, init?: RequestInit) => {
      const url = input instanceof Request ? input.url : input.toString();
      const method = init?.method ?? "GET";
      const body =
        typeof init?.body === "string" ? JSON.parse(init.body) : null;
      if (method !== "GET") backend.requests.push({ method, url, body });
      if (url === "/api/v1/session")
        return json({
          authenticated: true,
          user: { id: accountId, displayName: "Ada" },
          accountId,
          systemIds: [systemId],
          permissions,
          connectors: [],
        });
      if (url === `/api/v1/systems/${systemId}/inverters`)
        return json([
          {
            id: inverterId,
            name: "Roof inverter",
            strings: [{ id: stringId, name: "East" }],
          },
        ]);
      if (url.endsWith("/discovery-proposals?includeDismissed=true"))
        return json([]);
      if (url === "/api/v1/data-source-profiles")
        return json([
          {
            id: "smalog",
            displayName: "smalog",
            issuesCredential: true,
            snippetKinds: [],
            channelKeys: [
              { key: "inv.{serial}.ac.l{phase}.power" },
              {
                key: "inv.{serial}.status",
                statusLabels: [
                  { code: 307, label: "Ok" },
                  { code: 455, label: "Warning" },
                ],
              },
            ],
          },
        ]);
      const collection = `/api/v1/systems/${systemId}/channels`;
      if (url === collection) return json(backend.channels);
      if (url === `${collection}/mppt-1` && method === "PUT") {
        backend.channels = backend.channels.map((entry) =>
          entry.channelKey === "mppt-1" ? { ...entry, ...body } : entry,
        );
        return json(backend.channels[0]);
      }
      if (url === `${collection}/mppt-1` && method === "DELETE") {
        backend.channels = backend.channels.filter(
          (entry) => entry.channelKey !== "mppt-1",
        );
        return json(null, 204);
      }
      throw new Error(`Unexpected request: ${method} ${url}`);
    }),
  );
  return backend;
}

function renderChannels() {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  render(
    <QueryClientProvider client={client}>
      <MemoryRouter initialEntries={[`/systems/${systemId}/settings/channels`]}>
        <Routes>
          <Route
            element={<SystemSettingsPage />}
            path="/systems/:systemId/settings/:section?"
          />
        </Routes>
      </MemoryRouter>
    </QueryClientProvider>,
  );
  return client;
}

describe("measurement channels section", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("en");
  });

  it("lists channels with their last value and highlights unassigned ones first", async () => {
    installBackend(["system_manage"]);
    renderChannels();

    expect(
      await screen.findByText("1 measurement channel is not assigned"),
    ).toBeVisible();
    const rows = screen.getAllByRole("row").slice(1);
    expect(within(rows[0] as HTMLElement).getByText("mppt-1")).toBeVisible();
    expect(within(rows[0] as HTMLElement).getByText("3,100 W")).toBeVisible();
    expect(
      within(rows[0] as HTMLElement).getByText("Not assigned", {
        selector: "[data-slot='badge']",
      }),
    ).toBeVisible();
    expect(
      within(rows[1] as HTMLElement).getByText("No value yet"),
    ).toBeVisible();
  });

  it("shows a decimal channel at its scale without trailing zeros", async () => {
    const backend = installBackend(["system_manage"]);
    backend.channels.push(
      channel({
        id: "019505c8-7c85-7f0b-9bc3-2a3c4d5e6f82",
        channelKey: "pvoutput.v7",
        displayName: "DC voltage",
        dataType: "decimal",
        unit: "V",
        scale: 3,
        targetKind: "plant",
        lastValue: 12_340,
      }),
      channel({
        id: "019505c8-7c85-7f0b-9bc3-2a3c4d5e6f83",
        channelKey: "counter",
        displayName: "Counter",
        dataType: "decimal",
        unit: "",
        scale: 3,
        targetKind: "plant",
        lastValue: 7_000,
      }),
    );
    renderChannels();

    expect(await screen.findByText("12.34 V")).toBeVisible();
    expect(screen.getByText("7")).toBeVisible();
  });

  it("shows a status code with the meaning the source profile gives it", async () => {
    const backend = installBackend(["system_manage"]);
    backend.channels.push(
      channel({
        id: "019505c8-7c85-7f0b-9bc3-2a3c4d5e6f84",
        channelKey: "inv.2000123456.status",
        displayName: "Device status",
        unit: "code",
        aggregation: "last",
        targetKind: "plant",
        lastValue: 455,
      }),
      channel({
        id: "019505c8-7c85-7f0b-9bc3-2a3c4d5e6f85",
        channelKey: "inv.3000654321.status",
        displayName: "Garage status",
        unit: "code",
        aggregation: "last",
        targetKind: "plant",
        lastValue: 999,
      }),
    );
    renderChannels();

    expect(await screen.findByText("Warning (455)")).toBeVisible();
    // A code the profile does not list is shown as its number.
    expect(screen.getByText("999")).toBeVisible();

    await i18n.changeLanguage("de");
    expect(await screen.findByText("Warnung (455)")).toBeVisible();
  });

  it("binds an unassigned channel to a string", async () => {
    const backend = installBackend(["system_manage"]);
    const user = userEvent.setup();
    const client = renderChannels();
    const invalidated = vi.spyOn(client, "invalidateQueries");

    await user.selectOptions(
      await screen.findByLabelText("What mppt-1 measures"),
      "String East",
    );

    expect(backend.requests[0]).toEqual({
      method: "PUT",
      url: `/api/v1/systems/${systemId}/channels/mppt-1`,
      body: {
        targetKind: "string",
        targetId: stringId,
        displayName: "mppt-1",
      },
    });
    // The channel is now listed under its string and no longer counted.
    expect(await screen.findByLabelText("What mppt-1 measures")).toHaveValue(
      `string:${stringId}`,
    );
    expect(
      screen.queryByText("1 measurement channel is not assigned"),
    ).not.toBeInTheDocument();
    expect(invalidated).toHaveBeenCalledWith({
      queryKey: ["system", systemId, "setup-status"],
    });
  });

  it("renames a channel", async () => {
    const backend = installBackend(["system_manage"]);
    const user = userEvent.setup();
    renderChannels();

    const name = await screen.findByLabelText(
      "Name of measurement channel mppt-1",
    );
    await user.clear(name);
    await user.type(name, "East roof");
    await user.click(screen.getByRole("button", { name: "Save" }));

    expect(backend.requests[0]?.body).toMatchObject({
      displayName: "East roof",
      targetKind: "unassigned",
      targetId: null,
    });
  });

  it("retires a channel after confirmation", async () => {
    const backend = installBackend(["system_manage"]);
    const user = userEvent.setup();
    renderChannels();

    await user.click(
      (await screen.findAllByRole("button", { name: "Retire" }))[0]!,
    );
    const dialog = await screen.findByRole("alertdialog");
    expect(within(dialog).getByText(/already stored are kept/)).toBeVisible();
    expect(backend.requests).toHaveLength(0);
    await user.click(
      within(dialog).getByRole("button", {
        name: "Retire measurement channel",
      }),
    );

    expect(backend.requests[0]).toMatchObject({
      method: "DELETE",
      url: `/api/v1/systems/${systemId}/channels/mppt-1`,
    });
  });

  it("offers no changes without management permission", async () => {
    installBackend(["system_read"]);
    renderChannels();

    expect(await screen.findByLabelText("What mppt-1 measures")).toBeDisabled();
    expect(
      screen.queryByRole("button", { name: "Retire" }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByLabelText("Name of measurement channel mppt-1"),
    ).not.toBeInTheDocument();
  });
});
