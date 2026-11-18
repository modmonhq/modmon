import { fireEvent, render, screen, within } from "@testing-library/vue";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import DashboardPage from "./DashboardPage.vue";
import StatePanel from "./StatePanel.vue";
import { dashboard } from "../test-fixtures";
import type { DashboardResponse } from "../api.generated";

function serviceRow(name: string) {
  return within(screen.getByRole("heading", { name }).closest("li")!);
}

function statTone(container: Element, title: string) {
  const stat = [...container.querySelectorAll(".mm-stat")].find(
    (item) => item.querySelector(".mm-card__title")?.textContent === title,
  );
  return stat?.querySelector(".mm-stat__value .mm-tone")?.getAttribute("data-tone");
}

describe("dashboard states", () => {
  it("shows healthy services and where their developer route came from", () => {
    const { container } = render(DashboardPage, { props: { dashboard: dashboard(), stale: false } });
    const row = serviceRow("web frontend");
    expect(row.getByRole("link", { name: /web\.example\.test/ }).getAttribute("href")).toBe("https://web.example.test");
    expect(row.getByText("healthy")).toBeTruthy();
    expect(row.getByText("automatic")).toBeTruthy();
    expect(row.queryByText("no TLS")).toBeNull();
    expect(screen.getByRole("status").textContent).toBe("live");
    expect(statTone(container, "Services")).toBe("ok");
    expect(statTone(container, "Routes")).toBe("ok");
    expect(statTone(container, "Issues")).toBe("ok");
    expect(screen.getByText("all checks passed")).toBeTruthy();
    expect(screen.getByText("of 1 service routed")).toBeTruthy();
    expect(screen.queryByText("Connection to mm lost")).toBeNull();
  });

  it("greys out the optional units that are off on this machine", () => {
    const response = dashboard();
    response.project!.optional = [
      { kind: "module", name: "status", on: false, enabled: false, services: ["status/web"] },
      { kind: "job", name: "report", on: true, enabled: true, jobs: ["report"] },
    ];
    render(DashboardPage, { props: { dashboard: response, stale: false } });
    const row = serviceRow("status optional module");
    expect(row.getByText("off locally")).toBeTruthy();
    expect(row.getByText("mm dev enable status").tagName).toBe("CODE");
    expect(row.getByText("status/web")).toBeTruthy();
    expect(screen.queryByRole("heading", { name: /report/ })).toBeNull();
  });

  it("makes degraded and stale state explicit without a refresh error", () => {
    const { container } = render(DashboardPage, { props: { dashboard: dashboard("degraded"), stale: true } });
    expect(screen.getByRole("status").textContent).toBe("reconnecting");
    expect(screen.queryByRole("alert")).toBeNull();
    expect(screen.getByText("Connection to mm lost")).toBeTruthy();
    expect(serviceRow("web frontend").getByText("degraded")).toBeTruthy();
    expect(statTone(container, "Services")).toBe("bad");
    expect(screen.getByText("healthy · 1 degraded")).toBeTruthy();
  });

  it("renders loading and error state actions", async () => {
    const loading = render(StatePanel, { props: { state: "loading" } });
    expect(screen.getByRole("heading", { level: 1, name: "Connecting" })).toBeTruthy();
    loading.unmount();
    const retry = render(StatePanel, { props: { state: "error", detail: "Disconnected. Run `mm doctor`." } });
    expect(screen.getByRole("heading", { level: 1, name: "Dashboard unreachable" })).toBeTruthy();
    expect(screen.getByText("mm doctor").tagName).toBe("CODE");
    expect(screen.getByText("mm doctor").parentElement?.textContent).toBe("Disconnected. Run mm doctor.");
    await fireEvent.click(screen.getByRole("button", { name: "Try again" }));
    expect(retry.emitted().retry).toHaveLength(1);
  });

  it("supports keyboard focus and activation on Reka tabs", async () => {
    render(DashboardPage, { props: { dashboard: dashboard(), stale: false } });
    const user = userEvent.setup();
    const services = screen.getByRole("tab", { name: /^services/i });
    expect(services.getAttribute("aria-selected")).toBe("true");
    const resources = screen.getByRole("tab", { name: /data & networks/i });
    resources.focus();
    expect(document.activeElement).toBe(resources);
    await user.keyboard("[Enter]");
    expect(resources.getAttribute("aria-selected")).toBe("true");
    expect(within(screen.getByRole("tabpanel")).getByRole("heading", { name: "Networks" })).toBeTruthy();
    await user.keyboard("[ArrowRight]");
    expect(document.activeElement).toBe(screen.getByRole("tab", { name: /^activity/i }));
  });

  it("renders degraded installation and complete service failure evidence", async () => {
    const value = dashboard();
    value.installation.components[0]!.healthy = false;
    value.installation.diagnostics = [{ code: "services.ingress", severity: "error", summary: "Ingress is down" }];
    const service = value.project!.services[0]!;
    service.app = null;
    service.health = "stopped";
    service.restart_count = null;
    service.image = null;
    service.revision = null;
    service.exit_code = 137;
    service.diagnostic_code = "runtime.container.exited";
    service.routes = [];
    value.project!.runtime_available = false;
    value.project!.operations = [
      { kind: "build", state: "succeeded", requested_at_unix_ms: 1_700_000_000_000, finished_at_unix_ms: 1_700_000_083_000 },
      { kind: "deploy", state: "failed", requested_at_unix_ms: 1_700_000_001_000, error_code: "runtime.failed" },
      { kind: "reconcile", state: "running", requested_at_unix_ms: 1_700_000_002_000 },
    ];
    value.project!.diagnostics = [{ code: "config.warning", severity: "warning", summary: "Review config", suggested_action: "Run `mm config validate`" }];
    const { container } = render(DashboardPage, { props: { dashboard: value, stale: true, refreshError: "stream failed" } });

    expect(screen.getByRole("alert").textContent).toContain("stream failed");
    expect(screen.getByRole("alert").textContent).toContain("Showing the snapshot from");
    expect(screen.queryByText("Connection to mm lost")).toBeNull();
    expect(screen.getByText("control plane degraded")).toBeTruthy();
    const row = serviceRow("web");
    expect(row.queryByRole("link")).toBeNull();
    expect(row.getByText("stopped")).toBeTruthy();
    expect(row.getByText("runtime.container.exited")).toBeTruthy();
    expect(row.getByText("137")).toBeTruthy();
    expect(row.queryByText("Image")).toBeNull();
    expect(row.queryByText("Revision")).toBeNull();
    expect(row.queryByText(/restart/)).toBeNull();
    expect(screen.getByText("unavailable")).toBeTruthy();
    expect(statTone(container, "Services")).toBe("bad");
    expect(statTone(container, "Routes")).toBe("idle");
    expect(statTone(container, "Issues")).toBe("bad");
    expect(screen.getByText("1 error · 1 warning")).toBeTruthy();
    // The newest operation is summarised whatever order the API lists them in.
    expect(statTone(container, "Last operation")).toBe("info");

    const user = userEvent.setup();
    await user.click(screen.getByRole("tab", { name: /^activity/i }));
    const activity = within(screen.getByRole("tabpanel"));
    expect(activity.getByText("runtime.failed")).toBeTruthy();
    expect(activity.getByText("running")).toBeTruthy();
    expect(activity.getByText("1m 23s")).toBeTruthy();
    await user.click(screen.getByRole("tab", { name: /doctor/i }));
    const doctor = within(screen.getByRole("tabpanel"));
    expect(doctor.getByText("Review config")).toBeTruthy();
    expect(doctor.getByText("mm config validate").tagName).toBe("CODE");
    expect(doctor.getByText("Ingress is down")).toBeTruthy();
    expect(doctor.getByText("2 findings")).toBeTruthy();
  });

  it("shows empty project collections without inventing runtime state", async () => {
    const value = dashboard();
    value.project!.services = [];
    value.project!.resources = [];
    value.project!.operations = [];
    const { container } = render(DashboardPage, { props: { dashboard: value, stale: false } });
    expect(screen.getByText("No services")).toBeTruthy();
    expect(statTone(container, "Services")).toBe("idle");
    expect(screen.getByText("none reported")).toBeTruthy();
    expect(statTone(container, "Last operation")).toBe("idle");
    expect(screen.getByText("nothing recorded")).toBeTruthy();

    const user = userEvent.setup();
    await user.click(screen.getByRole("tab", { name: /data & networks/i }));
    expect(screen.getByText("No volumes")).toBeTruthy();
    expect(screen.getByText("No networks")).toBeTruthy();
    await user.click(screen.getByRole("tab", { name: /^activity/i }));
    expect(screen.getByText("No activity")).toBeTruthy();
    await user.click(screen.getByRole("tab", { name: /doctor/i }));
    expect(screen.getByText("No issues")).toBeTruthy();
  });

  it("renders installation scope for ready and unconfigured hosts", () => {
    const ready = dashboard();
    ready.project = null;
    const first = render(DashboardPage, { props: { dashboard: ready, stale: false } });
    expect(screen.getByRole("heading", { level: 1, name: "Installation" })).toBeTruthy();
    expect(screen.getByText("docker compose")).toBeTruthy();
    expect(screen.getByRole("heading", { name: "Projects" })).toBeTruthy();
    expect(screen.getByText("1 project")).toBeTruthy();
    const projects = within(screen.getByRole("heading", { name: "Projects" }).closest("section")!);
    expect(projects.getByRole("link", { name: "Example" }).getAttribute("href")).toBe("https://mm.example.test");
    expect(projects.getByText("mm.example.test")).toBeTruthy();
    expect(screen.getByText("installed")).toBeTruthy();
    expect(screen.getByText("stack 1.0.0")).toBeTruthy();
    expect(statTone(first.container, "Control plane")).toBe("ok");
    expect(screen.queryByRole("heading", { name: "Needs attention" })).toBeNull();
    expect(screen.queryByRole("navigation", { name: "Registered projects" })).toBeTruthy();
    first.unmount();

    const empty: DashboardResponse = {
      ...dashboard(),
      scope: { kind: "installation" },
      project: null,
      projects: [],
      installation: {
        ...dashboard().installation,
        installed: false,
        runtime: null,
        stack_version: null,
        components: [],
        diagnostics: [{ code: "services.missing", severity: "warning", summary: "Shared services are not installed" }],
      },
    };
    const second = render(DashboardPage, { props: { dashboard: empty, stale: false } });
    expect(screen.getByText("runtime not selected")).toBeTruthy();
    expect(screen.getByText("setup needed")).toBeTruthy();
    expect(screen.getByText("not installed")).toBeTruthy();
    expect(screen.getByText("No shared services installed")).toBeTruthy();
    expect(screen.queryByText(/^stack /)).toBeNull();
    expect(screen.getByText("mm init").closest("p")?.textContent).toBe("Run mm init in a project to add it.");
    expect(screen.queryByRole("navigation", { name: "Registered projects" })).toBeNull();
    expect(within(screen.getByRole("heading", { name: "Needs attention" }).closest("section")!).getByText("Shared services are not installed")).toBeTruthy();
    expect(statTone(second.container, "Issues")).toBe("warn");
    expect(statTone(second.container, "Projects")).toBe("idle");
  });
});
