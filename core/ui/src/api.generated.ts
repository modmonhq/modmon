/* Generated from dashboard.schema.json by ./scripts/generate-artifacts.sh. Do not edit. */

export type DashboardApiVersion = "1";
export type ServiceComponentKind = "local_ingress" | "mm_control_api" | "mailpit" | "cap" | "cap_valkey";
export type Severity = "info" | "warning" | "error";
export type RuntimeKind = "podman-compose" | "docker-compose";
export type EnvironmentDeliveryMode = "direct" | "env-file";
export type EnvironmentDeliveryProvenance =
  "environment-override" | "development-default" | "policy-default" | "built-in-default";
export type JobRunState =
  | "queued"
  | "launching"
  | "running"
  | "retry_wait"
  | "cancelling"
  | "succeeded"
  | "failed"
  | "timed_out"
  | "cancelled"
  | "unknown";
export type OperationKind = "build" | "bootstrap" | "deploy" | "down" | "reconcile" | "rollback" | "job";
export type OperationState = "planned" | "running" | "succeeded" | "failed" | "cancelled" | "interrupted";
export type UnitKind = "module" | "service" | "job";
export type DashboardResourceKind = "network" | "volume";
export type DashboardHealth = "healthy" | "degraded" | "stopped" | "unknown";
export type IngressRouteProvenance = "explicit_config" | "automatic_convention" | "mm_managed_service";
export type ServiceState = "missing" | "running" | "degraded" | "stopped" | "unknown" | "job_only";
export type DashboardScopeDto =
  | {
      kind: "installation";
    }
  | {
      kind: "project";
      slug: string;
    };
export type DashboardEvent =
  | {
      api_version: DashboardApiVersion;
      sequence: number;
      type: "connected";
    }
  | {
      occurred_at_unix_ms: number;
      sequence: number;
      type: "refresh";
    };

export interface DashboardContract {
  dashboard: DashboardResponse;
  event: DashboardEvent;
}
export interface DashboardResponse {
  api_version: DashboardApiVersion;
  generated_at_unix_ms: number;
  installation: InstallationDashboard;
  project?: ProjectDashboard | null;
  projects: DashboardProjectSummary[];
  scope: DashboardScopeDto;
  /**
   * Registered projects whose folder or configuration file is gone;
   * `mm workspace forget --stale` forgets them.
   */
  stale_workspaces?: DashboardStaleWorkspace[];
}
export interface InstallationDashboard {
  components: DashboardComponent[];
  diagnostics: DashboardDiagnostic[];
  installed: boolean;
  route: string;
  runtime?: RuntimeKind | null;
  stack_version?: string | null;
}
export interface DashboardComponent {
  healthy: boolean;
  kind: ServiceComponentKind;
  running: boolean;
  version: string;
}
export interface DashboardDiagnostic {
  code: string;
  severity: Severity;
  suggested_action?: string | null;
  summary: string;
}
export interface ProjectDashboard {
  config_revision: string;
  diagnostics: DashboardDiagnostic[];
  environment: string;
  environment_delivery: ResolvedEnvironmentDelivery;
  job_runs: DashboardJobRun[];
  jobs: DashboardJob[];
  name: string;
  operations: DashboardOperation[];
  /**
   * The `development: optional` units; the dashboard greys out those
   * that are off on this machine.
   */
  optional?: OptionalUnit[];
  project_id: string;
  resources: DashboardResource[];
  runtime?: RuntimeKind | null;
  runtime_available: boolean;
  schedules: DashboardSchedule[];
  services: DashboardService[];
  slug: string;
  target: string;
}
export interface ResolvedEnvironmentDelivery {
  mode: EnvironmentDeliveryMode;
  provenance: EnvironmentDeliveryProvenance;
}
export interface DashboardJobRun {
  attempt: number;
  error_code?: string | null;
  exit_code?: number | null;
  finished_at_unix_ms?: number | null;
  id: string;
  job: string;
  output_truncated: boolean;
  requested_at_unix_ms: number;
  schedule?: string | null;
  state: JobRunState;
}
export interface DashboardJob {
  maximum_attempts: number;
  module: string;
  name: string;
  service: string;
  timeout: string;
}
export interface DashboardOperation {
  error_code?: string | null;
  finished_at_unix_ms?: number | null;
  kind: OperationKind;
  requested_at_unix_ms: number;
  state: OperationState;
}
/**
 * A module, service or job that is `development: optional`, and whether it
 * runs on this machine.
 */
export interface OptionalUnit {
  /**
   * Whether `.config/mm.overrides.yaml` turns it on.
   */
  enabled: boolean;
  jobs?: string[];
  kind: UnitKind;
  /**
   * The name `mm dev enable` and `mm dev disable` take.
   */
  name: string;
  /**
   * Whether it runs in the development environment on this machine.
   */
  on: boolean;
  /**
   * Its services as `module/service`.
   */
  services?: string[];
  /**
   * Whether `mm dev up --with` turns it on for this run only.
   */
  this_run?: boolean;
  [k: string]: unknown;
}
export interface DashboardResource {
  kind: DashboardResourceKind;
  logical_name: string;
}
export interface DashboardSchedule {
  enabled: boolean;
  job: string;
  name: string;
  next_due_at_unix_ms?: number | null;
  retired: boolean;
}
export interface DashboardService {
  app?: string | null;
  desired_count: number;
  diagnostic_code?: string | null;
  exit_code?: number | null;
  health: DashboardHealth;
  healthy_count: number;
  image?: string | null;
  name: string;
  restart_count?: number | null;
  revision?: string | null;
  routes: DashboardRoute[];
  running_count: number;
  stack: string;
  state: ServiceState;
}
export interface DashboardRoute {
  provenance: IngressRouteProvenance;
  secure: boolean;
  url: string;
}
export interface DashboardProjectSummary {
  dashboard_url: string;
  environment: string;
  name: string;
  project_id: string;
  slug: string;
}
export interface DashboardStaleWorkspace {
  last_used_at_unix_ms: number;
  project_id: string;
}
