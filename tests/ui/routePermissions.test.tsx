import { render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { App } from "@/app";
import i18n from "@/shared/lib/i18n";

const systemId = "019505c8-7c85-7f0b-9bc3-2a3c4d5e6f70";
const runtimeConfig = {
  apiBaseUrl: "/api",
  telemetry: {
    enabled: false,
    headers: {},
    serviceName: "pvlog-ui",
    serviceVersion: "test",
  },
};

function session(permissions: string[]) {
  return {
    authenticated: true,
    user: { id: systemId, displayName: "Ada" },
    accountId: systemId,
    systemIds: [systemId],
    permissions,
    connectors: [],
  };
}

describe("route permission mapping", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("en");
  });

  it("accepts the backend system_read permission for the systems page", async () => {
    window.history.replaceState({}, "", "/systems");
    vi.stubGlobal(
      "fetch",
      vi.fn(async (input: RequestInfo | URL) => {
        const path = String(input);
        return new Response(
          JSON.stringify(
            path.includes("/overview")
              ? {
                  id: systemId,
                  name: "Rooftop South",
                  timezone: "Europe/Berlin",
                  lifecycle: "active",
                  inverterCount: 1,
                  stringCount: 2,
                  capacityWatts: 9200,
                }
              : session(["system_read"]),
          ),
          { status: 200 },
        );
      }),
    );

    render(<App runtimeConfig={runtimeConfig} />);

    expect(
      await screen.findByRole("heading", { name: "All Systems" }),
    ).toBeVisible();
    expect(await screen.findByText("Rooftop South")).toBeVisible();
    expect(screen.queryByText("Access denied")).not.toBeInTheDocument();
  });

  it("renders denied routes inside the normal application shell", async () => {
    window.history.replaceState({}, "", "/statistics");
    vi.stubGlobal(
      "fetch",
      vi.fn(
        async () => new Response(JSON.stringify(session([])), { status: 200 }),
      ),
    );

    render(<App runtimeConfig={runtimeConfig} />);

    expect(
      await screen.findByRole("heading", { name: "Access denied" }),
    ).toBeVisible();
    expect(screen.getByRole("navigation")).toBeVisible();
    expect(
      screen.getByRole("link", { name: "Return to dashboard" }),
    ).toHaveAttribute("href", "/");
  });
});
