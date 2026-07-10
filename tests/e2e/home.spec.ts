import { expect, test } from "@playwright/test";

test("boots the browser application from runtime configuration", async ({
  page,
}) => {
  await page.goto("/");

  await expect(
    page.getByRole("heading", { level: 1, name: "PVLog" }),
  ).toBeVisible();
  await expect(page.locator("html")).toHaveAttribute("lang", /^(de|en)$/);
});
