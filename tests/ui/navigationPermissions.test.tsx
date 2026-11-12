import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, within } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { mayOpen } from "@/features/auth";
import i18n from "@/shared/lib/i18n";
import { AppShell } from "@/widgets";

const accountId = "019505c8-7c85-7f0b-9bc3-2a3c4d5e6f70";
const systemId = "019505c8-7c85-7f0b-9bc3-2a3c4d5e6f71";

/** Which guarded destination each application address belongs to. */
function destinationOf(href: string) {
  if (href === "/" || href === "/account" || href.startsWith("#")) return null;
  if (href === "/systems") return "systems";
  if (href === "/setup") return "setup";
  if (href === "/administration") return "administration";
  if (href.startsWith("/systems/")) return "system";
  if (["/statistics", "/seasonal", "/weather"].includes(href)) return "reports";
  throw new Error(`Navigation offers an unknown address: ${href}`);
}

async function renderShell(permissions: string[]) {
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: string | URL | Request) => {
      const url = input instanceof Request ? input.url : input.toString();
      const body =
        url === "/api/v1/session"
          ? {
              authenticated: true,
              user: { id: accountId, displayName: "Ada" },
              accountId,
              systemIds: [systemId],
              permissions,
              connectors: [],
            }
          : {
              id: systemId,
              name: "Roof",
              timezone: "Europe/Berlin",
              lifecycle: "active",
              inverterCount: 0,
              stringCount: 0,
              capacityWatts: null,
              dataState: "current",
            };
      return new Response(JSON.stringify(body), { status: 200 });
    }),
  );
  render(
    <QueryClientProvider client={new QueryClient()}>
      <MemoryRouter>
        <AppShell systemIds={[systemId]}>
          <p>content</p>
        </AppShell>
      </MemoryRouter>
    </QueryClientProvider>,
  );
  // The account entry appears once the session is known.
  await screen.findByRole("link", { name: "Account" });
  return screen
    .getAllByRole("link")
    .map((link) => link.getAttribute("href") ?? "");
}

describe("permission-aware navigation", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("en");
  });

  it.each([
    [[]],
    [["system_read"]],
    [["system_read", "telemetry_read"]],
    [["system_read", "telemetry_read", "system_manage"]],
    [["audit_read"]],
  ])("offers no entry that leads to a forbidden page for %j", async (held) => {
    const hrefs = await renderShell(held);
    for (const href of hrefs) {
      const destination = destinationOf(href);
      if (destination) expect(mayOpen(held, destination), href).toBe(true);
    }
  });

  it("hides adding a system, system pages and administration from a reader", async () => {
    const hrefs = await renderShell(["system_read"]);
    expect(hrefs).toContain("/systems");
    expect(hrefs).not.toContain("/setup");
    expect(hrefs).not.toContain("/administration");
    expect(hrefs).not.toContain(`/systems/${systemId}`);
    expect(hrefs).not.toContain("/statistics");
  });

  it("offers adding a system next to the systems to someone who may manage them", async () => {
    await renderShell(["system_read", "telemetry_read", "system_manage"]);
    const navigation = within(
      screen.getByRole("navigation", { name: "Primary navigation" }),
    );
    expect(
      navigation.getByRole("link", { name: "Add system" }),
    ).toHaveAttribute("href", "/setup");
    expect(await navigation.findByRole("link", { name: "Roof" })).toBeVisible();
  });

  it("names the account entry", async () => {
    await renderShell([]);
    expect(screen.getByRole("link", { name: "Account" })).toHaveAttribute(
      "href",
      "/account",
    );
  });
});
