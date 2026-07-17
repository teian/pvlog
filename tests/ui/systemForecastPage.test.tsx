import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { App } from "@/app";
import i18n from "@/shared/lib/i18n";

const ID = "019505c8-7c85-7f0b-9bc3-2a3c4d5e6f70";
const RUN = "019505c8-7c85-7f0b-9bc3-2a3c4d5e6f71";
const NOW = 1_780_000_000_000;

describe("SystemForecastPage", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("en");
    window.history.replaceState({}, "", `/systems/${ID}/forecast`);
  });

  it("shows complete forecast metadata, underperformance, and table alternatives without replacing null with zero", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn((input: string) => mockResponse(input)),
    );
    renderApp();
    expect(
      await screen.findByRole("heading", { name: "PV yield forecasting" }),
    ).toBeVisible();
    expect(await screen.findByText("Fresh forecast")).toBeVisible();
    expect(screen.getByText("Forecast ready")).toBeVisible();
    expect(screen.getByText("75%")).toBeVisible();
    const tableButtons = screen.getAllByRole("button", { name: "Data table" });
    await userEvent.click(tableButtons[2]);
    expect(await screen.findByText(/Not available:/)).toBeVisible();
    const realization = screen
      .getByText("Forecast realization")
      .closest('[data-slot="card"]');
    expect(realization).not.toBeNull();
    expect(
      within(realization as HTMLElement).queryByText("0 kWh"),
    ).not.toBeInTheDocument();
  });

  it("shows stale partial configuration gaps and invalidates modeled resources after an effective-boundary update", async () => {
    let completenessCalls = 0;
    let updateBody: unknown;
    vi.stubGlobal(
      "fetch",
      vi.fn(async (input: string, init?: RequestInit) => {
        const url = new URL(input, "http://localhost");
        if (url.pathname.endsWith("/forecast-input-completeness"))
          completenessCalls += 1;
        if (
          url.pathname.endsWith("/forecast-settings") &&
          init?.method === "PUT"
        ) {
          updateBody = JSON.parse(String(init.body));
          return json(settings(), 200, { etag: '"2"' });
        }
        return mockResponse(input, { partial: true });
      }),
    );
    renderApp();
    expect(await screen.findByText("Stale forecast")).toBeVisible();
    expect(screen.getByText("Configuration required")).toBeVisible();
    expect(
      screen.getByText("A PV string is missing its orientation."),
    ).toBeVisible();
    const effectiveTo = screen.getByLabelText(
      "Effective until (optional epoch milliseconds)",
    );
    await userEvent.clear(effectiveTo);
    await userEvent.type(effectiveTo, String(NOW + 86_400_000));
    await userEvent.click(
      screen.getByRole("button", { name: "Save forecast settings" }),
    );
    await waitFor(() => expect(completenessCalls).toBeGreaterThan(1));
    expect(updateBody).toMatchObject({ effectiveTo: NOW + 86_400_000 });
    expect(
      screen.getByText(/affected forecasts will be recalculated/i),
    ).toBeVisible();
  });

  it("localizes provider unavailability in German while keeping actual telemetry conceptually independent", async () => {
    await i18n.changeLanguage("de");
    vi.stubGlobal(
      "fetch",
      vi.fn((input: string) => {
        const url = new URL(input, "http://localhost");
        if (url.pathname.endsWith("/yield-series"))
          return new Response(null, { status: 503 });
        return mockResponse(input);
      }),
    );
    renderApp();
    expect(
      await screen.findByText("Prognose vorübergehend nicht verfügbar"),
    ).toBeVisible();
    expect(screen.getByText(/Ist-Telemetrie bleibt verfügbar/)).toBeVisible();
    expect(document.documentElement.lang).toBe("de");
  });
});

function renderApp() {
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
}

