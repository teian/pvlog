import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter, Route, Routes } from "react-router";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { SystemSettingsPage } from "@/pages/SystemSettingsPage";
import i18n from "@/shared/lib/i18n";

const accountId = "019505c8-7c85-7f0b-9bc3-2a3c4d5e6f70";
const systemId = "019505c8-7c85-7f0b-9bc3-2a3c4d5e6f71";
const proposalId = "019505c8-7c85-7f0b-9bc3-2a3c4d5e6f72";
const inverterId = "019505c8-7c85-7f0b-9bc3-2a3c4d5e6f73";
const eastId = "019505c8-7c85-7f0b-9bc3-2a3c4d5e6f74";
const fifthId = "019505c8-7c85-7f0b-9bc3-2a3c4d5e6f75";
const serial = "2000123456";

function json(value: unknown, status = 200) {
  return new Response(value === null ? null : JSON.stringify(value), {
    status,
    headers: { "content-type": "application/json" },
  });
}

function channel(index: number, key: string, unit: string) {
  return {
    id: `019505c8-7c85-7f0b-9bc3-2a3c4d5e6f8${index}`,
    channelKey: `inv.${serial}.${key}`,
    displayName: `inv.${serial}.${key}`,
    dataType: "integer",
    unit,
    scale: 0,
    aggregation: "mean",
    targetKind: "unassigned",
    targetId: null,
    discoveredAtEpochMillis: 1_800_000_000_000,
    lastValueAtEpochMillis: 1_800_000_000_000,
    lastValue: 800,
  };
}

interface AdoptedString {
  tracker: number | null;
  name: string;
  capacityWatts: number;
}

interface Backend {
  requests: { method: string; url: string; body: unknown }[];
  proposalState: "open" | "dismissed" | "adopted";
  channels: Record<string, unknown>[];
  inverters: Record<string, unknown>[];
}

/** A backend that adopts the way the server does: it creates the inverter with its strings and binds the device's channels. */
function installBackend(permissions: string[]): Backend {
  const backend: Backend = {
    requests: [],
    proposalState: "open",
    channels: [
      channel(0, "ac.l1.power", "W"),
      channel(1, "mppt.1.power", "W"),
      channel(2, "mppt.1.voltage", "mV"),
      channel(3, "mppt.5.power", "W"),
    ],
    inverters: [],
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
      const system = `/api/v1/systems/${systemId}`;
      if (url === `${system}/inverters`) return json(backend.inverters);
      if (url === `${system}/channels`) return json(backend.channels);
      const proposals = `${system}/discovery-proposals`;
      if (url === `${proposals}?includeDismissed=true`)
        return json(
          backend.proposalState === "adopted"
            ? []
            : [
                {
                  id: proposalId,
                  deviceSerial: serial,
                  reportedName: null,
                  model: "STP 8.0",
                  firmware: null,
                  state: backend.proposalState,
                  trackers: [1, 5],
                  channelCount: 4,
                  firstSeenAtEpochMillis: 1_800_000_000_000,
                  lastSeenAtEpochMillis: 1_800_000_000_000,
                },
              ],
        );
      if (url === `${proposals}/${proposalId}/dismiss` && method === "POST") {
        backend.proposalState = "dismissed";
        return json(null, 204);
      }
      if (url === `${proposals}/${proposalId}/reset` && method === "POST") {
        backend.proposalState = "open";
        return json(null, 204);
      }
      if (url === `${proposals}/${proposalId}/adopt` && method === "POST") {
        const strings = (body as { strings: AdoptedString[] }).strings.map(
          (string) => ({
            stringId: string.tracker === 1 ? eastId : fifthId,
            tracker: string.tracker,
            name: string.name,
          }),
        );
        backend.proposalState = "adopted";
        backend.inverters = [
          {
            id: inverterId,
            name: (body as { name: string }).name,
            strings: strings.map((string) => ({
              id: string.stringId,
              name: string.name,
            })),
          },
        ];
        backend.channels = backend.channels.map((entry) => {
          const tracker = /\.mppt\.(\d+)\./u.exec(String(entry.channelKey));
          const target = strings.find(
            (string) => String(string.tracker) === tracker?.[1],
          );
          return target
            ? { ...entry, targetKind: "string", targetId: target.stringId }
            : { ...entry, targetKind: "inverter", targetId: inverterId };
        });
        return json(
          { inverterId, strings, boundChannels: backend.channels.length },
          201,
        );
      }
      throw new Error(`Unexpected request: ${method} ${url}`);
    }),
  );
  return backend;
}

