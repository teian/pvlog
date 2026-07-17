import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { App } from "@/app";
import i18n from "@/shared/lib/i18n";

const SYSTEM_ID = "019505c8-7c85-7f0b-9bc3-2a3c4d5e6f70";
const OBSERVATION_ID = "019505c8-7c85-7f0b-9bc3-2a3c4d5e6f71";

function issuesResponse() {
  return new Response(
    JSON.stringify([
      {
        kind: "missing_interval",
        startEpochMillis: 1_700_000_000_000,
        endEpochMillis: 1_700_003_600_000,
        sourceReferences: [],
        reasonCode: "not_reported",
      },
      {
        kind: "rejected_ingestion",
        startEpochMillis: 1_700_007_200_000,
        endEpochMillis: 1_700_010_800_000,
        sourceReferences: ["uploader:legacy"],
        reasonCode: "validation_failed",
      },
    ]),
    { status: 200, headers: { "content-type": "application/json" } },
  );
}

function renderApp() {
  return render(
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
}

describe("DataQualityPage", () => {
  let fetchMock: ReturnType<typeof vi.fn>;

  beforeEach(async () => {
    await i18n.changeLanguage("en");
    window.history.replaceState({}, "", `/systems/${SYSTEM_ID}/data-quality`);
    fetchMock = vi.fn(async (input: string, init?: RequestInit) => {
      const url = new URL(input, "http://localhost");
      if (url.pathname === "/api/v1/session") {
        return new Response(
          JSON.stringify({
            authenticated: true,
            user: { id: SYSTEM_ID, displayName: "Ada" },
            accountId: SYSTEM_ID,
            systemIds: [SYSTEM_ID],
            permissions: ["telemetry_read", "telemetry_write"],
            connectors: [],
          }),
          { status: 200, headers: { "content-type": "application/json" } },
        );
      }
      if (url.pathname === `/api/v1/systems/${SYSTEM_ID}/data-quality`) {
        return issuesResponse();
      }
      if (
        url.pathname ===
          `/api/v1/systems/${SYSTEM_ID}/observations/${OBSERVATION_ID}` &&
        init?.method === "PATCH"
      ) {
        const body = JSON.parse(String(init.body)) as {
          expectedVersion: number;
        };
        if (body.expectedVersion === 999) {
          return new Response(
            JSON.stringify({
              type: "https://pvlog.example/problems/conflict",
              title: "Conflict",
              status: 409,
              detail: "stale version",
            }),
            {
              status: 409,
              headers: { "content-type": "application/problem+json" },
            },
          );
        }
        return new Response(
          JSON.stringify({
            id: OBSERVATION_ID,
            systemId: SYSTEM_ID,
            values: { generationPower: 4300 },
            version: body.expectedVersion + 1,
            archived: false,
          }),
          { status: 200, headers: { "content-type": "application/json" } },
        );
      }
      return new Response(null, { status: 404 });
    });
    vi.stubGlobal("fetch", fetchMock);
  });

  it("lists data-quality issues by kind", async () => {
    renderApp();
    expect(
      await screen.findByRole("heading", { name: "Data quality" }),
    ).toBeVisible();
    const table = within(await screen.findByRole("table"));
    expect(table.getByText("Missing interval")).toBeVisible();
    expect(table.getByText("Rejected ingestion")).toBeVisible();
    expect(table.getByText("uploader:legacy")).toBeVisible();
  });

  it("submits a correction and shows the reconciliation indicator", async () => {
    const user = userEvent.setup();
    renderApp();
    await screen.findByRole("heading", { name: "Data quality" });
    await user.type(screen.getByLabelText("Observation ID"), OBSERVATION_ID);
    await user.type(screen.getByLabelText("Expected version"), "3");
    await user.type(screen.getByLabelText("Reason"), "Sensor recalibrated");
    await user.click(screen.getByRole("button", { name: "Submit correction" }));
    expect(await screen.findByText("Correction accepted")).toBeVisible();
    expect(await screen.findByText("Reconciliation in progress")).toBeVisible();
  });

  it("reports a conflict when the observation changed first", async () => {
    const user = userEvent.setup();
    renderApp();
    await screen.findByRole("heading", { name: "Data quality" });
    await user.type(screen.getByLabelText("Observation ID"), OBSERVATION_ID);
    await user.type(screen.getByLabelText("Expected version"), "999");
    await user.type(screen.getByLabelText("Reason"), "Sensor recalibrated");
    await user.click(screen.getByRole("button", { name: "Submit correction" }));
    expect(
      await screen.findByText(
        "Someone else changed this observation first. Reload it and try again.",
      ),
    ).toBeVisible();
  });

  it("navigates between the charts and data-quality tabs", async () => {
    const user = userEvent.setup();
    renderApp();
    await screen.findByRole("heading", { name: "Data quality" });
    await user.click(screen.getByRole("link", { name: "Charts" }));
    expect(
      await screen.findByRole("heading", { name: "Historical charts" }),
    ).toBeVisible();
  });
});
