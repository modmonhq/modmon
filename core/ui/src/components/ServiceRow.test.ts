import { render, screen, within } from "@testing-library/vue";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import DashboardPage from "./DashboardPage.vue";
import ServiceRow from "./ServiceRow.vue";
import { dashboard } from "../test-fixtures";

function service() {
  return dashboard().project!.services[0]!;
}

describe("service row", () => {
  it("draws one pip per desired replica by readiness", () => {
    const value = { ...service(), desired_count: 3, running_count: 2, healthy_count: 1, restart_count: 4 };
    const { container } = render(ServiceRow, { props: { service: value } });
    const states = [...container.querySelectorAll(".mm-pip")].map((pip) => pip.className);
    expect(states).toEqual(["mm-pip mm-pip--ok", "mm-pip mm-pip--warn", "mm-pip mm-pip--off"]);
    // The pips are decorative; the count beside them is what assistive technology reads.
    expect(container.querySelector(".mm-pips")?.getAttribute("aria-hidden")).toBe("true");
    expect(container.querySelector(".service__count")?.textContent).toBe("2/3 running");
    expect(container.querySelector(".service__restarts")?.textContent).toBe("4 restarts");
  });

  it("counts a single restart in the singular", () => {
    const { container } = render(ServiceRow, { props: { service: { ...service(), restart_count: 1 } } });
    expect(container.querySelector(".service__restarts")?.textContent).toBe("1 restart");
  });

  it("shows unparseable route URLs verbatim and flags routes without TLS", () => {
    const value = { ...service(), desired_count: 0, routes: [{ url: "not a url", secure: false, provenance: "explicit_config" as const }] };
    const { container } = render(ServiceRow, { props: { service: value } });
    expect(container.querySelector(".mm-pips")).toBeNull();
    expect(screen.getByRole("link", { name: "not a url" })).toBeTruthy();
    expect(screen.getByText("configured")).toBeTruthy();
    expect(screen.getByText("no TLS")).toBeTruthy();
  });

  it("names routes that mm manages itself", () => {
    const value = { ...service(), routes: [{ url: "https://mail.example.test", secure: true, provenance: "mm_managed_service" as const }] };
    render(ServiceRow, { props: { service: value } });
    expect(screen.getByText("mm managed")).toBeTruthy();
  });

  it("lists only the runtime facts a service reports", () => {
    const bare = { ...service(), image: null, revision: null, exit_code: null, diagnostic_code: null };
    const first = render(ServiceRow, { props: { service: bare } });
    expect(screen.getByText("stack").nextElementSibling?.textContent).toBe("app");
    expect(first.container.querySelector(".service__facts--problem")).toBeNull();
    expect(screen.queryByText("Image")).toBeNull();
    expect(screen.queryByText("Revision")).toBeNull();
    first.unmount();

    const second = render(ServiceRow, { props: { service: { ...bare, diagnostic_code: "runtime.unhealthy" } } });
    expect(screen.getByText("runtime.unhealthy")).toBeTruthy();
    expect(screen.queryByText("exit")).toBeNull();
    expect(screen.queryByText("Image")).toBeNull();
    second.unmount();

    render(ServiceRow, { props: { service: { ...service(), exit_code: 0 } } });
    expect(screen.getByText("exit").nextElementSibling?.textContent).toBe("0");
    expect(screen.queryByText("Diagnostic")).toBeNull();
    expect(screen.getByText("Image").nextElementSibling?.textContent).toBe("example-web");
    expect(screen.getByText("Revision").nextElementSibling?.textContent).toBe("abcdef123456");
  });
});

describe("service health summary", () => {
  it.each([
    [0, 0, "idle"],
    [2, 2, "ok"],
    [1, 2, "warn"],
    [0, 2, "bad"],
  ])("renders %i of %i healthy as %s", (healthy, total, tone) => {
    const value = dashboard();
    value.project!.services = Array.from({ length: total }, (_, index) => ({
      ...service(),
      name: `web-${index}`,
      health: index < healthy ? ("healthy" as const) : ("unknown" as const),
    }));
    const { container } = render(DashboardPage, { props: { dashboard: value, stale: false } });
    expect(container.querySelector(".stats .mm-stat .mm-tone")?.getAttribute("data-tone")).toBe(tone);
  });
});