function mockResponse(
  input: string,
  options: { partial?: boolean } = {},
): Promise<Response> {
  const url = new URL(input, "http://localhost");
  if (url.pathname === "/api/v1/session")
    return Promise.resolve(
      json({
        authenticated: true,
        user: { id: ID, displayName: "Ada" },
        accountId: ID,
        systemIds: [ID],
        permissions: ["telemetry_read"],
        connectors: [],
      }),
    );
  if (url.pathname.endsWith("/forecast-settings"))
    return Promise.resolve(json(settings(), 200, { etag: '"1"' }));
  if (url.pathname.endsWith("/forecast-input-completeness"))
    return Promise.resolve(
      json({
        scope: scope(),
        effectiveAt: NOW,
        includedCapacityWatts: options.partial ? 4000 : 8000,
        totalEffectiveCapacityWatts: 8000,
        complete: !options.partial,
        reasons: options.partial
          ? ["missing_orientation", "partial_effective_capacity"]
          : [],
        version: 1,
      }),
    );
  if (url.pathname.endsWith("/yield-series"))
    return Promise.resolve(json(yieldSeries(options.partial)));
  if (url.pathname.endsWith("/yield-performance"))
    return Promise.resolve(
      json(
        performanceSeries(
          url.searchParams.get("metric") ?? "generation_performance",
        ),
      ),
    );
  return Promise.resolve(new Response(null, { status: 404 }));
}

function settings() {
  return { ...settingsInput(), scope: scope(), version: 1 };
}
function settingsInput() {
  return {
    effectiveFrom: NOW,
    effectiveTo: null,
    modelIdentifier: "pvwatts-compatible",
    modelRevision: 1,
    losses: {
      soilingBasisPoints: 100,
      shadingBasisPoints: 200,
      mismatchBasisPoints: 100,
      wiringBasisPoints: 100,
      unavailabilityBasisPoints: 50,
    },
    calibrationBasisPoints: 0,
  };
}
function scope() {
  return { kind: "system", account_id: ID, system_id: ID };
}
function provenance() {
  return {
    providerId: "weather",
    adapter: "normalized-json",
    sourceUrl: "https://weather.example/forecast",
    licenseIdentifier: "open",
    attribution: "Example Weather",
    fetchedAt: NOW,
  };
}
function yieldSeries(partial = false) {
  return {
    scope: scope(),
    basis: "forecast",
    resolution: "hour",
    issueTime: NOW,
    weatherRunId: RUN,
    calculationRunId: RUN,
    modelIdentifier: "pvwatts-compatible",
    modelRevision: 1,
    configurationDigest: "0".repeat(64),
    freshness: partial ? "stale" : "fresh",
    provenance: provenance(),
    includedCapacityWatts: partial ? 4000 : 8000,
    totalEffectiveCapacityWatts: 8000,
    completeness: partial
      ? { partial: { reasons: ["partial_effective_capacity"] } }
      : "complete",
    unavailableReasons: [],
    points: [
      {
        intervalStart: NOW,
        intervalEnd: NOW + 3_600_000,
        centralPowerWatts: 4200,
        lowerPowerWatts: 3500,
        upperPowerWatts: 4800,
        centralEnergyWattHours: 4200,
        lowerEnergyWattHours: 3500,
        upperEnergyWattHours: 4800,
        coverageBasisPoints: 9500,
        completeness: "complete",
      },
    ],
  };
}
function performanceSeries(metric: string) {
  return {
    scope: scope(),
    metric,
    basis: metric === "forecast_realization" ? "forecast" : "expected",
    resolution: "day",
    issueTime: metric === "forecast_realization" ? NOW : null,
    weatherRunId: RUN,
    calculationRunId: RUN,
    modelIdentifier: "pvwatts-compatible",
    modelRevision: 1,
    configurationDigest: "0".repeat(64),
    freshness: "fresh",
    provenance: provenance(),
    points:
      metric === "generation_performance"
        ? [
            {
              intervalStart: NOW - 86_400_000,
              intervalEnd: NOW,
              actualEnergyWattHours: 7500,
              modeledEnergyWattHours: 10_000,
              ratioBasisPoints: 7500,
              actualCoverageBasisPoints: 9800,
              modeledCoverageBasisPoints: 10_000,
              unavailableReason: null,
            },
          ]
        : [
            {
              intervalStart: NOW - 86_400_000,
              intervalEnd: NOW,
              actualEnergyWattHours: null,
              modeledEnergyWattHours: 9000,
              ratioBasisPoints: null,
              actualCoverageBasisPoints: 0,
              modeledCoverageBasisPoints: 10_000,
              unavailableReason: "missing_actual_telemetry",
            },
          ],
  };
}
function json(body: unknown, status = 200, headers: HeadersInit = {}) {
  return new Response(JSON.stringify(body), {
    status,
    headers: {
      "content-type": "application/json",
      ...Object.fromEntries(new Headers(headers)),
    },
  });
}
