import type { Tone } from "@mm/ui/types";
import type {
  DashboardComponent,
  DashboardDiagnostic,
  DashboardHealth,
  DashboardOperation,
  IngressRouteProvenance,
  OperationState,
  ServiceComponentKind,
  Severity,
} from "./api.generated";

// Labels and tones follow the terminal interface (core/src/presentation/tui/view).
export const healthTone: Record<DashboardHealth, Tone> = {
  healthy: "ok",
  degraded: "warn",
  stopped: "bad",
  unknown: "idle",
};

export const severityTone: Record<Severity, Tone> = { info: "info", warning: "warn", error: "bad" };

export const operationTone: Record<OperationState, Tone> = {
  planned: "idle",
  running: "info",
  succeeded: "ok",
  failed: "bad",
  cancelled: "idle",
  interrupted: "warn",
};

export const componentLabel: Record<ServiceComponentKind, string> = {
  local_ingress: "ingress",
  mm_control_api: "control api",
  mailpit: "mailpit",
  cap: "cap",
  cap_valkey: "cap valkey",
};

export const provenanceLabel: Record<IngressRouteProvenance, string> = {
  explicit_config: "configured",
  automatic_convention: "automatic",
  mm_managed_service: "mm managed",
};

export function componentTone(component: DashboardComponent): Tone {
  if (component.healthy) return "ok";
  return component.running ? "warn" : "bad";
}

/** Tone for "how many of these are fine": all, some, none, or nothing to count. */
export function shareTone(good: number, total: number): Tone {
  if (!total) return "idle";
  if (good === total) return "ok";
  return good === 0 ? "bad" : "warn";
}

export function issueSummary(diagnostics: readonly DashboardDiagnostic[]) {
  const errors = diagnostics.filter((item) => item.severity === "error").length;
  const warnings = diagnostics.filter((item) => item.severity === "warning").length;
  const tone: Tone = errors ? "bad" : warnings ? "warn" : "ok";
  return {
    count: errors + warnings,
    tone,
    detail: errors + warnings ? `${count(errors, "error")} · ${count(warnings, "warning")}` : "all checks passed",
  };
}

/** A count joined to its noun with a non-breaking space, so a wrapped line never splits them. */
function count(value: number, noun: string) {
  return plural(value, noun).replace(" ", "\u00a0");
}

export function friendly(value: string) {
  return value.replaceAll("_", " ").replaceAll("-", " ");
}

export function plural(count: number, noun: string) {
  return count === 1 ? `1 ${noun}` : `${count} ${noun}s`;
}

export function host(url: string) {
  try {
    return new URL(url).host;
  } catch {
    return url;
  }
}

export function formatTime(value: number) {
  return new Intl.DateTimeFormat(undefined, {
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit",
  }).format(value);
}

export function formatClock(value: number) {
  return new Intl.DateTimeFormat(undefined, { timeStyle: "medium" }).format(value);
}

export function duration(operation: DashboardOperation) {
  if (operation.finished_at_unix_ms == null) return undefined;
  const seconds = Math.max(0, Math.round((operation.finished_at_unix_ms - operation.requested_at_unix_ms) / 1000));
  return seconds < 60 ? `${seconds}s` : `${Math.floor(seconds / 60)}m ${seconds % 60}s`;
}
