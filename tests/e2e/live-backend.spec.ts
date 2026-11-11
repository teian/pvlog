import {
  expect,
  test,
  type ConsoleMessage,
  type Page,
  type Response,
} from "@playwright/test";

/**
 * Opt-in end-to-end pass over the complete authenticated UI flow against a
 * running backend. Every other spec in this directory stubs `/api/v1`, so this
 * is the only place where routing, session handling, and the reporting
 * endpoints are exercised together.
 *
 * Enable it with a reachable backend and real credentials:
 *   PVLOG_E2E_LIVE=1 PVLOG_E2E_EMAIL=... PVLOG_E2E_PASSWORD=... bun run test:e2e
 */
const live = process.env.PVLOG_E2E_LIVE === "1";
const email = process.env.PVLOG_E2E_EMAIL ?? "";
const password = process.env.PVLOG_E2E_PASSWORD ?? "";

/** API responses the UI is expected to tolerate while a feature is unavailable. */
const TOLERATED_STATUSES = new Set([401, 403, 404]);

/** The Vite development server compiles routes on first visit, so allow for that. */
const RENDER_TIMEOUT_MILLIS = 20_000;

interface Diagnostics {
  consoleErrors: string[];
  failedRequests: string[];
}

function collectDiagnostics(page: Page): Diagnostics {
  const diagnostics: Diagnostics = { consoleErrors: [], failedRequests: [] };
  page.on("console", (message: ConsoleMessage) => {
    if (message.type() === "error") {
      diagnostics.consoleErrors.push(message.text());
    }
  });
  page.on("response", (response: Response) => {
    const url = response.url();
    if (url.includes("/api/v1/") && response.status() >= 400) {
      diagnostics.failedRequests.push(
        `${response.status()} ${new URL(url).pathname}`,
      );
    }
  });
  page.on("pageerror", (error) => {
    diagnostics.consoleErrors.push(`pageerror: ${error.message}`);
  });
  return diagnostics;
}

async function signIn(page: Page): Promise<void> {
  await page.goto("/login");
  await page.getByLabel("Email address").fill(email);
  await page.getByLabel("Password", { exact: true }).fill(password);
  await page.getByRole("button", { name: "Sign in" }).click();
  await page.waitForURL((url) => !url.pathname.startsWith("/login"), {
    timeout: 15_000,
  });
}

test.describe("live backend", () => {
  test.skip(!live, "set PVLOG_E2E_LIVE=1 with a running backend to enable");
  test.skip(
    !email || !password,
    "PVLOG_E2E_EMAIL and PVLOG_E2E_PASSWORD are required",
  );
  test.describe.configure({ mode: "serial" });

  test("walks every navigation target of the authenticated shell", async ({
    page,
  }) => {
    test.setTimeout(120_000);
    const diagnostics = collectDiagnostics(page);
    await page.setViewportSize({ width: 1440, height: 900 });
    await signIn(page);

    const session = await page.evaluate(async () => {
      const response = await fetch("/api/v1/session", {
        credentials: "include",
      });
      return (await response.json()) as { systemIds: string[] };
    });
    const systemId = session.systemIds.at(-1);
    expect(
      systemId,
      "the signed-in account needs at least one system",
    ).toBeTruthy();

    // Administration renders in its own frame and deliberately drops the shell navigation.
    const routes = [
      { path: "/", shell: true },
      { path: "/systems", shell: true },
      { path: "/statistics", shell: true },
      { path: "/seasonal", shell: true },
      { path: "/weather", shell: true },
      { path: "/administration", shell: false },
      { path: "/account", shell: true },
      { path: `/systems/${systemId}`, shell: true },
      { path: `/systems/${systemId}/forecast`, shell: true },
      { path: `/systems/${systemId}/data-quality`, shell: true },
    ];

    for (const route of routes) {
      await test.step(`renders ${route.path}`, async () => {
        await page.goto(route.path);
        if (route.shell) {
          await expect(
            page.getByRole("navigation", { name: "Primary navigation" }),
          ).toBeVisible({ timeout: RENDER_TIMEOUT_MILLIS });
        }
        await expect(page.locator("main").first()).toBeVisible({
          timeout: RENDER_TIMEOUT_MILLIS,
        });
      });
    }

    expect(
      diagnostics.failedRequests.filter(
        (entry) => !TOLERATED_STATUSES.has(Number(entry.split(" ")[0])),
      ),
      "server-side API failures",
    ).toEqual([]);
    expect(
      diagnostics.consoleErrors.filter(
        (entry) => !entry.startsWith("Failed to load resource"),
      ),
      "browser console errors",
    ).toEqual([]);
    test.info().annotations.push({
      type: "tolerated-api-failures",
      description:
        [...new Set(diagnostics.failedRequests)].join(", ") || "none",
    });
  });

  test("reports the seeded two-year statistics", async ({ page }) => {
    const diagnostics = collectDiagnostics(page);
    await signIn(page);
    await page.goto("/statistics");

    await expect(page.getByRole("heading", { level: 1 })).toBeVisible();
    await expect(page.locator("main")).not.toContainText("NaN");
    expect(diagnostics.consoleErrors, "browser console errors").toEqual([]);
  });
});
