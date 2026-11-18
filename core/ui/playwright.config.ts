import { defineConfig } from "@playwright/test";

const liveUrl = process.env.MM_DASHBOARD_URL;
const certificateSpki = process.env.MM_GATE_CERT_SPKI;

export default defineConfig({
  testDir: "./tests",
  reporter: "line",
  use: {
    baseURL: liveUrl ?? "http://127.0.0.1:42832",
    ignoreHTTPSErrors: false,
    trace: "retain-on-failure",
    launchOptions: certificateSpki ? {
      args: [`--ignore-certificate-errors-spki-list=${certificateSpki}`],
    } : undefined,
  },
  webServer: liveUrl ? undefined : {
    command: "node tests/fixture-server.mjs",
    url: "http://127.0.0.1:42832",
    reuseExistingServer: false,
  },
  projects: [
    { name: "desktop", use: { viewport: { width: 1440, height: 900 } } },
    { name: "mobile", use: { viewport: { width: 390, height: 844 } } },
  ],
});
