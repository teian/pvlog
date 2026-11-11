import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter, Route, Routes } from "react-router";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { SystemSettingsPage } from "@/pages/SystemSettingsPage";
import i18n from "@/shared/lib/i18n";

const accountId = "019505c8-7c85-7f0b-9bc3-2a3c4d5e6f70";
const systemId = "019505c8-7c85-7f0b-9bc3-2a3c4d5e6f71";
const tariffId = "019505c8-7c85-7f0b-9bc3-2a3c4d5e6f90";

function json(value: unknown, status = 200) {
  return new Response(value === null ? null : JSON.stringify(value), {
    status,
    headers: { "content-type": "application/json" },
  });
}

interface Backend {
  requests: { method: string; url: string; body: unknown }[];
  tariffs: Record<string, unknown>[];
  sharing: Record<string, unknown>;
}

function installBackend(permissions: string[]): Backend {
  const backend: Backend = {
    requests: [],
    tariffs: [],
    sharing: {
      systemId,
      linkEnabled: true,
      publicToken: "token-1",
      visibility: "unlisted",
    },
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
      const tariffs = `/api/v1/systems/${systemId}/tariffs`;
      if (url === tariffs && method === "GET") return json(backend.tariffs);
      if (url === tariffs && method === "POST") {
        const stored = { id: tariffId, ...body };
        backend.tariffs = [stored];
        return json(stored, 201);
      }
      if (url === `${tariffs}/${tariffId}` && method === "PUT") {
        const stored = { id: tariffId, ...body };
        backend.tariffs = [stored];
        return json(stored);
      }
      if (url === `${tariffs}/${tariffId}` && method === "DELETE") {
        backend.tariffs = [];
        return json(null, 204);
      }
      const sharing = `/api/v1/systems/${systemId}/sharing`;
      if (url === sharing && method === "PUT") {
        backend.sharing = {
          ...backend.sharing,
          linkEnabled: body.linkEnabled,
          ...(body.visibility ? { visibility: body.visibility } : {}),
          ...(body.rotateToken ? { publicToken: "token-2" } : {}),
        };
      }
      if (url === sharing) return json(backend.sharing);
      throw new Error(`Unexpected request: ${method} ${url}`);
    }),
  );
  return backend;
}

function renderSection(section: string) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  render(
    <QueryClientProvider client={client}>
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
  return client;
}

describe("tariffs section", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("en");
  });

  it("adds, edits and deletes an export tariff", async () => {
    const backend = installBackend(["system_manage"]);
    const user = userEvent.setup();
    const client = renderSection("tariffs");
    const invalidated = vi.spyOn(client, "invalidateQueries");

    expect(await screen.findByText("No tariff yet")).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Add tariff" }));
    expect(
      screen.getByText(/Enter a name, a three-letter currency/),
    ).toBeVisible();
    expect(backend.requests).toHaveLength(0);

    await user.type(screen.getByLabelText("Name"), "Feed-in 2024");
    await user.type(screen.getByLabelText("Price per kWh"), "8");
    await user.type(screen.getByLabelText("Valid from"), "2024-01-01");
    await user.click(screen.getByRole("button", { name: "Add tariff" }));

    const row = (await screen.findByText("Feed-in 2024")).closest("tr");
    expect(
      within(row as HTMLElement).getByText("Exported energy"),
    ).toBeVisible();
    expect(backend.requests[0]).toEqual({
      method: "POST",
      url: `/api/v1/systems/${systemId}/tariffs`,
      body: {
        name: "Feed-in 2024",
        direction: "export",
        currencyCode: "EUR",
        minorUnitsPerKwh: 8,
        effectiveFromEpochMillis: Date.UTC(2024, 0, 1),
        effectiveToEpochMillis: null,
      },
    });
    // The checklist's tariff item is refreshed.
    expect(invalidated).toHaveBeenCalledWith({
      queryKey: ["system", systemId, "setup-status"],
    });

    await user.click(
      screen.getByRole("button", { name: "Edit tariff Feed-in 2024" }),
    );
    const price = screen.getByLabelText("Price per kWh");
    expect(price).toHaveValue("8");
    await user.clear(price);
    await user.type(price, "9");
    await user.click(screen.getByRole("button", { name: "Save tariff" }));
    expect(
      await screen.findByText("9 minor units of EUR per kWh"),
    ).toBeVisible();
    expect(backend.requests[1]).toMatchObject({
      method: "PUT",
      url: `/api/v1/systems/${systemId}/tariffs/${tariffId}`,
      body: { minorUnitsPerKwh: 9 },
    });

    await user.click(
      screen.getByRole("button", { name: "Delete tariff Feed-in 2024" }),
    );
    expect(await screen.findByText("No tariff yet")).toBeVisible();
    expect(backend.requests[2]?.method).toBe("DELETE");
  });

  it("is read-only without management permission", async () => {
    installBackend(["system_read"]);
    renderSection("tariffs");

    expect(await screen.findByText("No tariff yet")).toBeVisible();
    expect(
      screen.queryByRole("button", { name: "Add tariff" }),
    ).not.toBeInTheDocument();
  });
});

