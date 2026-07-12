import { expect, test } from "@playwright/test";
import { readFileSync } from "node:fs";

const catalog = JSON.parse(
  readFileSync("assets/equipment-catalog/catalog-v1.json", "utf8"),
) as { revision: string; inverters: unknown[]; solarModules: unknown[] };

test("reviews edited prefills and keeps validation recoverable", async ({
  page,
}) => {
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
        systemIds: ["019505c8-7c85-7f0b-9bc3-2a3c4d5e6f71"],
        permissions: [],
        connectors: [],
      },
    }),
  );
  await page.route("**/api/v1/equipment-catalog/inverters**", (route) =>
    route.fulfill({
      json: {
        revision: catalog.revision,
        total: catalog.inverters.length,
        offset: 0,
        limit: 25,
        items: catalog.inverters,
      },
    }),
  );
  await page.route("**/api/v1/equipment-catalog/solar-modules**", (route) =>
    route.fulfill({
      json: {
        revision: catalog.revision,
        total: catalog.solarModules.length,
        offset: 0,
        limit: 25,
        items: catalog.solarModules,
      },
    }),
  );
  await page.route(
    "**/api/v1/accounts/**/systems/**/inverters",
    async (route) =>
      route.request().method() === "POST"
        ? route.fulfill({
            status: 422,
            json: { detail: "totalPeakPowerWatts" },
          })
        : route.fulfill({ json: [] }),
  );
  await page.goto("/administration");
  const confirmation = page
    .getByRole("heading", { name: "Confirm equipment snapshot" })
    .locator("..");
  const inverterResults = confirmation.getByRole("listbox").first();
  await expect(inverterResults).toBeVisible();
  await inverterResults.selectOption({ index: 1 });
  const snapshot = page.getByLabel(
    "Editable inverter specification snapshot (JSON)",
  );
  await expect(snapshot).toHaveValue(/catalog_copied/);
  await snapshot.fill(
    (await snapshot.inputValue()).replace(
      "Symo GEN24 10.0",
      "Customized GEN24",
    ),
  );
  await page.getByRole("button", { name: "Save confirmed equipment" }).click();
  await expect(page.getByRole("alert")).toContainText("invalid");
  await expect(
    page.getByRole("button", { name: "Enter inverter manually" }).first(),
  ).toBeEnabled();
});
