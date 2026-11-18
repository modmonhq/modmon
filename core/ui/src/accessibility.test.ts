import { render } from "@testing-library/vue";
import userEvent from "@testing-library/user-event";
import axe from "axe-core";
import { expect, it } from "vitest";
import DashboardPage from "./components/DashboardPage.vue";
import StatePanel from "./components/StatePanel.vue";
import { dashboard } from "./test-fixtures";

async function violations(container: Element) {
  return (await axe.run(container)).violations;
}

it("has no automated accessibility violations", async () => {
  const { container } = render(DashboardPage, { props: { dashboard: dashboard(), stale: false } });
  expect(await violations(container)).toEqual([]);
});

it("stays accessible when degraded, stale, and failing to refresh", async () => {
  const value = dashboard("degraded");
  value.installation.components[0]!.healthy = false;
  const service = value.project!.services[0]!;
  service.exit_code = 137;
  service.diagnostic_code = "runtime.container.exited";
  service.restart_count = 3;
  value.project!.operations = [
    { kind: "reconcile", state: "running", requested_at_unix_ms: 1_700_000_002_000 },
    { kind: "deploy", state: "failed", requested_at_unix_ms: 1_700_000_001_000, finished_at_unix_ms: 1_700_000_009_000, error_code: "runtime.failed" },
  ];
  value.project!.diagnostics = [{ code: "config.warning", severity: "warning", summary: "Review config", suggested_action: "Run `mm config validate`" }];
  const { container, getByRole } = render(DashboardPage, { props: { dashboard: value, stale: true, refreshError: "stream failed" } });
  expect(await violations(container)).toEqual([]);

  const user = userEvent.setup();
  for (const name of [/data & networks/i, /^activity/i, /doctor/i]) {
    await user.click(getByRole("tab", { name }));
    expect(await violations(container)).toEqual([]);
  }
});

it("keeps the installation scope and the empty states accessible", async () => {
  const value = dashboard();
  value.project = null;
  value.installation.diagnostics = [{ code: "services.cap", severity: "warning", summary: "Cap is not running" }];
  const ready = render(DashboardPage, { props: { dashboard: value, stale: false } });
  expect(await violations(ready.container)).toEqual([]);
  ready.unmount();

  value.projects = [];
  value.installation = { ...value.installation, installed: false, runtime: null, components: [], diagnostics: [] };
  const empty = render(DashboardPage, { props: { dashboard: value, stale: true } });
  expect(await violations(empty.container)).toEqual([]);
});

it("keeps the loading and error screens accessible", async () => {
  const loading = render(StatePanel, { props: { state: "loading" } });
  expect(await violations(loading.container)).toEqual([]);
  loading.unmount();
  const failed = render(StatePanel, { props: { state: "error", detail: "Dashboard request failed (503)" } });
  expect(await violations(failed.container)).toEqual([]);
});