describe("sharing section", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("en");
  });

  it("changes the visibility", async () => {
    const backend = installBackend(["system_manage"]);
    const user = userEvent.setup();
    renderSection("sharing");

    const visibility = await screen.findByLabelText("Visibility");
    expect(visibility).toHaveValue("unlisted");
    await user.selectOptions(visibility, "Public");

    expect(backend.requests[0]?.body).toEqual({
      linkEnabled: true,
      visibility: "public",
    });
    expect(
      await screen.findByText(
        "Listed in the public directory of this PVLog instance.",
      ),
    ).toBeVisible();
  });

  it("copies the link and replaces it only after confirmation", async () => {
    const backend = installBackend(["system_manage"]);
    const user = userEvent.setup();
    const writeText = vi.fn(async () => undefined);
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText },
    });
    renderSection("sharing");

    const link = await screen.findByLabelText("Public link");
    expect(link).toHaveValue(
      `${window.location.origin}/api/v1/public/systems/token-1`,
    );
    await user.click(screen.getByRole("button", { name: "Copy link" }));
    expect(writeText).toHaveBeenCalledWith(
      `${window.location.origin}/api/v1/public/systems/token-1`,
    );

    await user.click(screen.getByRole("button", { name: "Create a new link" }));
    const dialog = await screen.findByRole("alertdialog");
    expect(within(dialog).getByText(/stops working immediately/)).toBeVisible();
    expect(backend.requests).toHaveLength(0);
    await user.click(
      within(dialog).getByRole("button", { name: "Replace link" }),
    );

    expect(
      await screen.findByText(
        "This is the new link. The previous link no longer works.",
      ),
    ).toBeVisible();
    expect(screen.getByLabelText("Public link")).toHaveValue(
      `${window.location.origin}/api/v1/public/systems/token-2`,
    );
    expect(backend.requests[0]?.body).toEqual({
      linkEnabled: true,
      rotateToken: true,
    });
  });

  it("disables the link and hides it", async () => {
    const backend = installBackend(["system_manage"]);
    const user = userEvent.setup();
    renderSection("sharing");

    await user.click(
      await screen.findByRole("switch", { name: "Public link enabled" }),
    );

    expect(backend.requests[0]?.body).toEqual({ linkEnabled: false });
    expect(screen.queryByLabelText("Public link")).not.toBeInTheDocument();
  });

  it("is read-only without management permission", async () => {
    installBackend(["system_read"]);
    renderSection("sharing");

    expect(await screen.findByLabelText("Visibility")).toBeDisabled();
    expect(
      screen.getByRole("switch", { name: "Public link enabled" }),
    ).toBeDisabled();
    expect(
      screen.queryByRole("button", { name: "Create a new link" }),
    ).not.toBeInTheDocument();
  });
});
