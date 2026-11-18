import { expect, test } from "@playwright/test";

test("installation dashboard is keyboard reachable and CSP protected", async ({ page }) => {
  const response = await page.goto("/");
  expect(response?.headers()["content-security-policy"]).toContain("default-src 'none'");
  await expect(page.getByRole("heading", { level: 1 })).toBeVisible();
  await page.keyboard.press("Tab");
  await expect(page.locator(":focus")).toBeVisible();
  await expect(page.locator(":focus")).toHaveText("Skip to dashboard");
  expect(await page.locator("script[src^='http']").count()).toBe(0);
  expect(await page.locator("script:not([src])").count()).toBe(0);
});

test("installation dashboard states health first and fits the viewport", async ({ page }) => {
  // A blocked style, font, or script would surface here as a CSP console error.
  const errors: string[] = [];
  page.on("console", (message) => {
    if (message.type() === "error") errors.push(message.text());
  });
  await page.goto("/");

  await expect(page.getByRole("heading", { level: 1, name: "Installation" })).toBeVisible();
  await expect(page.getByRole("status")).toHaveText("live");
  await expect(page.getByText("control plane degraded")).toBeVisible();
  await expect(page.getByRole("heading", { name: "Needs attention" })).toBeVisible();
  await expect(page.getByText("mm dev services status")).toBeVisible();
  await expect(page.getByRole("heading", { name: "Control plane" })).toBeVisible();
  const project = page.getByRole("main").getByRole("link", { name: "example-with-a-long-project-name" });
  await expect(project).toHaveAttribute("href", "https://mm.example.test");

  // Tokens come from the shared kit and fonts from the embedded assets.
  await expect(page.locator("html")).toHaveCSS("background-color", "rgb(7, 9, 8)");
  expect(await page.evaluate(() => document.fonts.ready.then(() => document.fonts.check('600 16px "Geist"')))).toBe(true);
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
  expect(errors).toEqual([]);
});
