import { expect, test } from "@playwright/test";

const keyId = "019505c8-7c85-7f0b-9bc3-2a3c4d5e6f72";
const secret = `pvlog_${keyId}.${"a".repeat(64)}`;

test("creates and revokes a least-privilege account API key", async ({
  page,
}) => {
  let keys: unknown[] = [];
  await page.route("**/api/v1/**", (route) => route.fulfill({ json: [] }));
  await page.route("**/api/v1/session", (route) =>
    route.fulfill({
      json: {
        authenticated: true,
        user: {
          id: "019505c8-7c85-7f0b-9bc3-2a3c4d5e6f70",
          displayName: "Ada",
        },
        accountId: "019505c8-7c85-7f0b-9bc3-2a3c4d5e6f70",
        systemIds: [],
        permissions: ["credential_manage"],
        connectors: [],
      },
    }),
  );
  await page.route("**/api/v1/account/api-keys**", (route) => {
    if (route.request().method() === "POST") {
      const credential = metadata();
      keys = [credential];
      return route.fulfill({
        status: 201,
        json: { apiKey: secret, credential },
      });
    }
    if (route.request().method() === "DELETE") {
      keys = [];
      return route.fulfill({ status: 204, body: "" });
    }
    return route.fulfill({ json: keys });
  });
  await page.route("**/api/v1/account/profile", (route) =>
    route.fulfill({
      json: {
        id: "019505c8-7c85-7f0b-9bc3-2a3c4d5e6f70",
        email: "ada@example.test",
        displayName: "Ada",
      },
    }),
  );
  await page.goto("/account/api-keys");
  await expect(page).toHaveURL(/\/account$/u);
  await expect(
    page.getByRole("heading", { name: "Account", exact: true }),
  ).toBeVisible();
  await page
    .getByRole("region", { name: "Account API keys" })
    .getByLabel("Name")
    .fill("Home uploader");
  await page.getByLabel("Upload PV data").check();
  await page.getByRole("button", { name: "Create API key" }).click();
  await expect(page.getByRole("textbox", { name: "New API key" })).toHaveValue(
    secret,
  );
  await page.getByRole("button", { name: "Close one-time API key" }).click();
  await expect(page.getByText("Home uploader")).toBeVisible();
  await page.getByRole("button", { name: "Revoke" }).click();
  await page.getByRole("button", { name: "Revoke API key" }).click();
  await expect(page.getByText("The API key was revoked.")).toBeVisible();
});

function metadata() {
  return {
    id: keyId,
    name: "Home uploader",
    scopes: ["telemetry:write"],
    createdAtEpochMillis: 1_780_000_000_000,
    expiresAtEpochMillis: null,
    revokedAtEpochMillis: null,
  };
}
