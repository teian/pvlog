import { expect, test } from "@playwright/test";

const id = "019505c8-7c85-7f0b-9bc3-2a3c4d5e6f70";
const run = "019505c8-7c85-7f0b-9bc3-2a3c4d5e6f71";
const now = 1_780_000_000_000;

test("reviews a stale partial forecast, table alternatives, underperformance, and recalculation", async ({
  page,
}) => {
  let savedBoundary: number | null = null;
  await mockSession(page);
  await page.route(
    `**/api/v1/accounts/${id}/systems/${id}/**`,
    async (route) => {
      const request = route.request();
      const path = new URL(request.url()).pathname;
      if (path.endsWith("/forecast-settings")) {
        if (request.method() === "PUT")
          savedBoundary = (await request.postDataJSON()).effectiveTo as number;
        return route.fulfill({ json: settings(), headers: { etag: '"1"' } });
      }
      if (path.endsWith("/forecast-input-completeness"))
        return route.fulfill({
          json: {
            scope: scope(),
            effectiveAt: now,
            includedCapacityWatts: 4000,
            totalEffectiveCapacityWatts: 8000,
            complete: false,
            reasons: ["missing_orientation", "partial_effective_capacity"],
            version: 1,
          },
        });
      if (path.endsWith("/yield-series"))
        return route.fulfill({ json: yieldSeries() });
      if (path.endsWith("/yield-performance"))
        return route.fulfill({
          json: performanceSeries(
            new URL(request.url()).searchParams.get("metric") ??
              "generation_performance",
          ),
        });
      return route.fulfill({ status: 404 });
    },
  );
  await page.goto(`/systems/${id}/forecast`);
  await expect(
    page.getByRole("heading", { name: "PV yield forecasting" }),
  ).toBeVisible();
  await expect(page.getByText("Stale forecast")).toBeVisible();
  await expect(
    page.getByText("A PV string is missing its orientation."),
  ).toBeVisible();
  await expect(page.getByText("75%")).toBeVisible();
  await page.getByRole("button", { name: "Data table" }).last().click();
  await expect(page.getByText(/Not available:/)).toBeVisible();
  const boundary = now + 86_400_000;
  await page
    .getByLabel("Effective until (optional epoch milliseconds)")
    .fill(String(boundary));
  await page.getByRole("button", { name: "Save forecast settings" }).click();
  await expect(
    page.getByText(/affected forecasts will be recalculated/i),
  ).toBeVisible();
  expect(savedBoundary).toBe(boundary);
});

test("localizes provider outage without hiding the telemetry-independence message", async ({
  page,
}) => {
  await page.addInitScript(() =>
    window.localStorage.setItem("pvlog-language", "de"),
  );
  await mockSession(page);
  await page.route(`**/api/v1/accounts/${id}/systems/${id}/**`, (route) => {
    const path = new URL(route.request().url()).pathname;
    if (path.endsWith("/yield-series")) return route.fulfill({ status: 503 });
    if (path.endsWith("/forecast-settings"))
      return route.fulfill({ json: settings(), headers: { etag: '"1"' } });
    if (path.endsWith("/forecast-input-completeness"))
      return route.fulfill({
        json: {
          scope: scope(),
          effectiveAt: now,
          includedCapacityWatts: 8000,
          totalEffectiveCapacityWatts: 8000,
          complete: true,
          reasons: [],
          version: 1,
        },
      });
    if (path.endsWith("/yield-performance"))
      return route.fulfill({
        json: performanceSeries(
          new URL(route.request().url()).searchParams.get("metric") ??
            "generation_performance",
        ),
      });
    return route.fulfill({ status: 404 });
  });
  await page.goto(`/systems/${id}/forecast`);
  await expect(
    page.getByText("Prognose vorübergehend nicht verfügbar"),
  ).toBeVisible();
  await expect(page.getByText(/Ist-Telemetrie bleibt verfügbar/)).toBeVisible();
  await expect(page.locator("html")).toHaveAttribute("lang", "de");
});

async function mockSession(page: import("@playwright/test").Page) {
  await page.route("**/api/v1/session", (route) =>
    route.fulfill({
      json: {
        authenticated: true,
        user: { id, displayName: "Ada" },
        accountId: id,
        systemIds: [id],
        permissions: ["analytics:read"],
        connectors: [],
      },
    }),
  );
}
function scope() {
  return { kind: "system", account_id: id, system_id: id };
}
function provenance() {
  return {
    providerId: "weather",
    adapter: "normalized-json",
    sourceUrl: "https://weather.example/forecast",
    licenseIdentifier: "open",
    attribution: "Example Weather",
    fetchedAt: now,
  };
}
function settings() {
  return {
    scope: scope(),
    effectiveFrom: now,
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
    version: 1,
  };
}
function yieldSeries() {
  return {
    scope: scope(),
    basis: "forecast",
    resolution: "hour",
    issueTime: now,
    weatherRunId: run,
    calculationRunId: run,
    modelIdentifier: "pvwatts-compatible",
    modelRevision: 1,
    configurationDigest: "0".repeat(64),
    freshness: "stale",
    provenance: provenance(),
    includedCapacityWatts: 4000,
    totalEffectiveCapacityWatts: 8000,
    completeness: { partial: { reasons: ["partial_effective_capacity"] } },
    unavailableReasons: [],
    points: [
      {
        intervalStart: now,
        intervalEnd: now + 3_600_000,
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
    issueTime: metric === "forecast_realization" ? now : null,
    weatherRunId: run,
    calculationRunId: run,
    modelIdentifier: "pvwatts-compatible",
    modelRevision: 1,
    configurationDigest: "0".repeat(64),
    freshness: "fresh",
    provenance: provenance(),
    points:
      metric === "generation_performance"
        ? [
            {
              intervalStart: now - 86_400_000,
              intervalEnd: now,
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
              intervalStart: now - 86_400_000,
              intervalEnd: now,
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
