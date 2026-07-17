import { expect, test } from "@playwright/test";
import AxeBuilder from "@axe-core/playwright";

const accountId = "019505c8-7c85-7f0b-9bc3-2a3c4d5e6f70";

test.beforeEach(async ({ page }) => {
  await page.route("**/api/v1/**", (route) => route.fulfill({ json: [] }));
  await page.route("**/api/v1/session", (route) =>
    route.fulfill({
      json: {
        authenticated: true,
        user: { id: accountId, displayName: "Frank Gehann" },
        accountId,
        systemIds: [],
        permissions: [],
        connectors: [],
      },
    }),
  );
  await page.route(`**/api/v1/accounts/${accountId}/alerts`, (route) =>
    route.fulfill({
      json: [
        {
          id: "019505c8-7c85-7f0b-9bc3-2a3c4d5e6f71",
          name: "Low yield vs. forecast",
          kind: "performance_below",
          timezone: "Europe/Berlin",
          enabled: true,
          condition: { percentage: 70 },
        },
      ],
    }),
  );
  await page.route("**/api/v1/admin/users", (route) =>
    route.fulfill({
      json: [
        {
          id: accountId,
          email: "frank@code-works.dev",
          displayName: "Frank Gehann",
          status: "active",
          emailVerifiedAt: 1_720_000_000_000,
          disabledAt: null,
          lockedUntil: null,
          createdAt: 1_720_000_000_000,
          updatedAt: 1_720_000_000_000,
        },
      ],
    }),
  );
  await page.route(`**/api/v1/accounts/${accountId}/roles`, (route) =>
    route.fulfill({
      json: [
        {
          id: "019505c8-7c85-7f0b-9bc3-2a3c4d5e6f72",
          name: "Administrator",
          kind: "built_in",
          permissions: [],
          parentRoleIds: [],
          version: 1,
          createdAt: 1_720_000_000_000,
          updatedAt: 1_720_000_000_000,
        },
      ],
    }),
  );
  await page.route(
    `**/api/v1/accounts/${accountId}/role-assignments?principalType=user*`,
    (route) =>
      route.fulfill({
        json: [
          {
            id: "019505c8-7c85-7f0b-9bc3-2a3c4d5e6f73",
            roleId: "019505c8-7c85-7f0b-9bc3-2a3c4d5e6f72",
            principalType: "user",
            principalId: accountId,
            accountId,
            systemId: null,
            expiresAt: null,
          },
        ],
      }),
  );
});

test("renders users and roles as the compact administration table", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await page.goto("/administration?section=users");

  await expect(
    page.getByRole("heading", { name: "Users & Roles" }),
  ).toBeVisible();
  await expect(page.getByRole("button", { name: "Invite User" })).toBeVisible();
  await expect(page.getByText("frank@code-works.dev")).toBeVisible();
  await expect(page.getByRole("combobox", { name: "Role" })).toHaveValue(
    "019505c8-7c85-7f0b-9bc3-2a3c4d5e6f72",
  );
  await expect(
    page.getByRole("button", { name: "Delete Frank Gehann" }),
  ).toBeVisible();

  const accessibility = await new AxeBuilder({ page }).analyze();
  expect(accessibility.violations).toEqual([]);

  await page.setViewportSize({ width: 375, height: 812 });
  await expect(page.getByText("frank@code-works.dev")).toBeVisible();
  await expect(page.getByRole("combobox", { name: "Role" })).toBeVisible();
});

test("shows the seeded instance administrator without an account", async ({
  page,
}) => {
  const instanceRoleId = "019505c8-7c85-7f0b-9bc3-2a3c4d5e6f74";
  await page.route("**/api/v1/session", (route) =>
    route.fulfill({
      json: {
        authenticated: true,
        user: { id: accountId, displayName: "PVLog Developer" },
        accountId: null,
        systemIds: [],
        permissions: ["role_manage"],
        connectors: [],
      },
    }),
  );
  await page.route("**/api/v1/admin/roles", (route) =>
    route.fulfill({
      json: [
        {
          id: instanceRoleId,
          name: "instance_administrator",
          kind: "built_in:InstanceAdministrator",
          permissions: ["role_manage"],
          parentRoleIds: [],
          version: 1,
          createdAt: 1_720_000_000_000,
          updatedAt: 1_720_000_000_000,
        },
      ],
    }),
  );
  await page.route(
    "**/api/v1/admin/role-assignments?principalType=user*",
    (route) =>
      route.fulfill({
        json: [
          {
            id: "019505c8-7c85-7f0b-9bc3-2a3c4d5e6f75",
            roleId: instanceRoleId,
            principalType: "user",
            principalId: accountId,
            accountId: null,
            systemId: null,
            expiresAt: null,
          },
        ],
      }),
  );

  await page.goto("/administration?section=users");

  const role = page.getByRole("combobox", { name: "Role" });
  await expect(role).toBeEnabled();
  await expect(role).toHaveValue(instanceRoleId);
  await expect(role.getByRole("option", { name: "Admin" })).toBeAttached();
});

test("matches the dedicated administration frame at desktop and mobile widths", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await page.goto("/administration?section=alert-rules");

  await expect(
    page.getByRole("heading", { level: 1, name: "Administration" }),
  ).toBeVisible();
  await expect(
    page.getByRole("navigation", { name: "Administration" }),
  ).toBeVisible();
  await expect(page.getByRole("link", { name: "Alert Rules" })).toHaveAttribute(
    "aria-current",
    "page",
  );
  expect((await page.locator("aside").boundingBox())?.width).toBe(252);
  await expect(page.getByText("Low yield vs. forecast")).toBeVisible();
  const accessibility = await new AxeBuilder({ page }).analyze();
  expect(accessibility.violations).toEqual([]);

  await page.setViewportSize({ width: 375, height: 812 });
  await expect(
    page.getByRole("navigation", { name: "Administration" }),
  ).toBeHidden();
  await page.getByRole("button", { name: "Open navigation" }).click();
  await expect(
    page.getByRole("navigation", { name: "Administration" }),
  ).toBeVisible();

  await page.setViewportSize({ width: 768, height: 1024 });
  await expect(
    page.getByRole("navigation", { name: "Administration" }),
  ).toBeVisible();
});
