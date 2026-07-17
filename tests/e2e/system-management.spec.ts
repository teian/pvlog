import AxeBuilder from "@axe-core/playwright";
import { expect, test } from "@playwright/test";

const accountId = "019505c8-7c85-7f0b-9bc3-2a3c4d5e6f70";
const systemId = "019505c8-7c85-7f0b-9bc3-2a3c4d5e6f71";
const inverterId = "019505c8-7c85-7f0b-9bc3-2a3c4d5e6f72";

test.beforeEach(async ({ page }) => {
  await page.route("**/api/v1/session", (route) =>
    route.fulfill({
      json: {
        authenticated: true,
        user: { id: accountId, displayName: "Ada" },
        accountId,
        systemIds: [systemId],
        permissions: ["system_manage", "system_read"],
        connectors: [],
      },
    }),
  );
  await page.route(`**/api/v1/systems/${systemId}`, (route) =>
    route.fulfill({
      json: {
        id: systemId,
        accountId,
        name: "South Roof",
        timezone: "Europe/Berlin",
        visibility: "private",
        lifecycle: "active",
        version: 1,
        createdAt: 1,
        updatedAt: 1,
      },
    }),
  );
  await page.route(`**/api/v1/systems/${systemId}/inverters`, (route) =>
    route.fulfill({
      json: [
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
      ],
    }),
  );
  await page.route("**/api/v1/equipment-catalog/**", (route) =>
    route.fulfill({
      json: {
        revision: "test",
        total: 0,
        offset: 0,
        limit: 25,
        items: [],
      },
    }),
  );
  await page.route("**/api/v1/geocoding/search?**", (route) =>
    route.fulfill({
      json: [
        {
          displayName: "Marienplatz 1, Munich, Germany",
          latitude: 48.1373932,
          longitude: 11.5754485,
          attribution: "© OpenStreetMap contributors",
        },
      ],
    }),
  );
});

test("matches the management hierarchy and edit-wizard behavior", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1440, height: 1000 });
  await page.goto("/onboarding");

  await expect(
    page.getByRole("heading", { level: 1, name: "System management" }),
  ).toBeVisible();
  await expect(page.getByRole("heading", { name: "South Roof" })).toBeVisible();
  await expect(page.getByText("18 × 450 Wp · South")).toBeVisible();

  await page.getByRole("button", { name: "Edit", exact: true }).click();
  await expect(
    page.getByRole("heading", { name: "Edit system" }),
  ).toBeVisible();
  await expect(
    page.getByRole("heading", { name: /System data/ }),
  ).toBeVisible();
  await expect(page.getByRole("heading", { name: /Inverters/ })).toBeVisible();
  await expect(page.getByRole("heading", { name: /PV strings/ })).toBeVisible();
  await page.getByRole("button", { name: "Advanced settings" }).click();
  await expect(page.getByLabel("Tilt (°)")).toHaveValue("30");
  await page.getByLabel("Location").fill("Marienplatz 1, Munich");
  await page.getByRole("option", { name: /Marienplatz 1/ }).click();
  await expect(page.getByText("48.137393, 11.575449")).toBeVisible();

  const results = await new AxeBuilder({ page }).analyze();
  expect(results.violations).toEqual([]);
});

for (const viewport of [
  { width: 375, height: 812 },
  { width: 768, height: 1024 },
  { width: 1440, height: 1000 },
]) {
  test(`stays within the viewport at ${String(viewport.width)}px`, async ({
    page,
  }) => {
    await page.setViewportSize(viewport);
    await page.goto("/onboarding");
    await expect(
      page.getByRole("heading", { name: "South Roof" }),
    ).toBeVisible();
    const width = await page.evaluate(() => ({
      scroll: document.documentElement.scrollWidth,
      client: document.documentElement.clientWidth,
    }));
    expect(width.scroll).toBeLessThanOrEqual(width.client);
  });
}
