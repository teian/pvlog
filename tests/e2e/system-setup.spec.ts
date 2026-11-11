import AxeBuilder from "@axe-core/playwright";
import { expect, test, type Page } from "@playwright/test";

const accountId = "019505c8-7c85-7f0b-9bc3-2a3c4d5e6f70";
const systemId = "019505c8-7c85-7f0b-9bc3-2a3c4d5e6f71";
const sourceId = "019505c8-7c85-7f0b-9bc3-2a3c4d5e6f72";
const apiKey = "pvlog_one-time-secret";

interface Backend {
  /** Systems the session reports; grows when the setup creates one. */
  systemIds: string[];
  /** Whether a test reading was sent through the data source. */
  tested: boolean;
  created: unknown[];
}

async function stubBackend(page: Page): Promise<Backend> {
  const backend: Backend = { systemIds: [], tested: false, created: [] };
  await page.route("**/api/v1/session", (route) =>
    route.fulfill({
      json: {
        authenticated: true,
        user: { id: accountId, displayName: "Ada" },
        accountId,
        systemIds: backend.systemIds,
        permissions: ["system_manage", "system_read", "telemetry_read"],
        connectors: [],
      },
    }),
  );
  await page.route("**/api/v1/systems", async (route) => {
    backend.created.push(route.request().postDataJSON());
    backend.systemIds = [systemId];
    await route.fulfill({
      status: 201,
      json: {
        id: systemId,
        accountId,
        name: "Roof",
        timezone: "Europe/Berlin",
        statusIntervalSeconds: 300,
        visibility: "private",
        lifecycle: "active",
        version: 1,
        createdAt: 1,
        updatedAt: 1,
      },
    });
  });
  await page.route("**/api/v1/data-source-profiles", (route) =>
    route.fulfill({
      json: [
        {
          id: "smalog",
          displayName: "smalog",
          issuesCredential: true,
          snippetKinds: ["smalog-config", "environment"],
        },
        {
          id: "generic",
          displayName: "Generic uploader",
          issuesCredential: true,
          snippetKinds: ["curl"],
        },
      ],
    }),
  );
  await page.route(`**/api/v1/systems/${systemId}/data-sources`, (route) =>
    route.fulfill({
      status: 201,
      json: {
        dataSource: {
          id: sourceId,
          systemId,
          name: "smalog",
          profile: "smalog",
          state: "never_connected",
          serverUrl: "https://pvlog.example",
          credentialId: "019505c8-7c85-7f0b-9bc3-2a3c4d5e6f73",
          readingsLastDay: 0,
          createdAtEpochMillis: 1,
          updatedAtEpochMillis: 1,
        },
        configuration: {
          serverUrl: "https://pvlog.example",
          systemId,
          apiKey,
          snippets: [
            {
              id: "smalog-config",
              title: "config.toml",
              language: "toml",
              content: `[pvlog]\nenabled = true\nurl = "https://pvlog.example"\nsystem_id = "${systemId}"\napi_key = "\${PVLOG_API_KEY}"\n`,
            },
            {
              id: "environment",
              title: "Environment",
              language: "dotenv",
              content: `PVLOG_API_KEY=${apiKey}\n`,
            },
          ],
        },
      },
    }),
  );
  await page.route(
    `**/api/v1/systems/${systemId}/data-sources/${sourceId}/test-reading`,
    async (route) => {
      backend.tested = true;
      await route.fulfill({
        status: 201,
        json: {
          id: "019505c8-7c85-7f0b-9bc3-2a3c4d5e6f75",
          systemId,
          dataSourceId: sourceId,
          measuredAtEpochMillis: 1_800_000_000_000,
          receivedAtEpochMillis: 1_800_000_000_000,
          values: { generation_power: 100 },
          test: true,
        },
      });
    },
  );
  await page.route(`**/api/v1/systems/${systemId}/first-data`, (route) =>
    route.fulfill({
      json: {
        systemId,
        received: false,
        reading: backend.tested
          ? {
              id: "019505c8-7c85-7f0b-9bc3-2a3c4d5e6f75",
              measuredAtEpochMillis: 1_800_000_000_000,
              receivedAtEpochMillis: 1_800_000_000_000,
              generationPowerWatts: 100,
              dataSourceId: sourceId,
              dataSourceName: "smalog",
              test: true,
            }
          : null,
      },
    }),
  );
  return backend;
}

test("leads from the first sign-in to a verified test reading", async ({
  page,
}) => {
  const backend = await stubBackend(page);
  await page.setViewportSize({ width: 1440, height: 1000 });

  // Someone who may manage systems and has none lands in the setup.
  await page.goto("/");
  await expect(page).toHaveURL(/\/setup$/);
  await expect(
    page.getByRole("heading", { level: 1, name: "Set up a system" }),
  ).toBeVisible();

  await page.getByLabel("System name").fill("Roof");
  await page
    .getByRole("button", { name: "Create system and continue" })
    .click();
  expect(backend.created[0]).toMatchObject({ name: "Roof" });

  await expect(
    page.getByRole("group", { name: "Which logger sends the data?" }),
  ).toBeVisible();
  await expect(page.getByRole("radio", { name: /smalog/ })).toBeChecked();
  await page.getByRole("button", { name: "Create data source" }).click();

  await expect(page.getByRole("textbox", { name: "API key" })).toHaveValue(
    apiKey,
  );
  await expect(page.getByRole("textbox", { name: "System ID" })).toHaveValue(
    systemId,
  );
  await expect(page.getByText("Copy this configuration now")).toBeVisible();
  const results = await new AxeBuilder({ page }).analyze();
  expect(results.violations).toEqual([]);

  await page
    .getByRole("button", { name: "I have copied the configuration" })
    .click();
  await expect(page.getByRole("status")).toContainText(
    "Waiting for the first reading",
  );
  await page.getByRole("button", { name: "Send test reading" }).click();
  await expect(page.getByText("Test reading received")).toBeVisible();
  await expect(page.getByText("100 W")).toBeVisible();
  await expect(page.getByRole("textbox", { name: "API key" })).toHaveCount(0);
});

test("the setup form fits a phone and is accessible", async ({ page }) => {
  await stubBackend(page);
  await page.setViewportSize({ width: 375, height: 812 });
  await page.goto("/setup");
  await expect(page.getByLabel("System name")).toBeVisible();
  const overflow = await page.evaluate(
    () => document.documentElement.scrollWidth - window.innerWidth,
  );
  expect(overflow).toBeLessThanOrEqual(0);
  const results = await new AxeBuilder({ page }).analyze();
  expect(results.violations).toEqual([]);
});

test("the former setup address leads to the guided setup", async ({ page }) => {
  await stubBackend(page);
  await page.goto("/onboarding");
  await expect(page).toHaveURL(/\/setup$/);
});
