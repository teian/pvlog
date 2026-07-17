import { expect, test } from "@playwright/test";
import AxeBuilder from "@axe-core/playwright";

test.beforeEach(async ({ page }) => {
  await page.route("**/api/v1/session", async (route) => {
    await route.fulfill({
      json: {
        authenticated: true,
        user: {
          id: "019505c8-7c85-7f0b-9bc3-2a3c4d5e6f70",
          displayName: "Ada",
        },
        accountId: "019505c8-7c85-7f0b-9bc3-2a3c4d5e6f70",
        systemIds: [],
        permissions: [],
        connectors: [],
      },
    });
  });
  await page.route("**/api/v1/dashboard", async (route) => {
    await route.fulfill({
      json: {
        observedAtEpochMillis: Date.now(),
        ageSeconds: 0,
        freshnessThresholdSeconds: 60,
        generationWatts: 4200,
        consumptionWatts: null,
        gridWatts: null,
        batteryBasisPoints: null,
        coverageBasisPoints: 10000,
        recentAlerts: [],
        ingestion: { acceptedToday: 1, rejectedToday: 0, lagSeconds: 0 },
      },
    });
  });
});

test("boots the browser application from runtime configuration", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await page.goto("/");

  await expect(
    page.getByRole("heading", { level: 1, name: /all systems/i }),
  ).toBeVisible();
  await expect(page.getByRole("banner")).toBeHidden();
  await expect(page.getByRole("navigation")).toBeVisible();
  for (const label of [
    "Dashboard",
    "Systems",
    "Statistics",
    "Seasonal",
    "Weather",
    "Manage",
    "Administration",
  ]) {
    await expect(page.getByText(label, { exact: true }).first()).toBeVisible();
  }
  expect((await page.locator("aside").boundingBox())?.width).toBe(252);
  await expect(page.locator("html")).toHaveAttribute("lang", /^(de|en)$/);
});

test("keeps navigation reachable at mobile and tablet widths", async ({
  page,
}) => {
  await page.setViewportSize({ width: 375, height: 812 });
  await page.goto("/");

  await expect(page.getByRole("banner")).toBeVisible();
  await expect(page.getByRole("navigation")).toBeHidden();
  await page.getByRole("button", { name: "Open navigation" }).click();
  await expect(page.getByRole("navigation")).toBeVisible();

  await page.setViewportSize({ width: 768, height: 1024 });
  await expect(page.getByRole("banner")).toBeHidden();
  await expect(page.getByRole("navigation")).toBeVisible();
});

test("supports keyboard navigation without critical accessibility violations", async ({
  page,
}) => {
  await page.goto("/");
  await expect(
    page.getByRole("heading", { level: 1, name: /all systems/i }),
  ).toBeVisible();
  await page.keyboard.press("Tab");
  await expect(page.getByRole("link", { name: /skip/i })).toBeFocused();

  const results = await new AxeBuilder({ page }).analyze();
  expect(results.violations).toEqual([]);
});
