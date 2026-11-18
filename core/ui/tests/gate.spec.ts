import { expect, test } from "@playwright/test";
import { readFileSync } from "node:fs";

const gateUrl = process.env.MM_GATE_URL;
const username = process.env.MM_GATE_USERNAME;
const passwordFile = process.env.MM_GATE_PASSWORD_FILE;

test.use({ trace: "off" });

test("live trusted-HTTPS gate requires Cap and credentials, then revokes logout", async ({
  page,
  context,
}) => {
  test.setTimeout(60_000);
  test.skip(
    !gateUrl || !username || !passwordFile,
    "requires MM_GATE_URL, MM_GATE_USERNAME, and MM_GATE_PASSWORD_FILE",
  );
  const password = readFileSync(passwordFile!, "utf8").replace(/[\r\n]+$/, "");
  const browserErrors: string[] = [];
  page.on("console", (message) => {
    if (message.type() === "error") browserErrors.push(message.text());
  });
  page.on("requestfailed", (request) => {
    browserErrors.push(`${request.url()}: ${request.failure()?.errorText ?? "request failed"}`);
  });

  await page.goto(gateUrl!);
  await expect(page.getByRole("heading", { name: "Access required" })).toBeVisible();
  expect(page.url()).toMatch(/^https:\/\//);
  const loginUrl = new URL(page.url());
  const policyId = loginUrl.pathname.split("/").at(-1);
  expect(policyId).toMatch(/^[0-9a-f-]{36}$/);

  await page.getByLabel("Username").fill(username!);
  await page.getByRole("button", { name: "Click to verify you're a human" }).click();
  try {
    await expect(page.locator("#cap-token")).not.toHaveValue("", { timeout: 30_000 });
  } catch {
    throw new Error(`Cap did not solve: ${browserErrors.join(" | ")}`);
  }
  const passwordInput = page.getByLabel("Password");
  let authenticated = false;
  try {
    await passwordInput.fill(password);
    const loginResponsePromise = page.waitForResponse(
      (response) =>
        response.request().method() === "POST" &&
        new URL(response.url()).pathname === loginUrl.pathname,
    );
    await page.getByRole("button", { name: "Continue", exact: true }).click();
    const loginResponse = await loginResponsePromise;
    expect(loginResponse.status(), "gate login response status").toBe(303);
    await expect(page).toHaveURL(gateUrl!);
    authenticated = true;
  } finally {
    if (!authenticated && (await passwordInput.count()) > 0) {
      await passwordInput.fill("");
    }
  }

  const cookies = await context.cookies();
  const session = cookies.find((cookie) => cookie.name.startsWith("__Host-mm_gate_"));
  const csrf = cookies.find((cookie) => cookie.name.startsWith("__Host-mm_gate_csrf_"));
  expect(session).toMatchObject({ secure: true, httpOnly: true, sameSite: "Lax" });
  expect(csrf).toMatchObject({ secure: true, httpOnly: false, sameSite: "Strict" });
  expect((session?.expires ?? 0) - Date.now() / 1000).toBeGreaterThan(6.9 * 86_400);

  const logout = await page.evaluate(
    async ({ id, token }) => {
      const body = new window.URLSearchParams({ csrf_token: token });
      const response = await fetch(`/_mm/gate/logout/${id}`, {
        method: "POST",
        headers: { "content-type": "application/x-www-form-urlencoded" },
        body,
      });
      return response.status;
    },
    { id: policyId!, token: csrf!.value },
  );
  expect(logout).toBe(200);
  await page.goto(gateUrl!);
  await expect(page.getByRole("heading", { name: "Access required" })).toBeVisible();
});
