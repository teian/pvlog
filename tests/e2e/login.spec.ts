import AxeBuilder from "@axe-core/playwright";
import { expect, test } from "@playwright/test";

const brandDescription =
  "Real-time production monitoring, fault detection, and reporting for your PV installations.";

test.beforeEach(async ({ page }) => {
  await page.route("**/api/v1/session", async (route) => {
    await route.fulfill({
      json: {
        authenticated: false,
        user: null,
        accountId: null,
        systemIds: [],
        permissions: [],
        connectors: [
          {
            id: "oidc",
            name: "SSO",
            authorizationUrl: "https://identity.example/authorize",
          },
        ],
      },
    });
  });
});

test("matches the split desktop login composition", async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await page.goto("/login");

  const brandPanel = page
    .getByText(brandDescription)
    .locator("..")
    .locator("..");
  await expect(
    page.getByRole("heading", { level: 1, name: "Sign in" }),
  ).toBeVisible();
  await expect(page.getByLabel("Email address")).toHaveAttribute(
    "placeholder",
    "you@company.com",
  );
  await expect(
    page.getByRole("link", { name: "Sign in with SSO" }),
  ).toBeVisible();

  const brandBox = await brandPanel.boundingBox();
  expect(brandBox?.width).toBeCloseTo(634, -1);
  expect(brandBox?.height).toBe(900);

  const results = await new AxeBuilder({ page }).analyze();
  expect(results.violations).toEqual([]);
});

test("collapses the brand panel on a narrow viewport", async ({ page }) => {
  await page.setViewportSize({ width: 375, height: 812 });
  await page.goto("/login");

  await expect(page.getByText(brandDescription)).toBeHidden();
  await expect(
    page.getByRole("heading", { level: 1, name: "Sign in" }),
  ).toBeVisible();
  await expect(page.getByRole("button", { name: "Sign in" })).toBeVisible();
});
