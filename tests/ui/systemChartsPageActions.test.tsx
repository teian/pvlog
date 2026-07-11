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
            {
              timestampEpochMillis: 1_700_003_600_000,
              value: 4500,
              coverageBasisPoints: 9800,
              qualityFlags: 0,
              provenance: "corrected",
            },
          ],
          gaps: [
            {
              startEpochMillis: 1_700_007_200_000,
              endEpochMillis: 1_700_010_800_000,
              kind: "missing",
            },
          ],
        },
      ],
    }),
    { status: 200, headers: { "content-type": "application/json" } },
  );
}

describe("SystemChartsPage actions", () => {
  let fetchMock: ReturnType<typeof vi.fn>;

  beforeEach(async () => {
    await i18n.changeLanguage("en");
    window.history.replaceState({}, "", `/systems/${SYSTEM_ID}`);
    URL.createObjectURL = vi.fn(() => "blob:mock");
    URL.revokeObjectURL = vi.fn();
    fetchMock = vi.fn(async (input: string, init?: RequestInit) => {
      const url = new URL(input, "http://localhost");
      if (url.pathname === "/api/v1/session") {
        return new Response(
          JSON.stringify({
            authenticated: true,
            user: { id: SYSTEM_ID, displayName: "Ada" },
            accountId: SYSTEM_ID,
            systemIds: [SYSTEM_ID],
            permissions: ["analytics:read", "exports:write"],
            connectors: [],
          }),
          { status: 200, headers: { "content-type": "application/json" } },
        );
      }
      if (url.pathname === `/api/v1/systems/${SYSTEM_ID}/series`) {
        return seriesResponse(url.searchParams.get("fields") ?? "");
      }
      if (
        url.pathname === `/api/v1/systems/${SYSTEM_ID}/analysis-exports` &&
        init?.method === "POST"
      ) {
        return new Response("timestamp_epoch_millis,field,value\n", {
          status: 200,
          headers: { "content-type": "text/csv" },
        });
      }
      return new Response(null, { status: 404 });
    });
    vi.stubGlobal("fetch", fetchMock);
  });

  it("switches the generation chart to an accessible table with gap text", async () => {
    const user = userEvent.setup();
    render(
      <App
        runtimeConfig={{
          apiBaseUrl: "/api",
          telemetry: { enabled: false, headers: {}, serviceName: "pvlog-ui", serviceVersion: "test" },
        }}
      />,
    );
    await screen.findByRole("heading", { name: "Historical charts" });
    await waitFor(() => {
      expect(screen.getAllByText(/Resolution: Hourly/)).toHaveLength(2);
    });
    const [firstTableToggle] = screen.getAllByRole("radio", { name: "Table" });
    await user.click(firstTableToggle);
    expect(await screen.findByText("Timestamp")).toBeVisible();
    expect(screen.getByText(/Missing:/)).toBeVisible();
  });

  it("requests a CSV export matching the displayed chart", async () => {
    const user = userEvent.setup();
    render(
      <App
        runtimeConfig={{
          apiBaseUrl: "/api",
          telemetry: { enabled: false, headers: {}, serviceName: "pvlog-ui", serviceVersion: "test" },
        }}
      />,
    );
    await screen.findByRole("heading", { name: "Historical charts" });
    await waitFor(() => {
      expect(screen.getAllByText(/Resolution: Hourly/)).toHaveLength(2);
    });
    const [firstExportCsv] = screen.getAllByRole("button", { name: "Export CSV" });
    await user.click(firstExportCsv);
    await waitFor(() => {
      const exportCalls = fetchMock.mock.calls.filter(
        (call) =>
          new URL(String(call[0]), "http://localhost").pathname ===
          `/api/v1/systems/${SYSTEM_ID}/analysis-exports`,
      );
      expect(exportCalls).toHaveLength(1);
    });
  });

  it("shows a previous-period comparison when compare is toggled on", async () => {
    const user = userEvent.setup();
    render(
      <App
        runtimeConfig={{
          apiBaseUrl: "/api",
          telemetry: { enabled: false, headers: {}, serviceName: "pvlog-ui", serviceVersion: "test" },
        }}
      />,
    );
    await screen.findByRole("heading", { name: "Historical charts" });
    await waitFor(() => {
      expect(screen.getAllByText(/Resolution: Hourly/)).toHaveLength(2);
    });
    const [firstCompareToggle] = screen.getAllByRole("button", {
      name: "Compare to previous period",
    });
    await user.click(firstCompareToggle);
    expect(await screen.findByText(/Previous period averaged/)).toBeVisible();
  });
});
