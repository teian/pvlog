import AxeBuilder from "@axe-core/playwright";
import { expect, test } from "@playwright/test";

const systemId = "019505c8-7c85-7f0b-9bc3-2a3c4d5e6f70";

test.beforeEach(async ({ page }) => {
  await page.route("**/api/v1/session", (route) =>
    route.fulfill({
      json: {
        authenticated: true,
        user: { id: systemId, displayName: "Ada" },
        accountId: systemId,
        systemIds: [systemId],
        permissions: ["system_read", "telemetry_read"],
        connectors: [],
      },
    }),
  );
  await page.route(`**/api/v1/systems/${systemId}/overview`, (route) =>
    route.fulfill({
      json: {
        id: systemId,
        name: "Rooftop South",
        timezone: "Europe/Berlin",
        lifecycle: "active",
        inverterCount: 1,
        stringCount: 2,
        capacityWatts: 9200,
      },
    }),
  );
  await page.route(
    `**/api/v1/systems/${systemId}/reporting/statistics`,
    (route) =>
      route.fulfill({
        json: {
          systemId,
          generationEnergyWh: 123000,
          consumptionEnergyWh: 65000,
          peakGenerationPowerWatts: 8400,
          firstObservationAtEpochMillis: 1767225600000,
          lastObservationAtEpochMillis: 1784073600000,
          coverageBasisPoints: 9900,
          monthly: [],
        },
      }),
  );
  await page.route(`**/api/v1/systems/${systemId}/seasonal`, (route) =>
    route.fulfill({
      json: {
        systemId,
        seasons: [
          {
            season: "winter",
            generationEnergyWh: 1000,
            measuredDays: 2,
            averageDailyEnergyWh: 500,
          },
          {
            season: "spring",
            generationEnergyWh: 3000,
            measuredDays: 3,
            averageDailyEnergyWh: 1000,
          },
          {
            season: "summer",
            generationEnergyWh: 5000,
            measuredDays: 4,
            averageDailyEnergyWh: 1250,
          },
          {
            season: "autumn",
            generationEnergyWh: 2000,
            measuredDays: 2,
            averageDailyEnergyWh: 1000,
          },
        ],
      },
    }),
  );
  await page.route(`**/api/v1/systems/${systemId}/weather-forecast`, (route) =>
    route.fulfill({
      json: {
        systemId,
        issuedAtEpochMillis: null,
        attribution: null,
        points: [],
      },
    }),
  );
});

test("opens every reporting view from the design navigation", async ({
  page,
}) => {
  await page.goto("/systems");
  await expect(
    page.getByRole("heading", { name: /all systems/i }),
  ).toBeVisible();
  await expect(page.getByText("Rooftop South")).toBeVisible();

  for (const [label, heading] of [
    ["Statistics", "Statistics"],
    ["Seasonal", "Seasonal"],
    ["Weather", "Weather"],
  ]) {
    await page.getByRole("link", { name: label, exact: true }).click();
    await expect(
      page.getByRole("heading", { name: heading, exact: true }),
    ).toBeVisible();
  }

  const results = await new AxeBuilder({ page }).analyze();
  expect(results.violations).toEqual([]);
});

test("shows permission failures inside the application shell", async ({
  page,
}) => {
  await page.unroute("**/api/v1/session");
  await page.route("**/api/v1/session", (route) =>
    route.fulfill({
      json: {
        authenticated: true,
        user: { id: systemId, displayName: "Ada" },
        accountId: systemId,
        systemIds: [systemId],
        permissions: [],
        connectors: [],
      },
    }),
  );

  await page.goto("/statistics");

  await expect(page).toHaveURL(/\/forbidden$/);
  await expect(
    page.getByRole("heading", { name: /access denied/i }),
  ).toBeVisible();
  await expect(page.getByRole("navigation")).toBeVisible();
  await expect(
    page.getByRole("link", { name: /return to dashboard/i }),
  ).toBeVisible();

  const results = await new AxeBuilder({ page }).analyze();
  expect(results.violations).toEqual([]);
});
