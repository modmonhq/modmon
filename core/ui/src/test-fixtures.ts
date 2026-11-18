import type { DashboardResponse } from "./api.generated";

export function dashboard(health: "healthy" | "degraded" = "healthy"): DashboardResponse {
  return {
    api_version: "1",
    generated_at_unix_ms: 1_700_000_000_000,
    scope: { kind: "project", slug: "example" },
    installation: { installed: true, stack_version: "1.0.0", runtime: "docker-compose", route: "https://mm.test", components: [{ kind: "local_ingress", version: "3", running: true, healthy: true }], diagnostics: [] },
    projects: [{ project_id: "219c527b-6242-7b43-bfd4-bd60e27c1de3", name: "Example", slug: "example", dashboard_url: "https://mm.example.test", environment: "local" }],
    project: {
      project_id: "219c527b-6242-7b43-bfd4-bd60e27c1de3", name: "Example", slug: "example", environment: "local", target: "local", runtime: "docker-compose", runtime_available: true, config_revision: "abc123", environment_delivery: { mode: "direct", provenance: "development-default" },
      services: [{ name: "web", app: "frontend", stack: "app", state: health === "healthy" ? "running" : "degraded", health, desired_count: 1, running_count: 1, healthy_count: health === "healthy" ? 1 : 0, restart_count: 0, image: "example-web", revision: "abcdef123456", routes: [{ url: "https://web.example.test", secure: true, provenance: "automatic_convention" }] }],
      resources: [{ kind: "network", logical_name: "app" }], operations: [], jobs: [], schedules: [], job_runs: [], diagnostics: [],
    },
  };
}
