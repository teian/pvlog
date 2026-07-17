import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router";
import { describe, expect, it, vi } from "vitest";

import { OnboardingPage } from "@/pages/OnboardingPage";
import i18n from "@/shared/lib/i18n";

const accountId = "019505c8-7c85-7f0b-9bc3-2a3c4d5e6f70";
const systemId = "019505c8-7c85-7f0b-9bc3-2a3c4d5e6f71";

function json(value: unknown, status = 200) {
  return new Response(JSON.stringify(value), {
    status,
    headers: { "content-type": "application/json" },
  });
}

function session() {
  return {
    authenticated: true,
    user: { id: accountId, displayName: "Ada" },
    accountId,
    systemIds: [systemId],
    permissions: ["system_manage"],
    connectors: [],
  };
}

function system(name = "South Roof") {
  return {
    id: systemId,
    accountId,
    name,
    timezone: "Europe/Berlin",
    visibility: "private",
    lifecycle: "active",
    version: 1,
    createdAt: 1,
    updatedAt: 1,
  };
}

function inverterTree() {
  const inverterId = "019505c8-7c85-7f0b-9bc3-2a3c4d5e6f72";
  return [
    {
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
          id: "019505c8-7c85-7f0b-9bc3-2a3c4d5e6f73",
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
          effectiveFrom: 1,
          effectiveTo: null,
        },
      ],
    },
  ];
}

function emptyCatalog() {
  return { revision: "test", total: 0, offset: 0, limit: 25, items: [] };
}

function renderPage() {
  render(
    <QueryClientProvider
      client={
        new QueryClient({ defaultOptions: { queries: { retry: false } } })
      }
    >
      <MemoryRouter initialEntries={["/onboarding"]}>
        <OnboardingPage />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

describe("OnboardingPage system management", () => {
  it("renders the expandable system hierarchy and opens edit mode", async () => {
    await i18n.changeLanguage("en");
    vi.stubGlobal(
      "fetch",
      vi.fn(async (input: string | URL | Request) => {
        const url = input instanceof Request ? input.url : input.toString();
        if (url === "/api/v1/session") return json(session());
        if (url === `/api/v1/systems/${systemId}`) return json(system());
        if (url === `/api/v1/systems/${systemId}/inverters`)
          return json(inverterTree());
        if (url.startsWith("/api/v1/equipment-catalog/"))
          return json(emptyCatalog());
        if (url.startsWith("/api/v1/geocoding/search?"))
          return json([
            {
              displayName: "Marienplatz 1, Munich, Germany",
              latitude: 48.1373932,
              longitude: 11.5754485,
              attribution: "© OpenStreetMap contributors",
            },
          ]);
        throw new Error(`Unexpected request: ${url}`);
      }),
    );
    const user = userEvent.setup();
    renderPage();

    expect(await screen.findByText("South Roof")).toBeVisible();
    expect(screen.getByText("Symo")).toBeVisible();
    expect(screen.getByText("18 × 450 Wp · South")).toBeVisible();
    await user.click(screen.getAllByRole("button", { name: "Edit" })[0]!);
    expect(
      await screen.findByRole("heading", { name: "Edit system" }),
    ).toBeVisible();
    expect(screen.getByLabelText("System name")).toHaveValue("South Roof");
    expect(screen.getByText("PV strings")).toBeVisible();
    await user.type(screen.getByLabelText("Location"), "Marienplatz 1, Munich");
    expect(
      await screen.findByRole("option", { name: /Marienplatz 1/ }),
    ).toBeVisible();
    await user.keyboard("{ArrowDown}{Enter}");
    expect(await screen.findByText("48.137393, 11.575449")).toBeVisible();
  });

  it("creates a system and its inverter aggregate through the wizard", async () => {
    await i18n.changeLanguage("en");
    const fetchMock = vi.fn(
      async (input: string | URL | Request, init?: RequestInit) => {
        const url = input instanceof Request ? input.url : input.toString();
        const method = init?.method ?? "GET";
        if (url === "/api/v1/session") return json(session());
        if (url === `/api/v1/systems/${systemId}`) return json(system());
        if (url === `/api/v1/systems/${systemId}/inverters`)
          return method === "POST" ? json(inverterTree()[0], 201) : json([]);
        if (url.startsWith("/api/v1/equipment-catalog/"))
          return json(emptyCatalog());
        if (url === "/api/v1/systems" && method === "POST")
          return json(system("New roof"), 201);
        throw new Error(`Unexpected ${method} request: ${url}`);
      },
    );
    vi.stubGlobal("fetch", fetchMock);
    const user = userEvent.setup();
    renderPage();

    await user.click(await screen.findByRole("button", { name: "Add system" }));
    await user.type(screen.getByLabelText("System name"), "New roof");
    await user.click(screen.getByRole("button", { name: "Create system" }));

    expect(
      await screen.findByRole("heading", {
        name: "System created successfully",
      }),
    ).toBeVisible();
    const createSystem = fetchMock.mock.calls.find(
      ([input, init]) => input === "/api/v1/systems" && init?.method === "POST",
    );
    expect(JSON.parse(String(createSystem?.[1]?.body))).toMatchObject({
      name: "New roof",
    });
    const createInverter = fetchMock.mock.calls.find(
      ([input, init]) =>
        input === `/api/v1/systems/${systemId}/inverters` &&
        init?.method === "POST",
    );
    expect(JSON.parse(String(createInverter?.[1]?.body))).toMatchObject({
      name: "INV-1",
      strings: [{ name: "STR-1", modulePeakPowerWatts: 400 }],
    });
  });
});