describe("dashboard details", () => {
  it("tags distinct targets and runtimes and times short operations", async () => {
    const value = dashboard();
    value.project!.target = "staging";
    value.project!.runtime = null;
    value.project!.operations = [
      { kind: "deploy", state: "succeeded", requested_at_unix_ms: 2_000, finished_at_unix_ms: 44_000 },
      { kind: "build", state: "interrupted", requested_at_unix_ms: 1_000, finished_at_unix_ms: 1_500 },
    ];
    render(DashboardPage, { props: { dashboard: value, stale: false } });
    expect(within(screen.getByRole("list", { name: "Project context" })).getByText("staging")).toBeTruthy();
    expect(screen.queryByText("docker compose")).toBeNull();
    expect(screen.getByText("succeeded in 42s")).toBeTruthy();
    await userEvent.setup().click(screen.getByRole("tab", { name: /^activity/i }));
    const activity = within(screen.getByRole("tabpanel"));
    expect(activity.getByText("42s")).toBeTruthy();
    expect(activity.getByText("interrupted")).toBeTruthy();
  });

  it("marks the current project in the project navigation", () => {
    const value = dashboard();
    value.projects.push({ ...value.projects[0]!, project_id: "219c527b-6242-7b43-bfd4-bd60e27c1de4", name: "Other", slug: "other" });
    render(DashboardPage, { props: { dashboard: value, stale: false } });
    const navigation = within(screen.getByRole("navigation", { name: "Registered projects" }));
    expect(navigation.getByRole("link", { name: "Example" }).getAttribute("aria-current")).toBe("page");
    expect(navigation.getByRole("link", { name: "Other" }).getAttribute("aria-current")).toBeNull();
    expect(screen.getByRole("link", { name: "mm installation dashboard" }).getAttribute("href")).toBe("https://mm.test");
  });

  it("distinguishes stopped components and omits absent suggested actions", async () => {
    const value = dashboard();
    value.installation.components = [
      { kind: "mailpit", version: "1", running: false, healthy: false },
      { kind: "cap", version: "1", running: true, healthy: false },
      { kind: "mm_control_api", version: "1", running: true, healthy: true },
    ];
    value.project!.diagnostics = [
      { code: "runtime.missing", severity: "error", summary: "Runtime missing" },
      { code: "runtime.note", severity: "info", summary: "Runtime note" },
    ];
    const first = render(DashboardPage, { props: { dashboard: value, stale: false } });
    const components = first.container.querySelector(".control__components")!;
    expect(components.querySelector('.mm-badge[data-tone="bad"]')?.textContent).toBe("mailpit");
    expect(components.querySelector('.mm-badge[data-tone="warn"]')?.textContent).toBe("cap");
    expect(components.querySelector('.mm-badge[data-tone="ok"]')?.textContent).toBe("control api");
    await userEvent.setup().click(screen.getByRole("tab", { name: /doctor/i }));
    expect(screen.getByText("Runtime missing").closest("li")?.querySelector("p")).toBeNull();
    expect(screen.getByText("Runtime missing").closest("li")?.getAttribute("data-tone")).toBe("bad");
    expect(screen.getByText("Runtime note").closest("li")?.getAttribute("data-tone")).toBe("info");
    first.unmount();

    // Findings that are only informational are listed without raising the issue count.
    value.project!.diagnostics = [{ code: "runtime.note", severity: "info", summary: "Runtime note" }];
    render(DashboardPage, { props: { dashboard: value, stale: false } });
    expect(screen.getByText("all checks passed")).toBeTruthy();
    expect(screen.getByRole("tab", { name: "Doctor 1" }).querySelector("[data-tone]")).toBeNull();
  });
});
