import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { App } from "@/app";
import i18n from "@/shared/lib/i18n";

const SYSTEM_ID = "019505c8-7c85-7f0b-9bc3-2a3c4d5e6f70";

function seriesResponse(field: string) {
  return new Response(
    JSON.stringify({
      actualResolution: "hourly",
      timezone: "UTC",
      series: [
        {
          field,
          unit: "watts",
          points: [
            {
              timestampEpochMillis: 1_700_000_000_000,
              value: 4200,
              coverageBasisPoints: 9800,
              qualityFlags: 0,
            },
          ],
          gaps: [],
        },
      ],
    }),
    { status: 200, headers: { "content-type": "application/json" } },
  );
}

describe("SystemChartsPage", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("en");
    window.history.replaceState({}, "", `/systems/${SYSTEM_ID}`);
    vi.stubGlobal(
      "fetch",
      vi.fn(async (input: string) => {
        const url = new URL(input, "http://localhost");
        if (url.pathname === "/api/v1/session") {
          return new Response(
            JSON.stringify({
              authenticated: true,
              user: { id: SYSTEM_ID, displayName: "Ada" },
              accountId: SYSTEM_ID,
              systemIds: [SYSTEM_ID],
              permissions: ["analytics:read"],
              connectors: [],
            }),
            { status: 200, headers: { "content-type": "application/json" } },
          );
        }
        if (url.pathname === `/api/v1/systems/${SYSTEM_ID}/series`) {
          return seriesResponse(url.searchParams.get("fields") ?? "");
        }
        return new Response(null, { status: 404 });
      }),
    );
  });

  it("renders bounded charts for the default categories", async () => {
    render(
      <App
        runtimeConfig={{
          apiBaseUrl: "/api",
          telemetry: {
            enabled: false,
            headers: {},
            serviceName: "pvlog-ui",
            serviceVersion: "test",
          },
        }}
      />,
    );
    expect(
      await screen.findByRole("heading", { name: "Historical charts" }),
    ).toBeVisible();
    await waitFor(() => {
      expect(screen.getAllByText(/Resolution: Hourly/)).toHaveLength(2);
    });
  });

  it("adds a chart when a category is toggled on", async () => {
    const user = userEvent.setup();
    render(
      <App
        runtimeConfig={{
          apiBaseUrl: "/api",
          telemetry: {
            enabled: false,
            headers: {},
            serviceName: "pvlog-ui",
            serviceVersion: "test",
          },
        }}
      />,
    );
    await screen.findByRole("heading", { name: "Historical charts" });
    await waitFor(() => {
      expect(screen.getAllByText(/Resolution: Hourly/)).toHaveLength(2);
    });
    await user.click(screen.getByRole("button", { name: "Battery" }));
    await waitFor(() => {
      expect(screen.getAllByText(/Resolution: Hourly/)).toHaveLength(3);
    });
  });

  it("requests a bounded point budget for every series query", async () => {
    render(
      <App
        runtimeConfig={{
          apiBaseUrl: "/api",
          telemetry: {
            enabled: false,
            headers: {},
            serviceName: "pvlog-ui",
            serviceVersion: "test",
          },
        }}
      />,
    );
    await screen.findByRole("heading", { name: "Historical charts" });
    await waitFor(() => {
      expect(screen.getAllByText(/Resolution: Hourly/)).toHaveLength(2);
    });
    const seriesRequests = vi
      .mocked(fetch)
      .mock.calls.map((call) => new URL(String(call[0]), "http://localhost"))
      .filter((url) => url.pathname === `/api/v1/systems/${SYSTEM_ID}/series`);
    expect(seriesRequests.length).toBeGreaterThan(0);
    for (const url of seriesRequests) {
      const maximumPoints = Number(url.searchParams.get("maximumPoints"));
      expect(maximumPoints).toBeGreaterThan(0);
      expect(maximumPoints).toBeLessThanOrEqual(10_000);
    }
  });
});
