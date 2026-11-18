import type { DashboardResponse } from "./api.generated";
import type { DashboardEvent } from "./api.generated";
import { validateDashboard, validateEvent } from "./api.validator.generated.js";

export const DASHBOARD_API_VERSION = "1";

export class ApiVersionError extends Error {}

export async function loadDashboard(signal?: AbortSignal): Promise<DashboardResponse> {
  const response = await fetch("/api/v1/dashboard", {
    credentials: "same-origin",
    headers: { Accept: "application/json" },
    signal,
  });
  if (!response.ok) throw new Error(`Dashboard request failed (${response.status})`);
  const payload: unknown = await response.json();
  if (
    typeof payload === "object" &&
    payload !== null &&
    "api_version" in payload &&
    payload.api_version !== DASHBOARD_API_VERSION
  ) {
    throw new ApiVersionError("Dashboard assets and local API versions differ. Run `mm services repair`.");
  }
  if (!isDashboardResponse(payload)) throw new Error("Dashboard response was malformed");
  return payload;
}

export function isDashboardResponse(value: unknown): value is DashboardResponse {
  return validateDashboard(value);
}

export function isDashboardEvent(value: unknown): value is DashboardEvent {
  return validateEvent(value);
}
