import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter, Route, Routes } from "react-router";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { AdministrationPage } from "@/pages/AdministrationPage";
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

function storedInverter() {
  return {
    id: inverterId,
    systemId,
    name: "Fronius Symo",
    manufacturer: "Fronius",
    model: "Symo",
    ratedPowerWatts: 8000,
    specificationSnapshot: null,
    effectiveFrom: 1,
    effectiveTo: null,
    version: 1,
    strings: [
      {
        id: stringId,
        inverterId,
        name: "South",
        panelCount: 18,
        panelManufacturer: "Example",
        panelModel: "M450",
        ratedPowerWatts: 8100,
        moduleSpecificationSnapshot: null,
        modulePeakPowerWatts: 450,
        totalPeakPowerWatts: 8100,
        orientationDegrees: 180,
        tiltDegrees: 30,
        temperatureCoefficientPpmPerCelsius: -3400,
        shading: [
          {
            fromMinuteOfDay: 420,
            toMinuteOfDay: 540,
            label: "Chimney",
            lossBasisPoints: 3000,
          },
        ],
        effectiveFrom: 1,
        effectiveTo: null,
      },
    ],
  };
}

interface Backend {
  requests: { method: string; url: string; body: unknown }[];
  inverters: unknown[];
}

function installBackend(permissions: string[], inverters: unknown[]): Backend {
  const backend: Backend = { requests: [], inverters };
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
      const collection = `/api/v1/systems/${systemId}/inverters`;
      if (url === collection && method === "GET")
        return json(backend.inverters);
      if (url === collection && method === "POST") {
        backend.inverters = [storedInverter()];
        return json(storedInverter(), 201);
      }
      if (url === `${collection}/${inverterId}` && method === "PUT")
        return json(storedInverter());
      if (url.startsWith("/api/v1/equipment-catalog/"))
        return json({
          revision: "t",
          total: 0,
          offset: 0,
          limit: 25,
          items: [],
        });
      // Administration panels load their own data; none of it matters here.
      if (url.startsWith("/api/v1/")) return json([], 200);
      throw new Error(`Unexpected request: ${method} ${url}`);
    }),
  );
  return backend;
}

function renderEquipment() {
  render(
    <QueryClientProvider
      client={
        new QueryClient({ defaultOptions: { queries: { retry: false } } })
      }
    >
      <MemoryRouter
        initialEntries={[`/systems/${systemId}/settings/equipment`]}
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

describe("equipment section", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("en");
  });

  it("describes a system without equipment by capacity, orientation and tilt", async () => {
    const backend = installBackend(["system_manage"], []);
    const user = userEvent.setup();
    renderEquipment();

    await user.type(
      await screen.findByLabelText("Installed capacity (kWp)"),
      "9.9",
    );
    await user.selectOptions(screen.getByLabelText("Orientation"), "East");
    await user.click(screen.getByRole("button", { name: "Save array" }));

    expect(await screen.findByText("Equipment saved.")).toBeVisible();
    const [request] = backend.requests;
    expect(request?.method).toBe("POST");
    expect(request?.body).toMatchObject({
      name: "Inverter 1",
      strings: [
        {
          name: "Array 1",
          panelCount: 1,
          modulePeakPowerWatts: 9900,
          orientationDegrees: 90,
          tiltDegrees: 30,
          panelManufacturer: null,
          panelModel: null,
        },
      ],
    });
    // After the save the stored equipment is shown in the detailed editor.
    expect(await screen.findByDisplayValue("Fronius Symo")).toBeVisible();
  });

  it("keeps string identifiers and loss inputs when equipment is edited", async () => {
    const backend = installBackend(["system_manage"], [storedInverter()]);
    const user = userEvent.setup();
    renderEquipment();

    const name = await screen.findByLabelText("String name");
    await user.clear(name);
    await user.type(name, "South roof");
    await user.click(screen.getByRole("button", { name: "Save equipment" }));

    expect(await screen.findByText("Equipment saved.")).toBeVisible();
    const [request] = backend.requests;
    expect(request?.method).toBe("PUT");
    expect(request?.url).toBe(
      `/api/v1/systems/${systemId}/inverters/${inverterId}`,
    );
    expect(request?.body).toMatchObject({
      strings: [
        {
          id: stringId,
          name: "South roof",
          temperatureCoefficientPpmPerCelsius: -3400,
          shading: [
            {
              fromMinuteOfDay: 420,
              toMinuteOfDay: 540,
              label: "Chimney",
              lossBasisPoints: 3000,
            },
          ],
        },
      ],
    });
  });

  it("shows the identity a device reported and keeps its serial when equipment is edited", async () => {
    const backend = installBackend(
      ["system_manage"],
      [
        {
          ...storedInverter(),
          serialReference: "2000123456",
          firmwareVersion: "3.10.18.R",
          firmwareChangedAt: Date.UTC(2027, 0, 15),
        },
      ],
    );
    const user = userEvent.setup();
    renderEquipment();

    expect(await screen.findByText("2000123456")).toBeVisible();
    expect(screen.getByText("3.10.18.R")).toBeVisible();
    expect(screen.getByText("(since Jan 15, 2027)")).toBeVisible();

    await user.click(screen.getByRole("button", { name: "Save equipment" }));

    expect(await screen.findByText("Equipment saved.")).toBeVisible();
    expect(backend.requests[0]?.body).toMatchObject({
      serialReference: "2000123456",
    });
    expect(backend.requests[0]?.body).not.toHaveProperty("firmwareVersion");
  });

  it("names what is missing instead of saving", async () => {
    const backend = installBackend(["system_manage"], [storedInverter()]);
    const user = userEvent.setup();
    renderEquipment();

    await user.clear(await screen.findByLabelText("String name"));

    expect(
      screen.getByText("The name for string 1 of inverter 1 is missing."),
    ).toBeVisible();
    expect(
      screen.getByRole("button", { name: "Save equipment" }),
    ).toBeDisabled();
    expect(backend.requests).toHaveLength(0);
  });

  it("is read-only without management permission", async () => {
    installBackend(["system_read"], [storedInverter()]);
    renderEquipment();

    expect(await screen.findByLabelText("String name")).toBeDisabled();
    expect(
      screen.queryByRole("button", { name: "Save equipment" }),
    ).not.toBeInTheDocument();
  });
});

describe("administration", () => {
  it("no longer edits system equipment and points to the system's settings", async () => {
    await i18n.changeLanguage("en");
    installBackend(["system_manage"], []);
    render(
      <QueryClientProvider
        client={
          new QueryClient({ defaultOptions: { queries: { retry: false } } })
        }
      >
        <MemoryRouter initialEntries={["/administration?section=data-sources"]}>
          <AdministrationPage />
        </MemoryRouter>
      </QueryClientProvider>,
    );

    expect(await screen.findByText("System equipment has moved")).toBeVisible();
    expect(
      await screen.findByRole("link", { name: "Open equipment settings" }),
    ).toHaveAttribute("href", `/systems/${systemId}/settings/equipment`);
    for (const former of [
      "Equipment templates",
      "System resources",
      "Confirm equipment snapshot",
    ])
      expect(screen.queryByText(former)).not.toBeInTheDocument();
  });
});