function renderChannels() {
  render(
    <QueryClientProvider
      client={
        new QueryClient({ defaultOptions: { queries: { retry: false } } })
      }
    >
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
}

async function proposal() {
  return screen.findByRole("group", { name: `Inverter ${serial}` });
}

describe("discovered equipment in the measurement channels section", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("en");
  });

  it("offers a reported device with the DC inputs that delivered power", async () => {
    installBackend(["system_manage"]);
    renderChannels();

    const card = await proposal();
    expect(
      screen.getByRole("heading", { name: "Discovered equipment" }),
    ).toBeVisible();
    expect(within(card).getByText("4 measurement channels")).toBeVisible();
    expect(within(card).getByText(serial)).toBeVisible();
    expect(within(card).getByText("STP 8.0")).toBeVisible();
    expect(within(card).getByText("DC input 1")).toBeVisible();
    expect(within(card).getByText("DC input 5")).toBeVisible();
    expect(within(card).getByLabelText("Inverter name")).toHaveValue(
      `Inverter ${serial}`,
    );
    expect(
      within(card)
        .getAllByLabelText("String name")
        .map((input) => (input as HTMLInputElement).value),
    ).toEqual(["MPPT 1", "MPPT 5"]);
  });

  it("adopting creates the inverter and strings and binds their measurement channels", async () => {
    const backend = installBackend(["system_manage"]);
    const user = userEvent.setup();
    renderChannels();

    const card = await proposal();
    const names = within(card).getAllByLabelText("String name");
    const capacities = within(card).getAllByLabelText("Capacity (kWp)");
    await user.clear(names[0] as HTMLElement);
    await user.type(names[0] as HTMLElement, "East");
    await user.type(capacities[0] as HTMLElement, "4,4");
    await user.type(capacities[1] as HTMLElement, "3");
    await user.click(within(card).getByRole("button", { name: "Adopt" }));

    expect(backend.requests).toEqual([
      {
        method: "POST",
        url: `/api/v1/systems/${systemId}/discovery-proposals/${proposalId}/adopt`,
        body: {
          name: `Inverter ${serial}`,
          strings: [
            { tracker: 1, name: "East", capacityWatts: 4400 },
            { tracker: 5, name: "MPPT 5", capacityWatts: 3000 },
          ],
        },
      },
    ]);
    // The device is equipment now: its channels sit at the new inverter and
    // strings, and nothing is left to propose or to assign.
    expect(
      await screen.findByLabelText(`What inv.${serial}.mppt.1.power measures`),
    ).toHaveValue(`string:${eastId}`);
    expect(
      screen.getByLabelText(`What inv.${serial}.mppt.1.voltage measures`),
    ).toHaveValue(`string:${eastId}`);
    expect(
      screen.getByLabelText(`What inv.${serial}.mppt.5.power measures`),
    ).toHaveValue(`string:${fifthId}`);
    expect(
      screen.getByLabelText(`What inv.${serial}.ac.l1.power measures`),
    ).toHaveValue(`inverter:${inverterId}`);
    expect(
      within(
        screen.getByLabelText(`What inv.${serial}.mppt.5.power measures`),
      ).getByRole("option", { name: "String MPPT 5" }),
    ).toBeInTheDocument();
    expect(
      screen.queryByRole("heading", { name: "Discovered equipment" }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByText(/measurement channels are not assigned/u),
    ).not.toBeInTheDocument();
  });

  it("asks for a capacity before adopting", async () => {
    const backend = installBackend(["system_manage"]);
    const user = userEvent.setup();
    renderChannels();

    const card = await proposal();
    await user.type(
      within(card).getAllByLabelText("Capacity (kWp)")[0] as HTMLElement,
      "4.4",
    );
    await user.click(within(card).getByRole("button", { name: "Adopt" }));

    expect(within(card).getByRole("alert")).toHaveTextContent(
      "Enter a name and a positive capacity for the inverter's strings.",
    );
    expect(backend.requests).toEqual([]);
  });

  it("dismisses a device and proposes it again on request", async () => {
    const backend = installBackend(["system_manage"]);
    const user = userEvent.setup();
    renderChannels();

    await user.click(
      within(await proposal()).getByRole("button", { name: "Dismiss" }),
    );
    expect(
      await screen.findByText(
        `Inverter ${serial} was dismissed and is not proposed any more.`,
      ),
    ).toBeVisible();
    expect(
      screen.queryByRole("group", { name: `Inverter ${serial}` }),
    ).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Propose again" }));
    expect(await proposal()).toBeVisible();
    expect(backend.requests.map((request) => request.url)).toEqual([
      `/api/v1/systems/${systemId}/discovery-proposals/${proposalId}/dismiss`,
      `/api/v1/systems/${systemId}/discovery-proposals/${proposalId}/reset`,
    ]);
  });

  it("shows the device without actions to someone who may not manage the system", async () => {
    installBackend([]);
    renderChannels();

    const card = await proposal();
    expect(within(card).getByText(serial)).toBeVisible();
    expect(
      within(card).queryByRole("button", { name: "Adopt" }),
    ).not.toBeInTheDocument();
    expect(
      within(card).queryByRole("button", { name: "Dismiss" }),
    ).not.toBeInTheDocument();
  });
});
