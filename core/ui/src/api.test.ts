import { afterEach, describe, expect, it, vi } from "vitest";
import { ApiVersionError, isDashboardEvent, isDashboardResponse, loadDashboard } from "./api";
import { dashboard } from "./test-fixtures";

describe("dashboard API guard", () => {
  afterEach(() => vi.unstubAllGlobals());

  it("accepts the versioned contract and rejects malformed payloads", () => {
    expect(isDashboardResponse(dashboard())).toBe(true);
    expect(isDashboardResponse({ api_version: "1.0", projects: "secret" })).toBe(false);
    expect(isDashboardResponse({ ...dashboard(), secret: "mm-dashboard-secret-sentinel" })).toBe(false);
    expect(isDashboardEvent({ type: "connected", api_version: "1", sequence: 1 })).toBe(true);
    expect(isDashboardEvent({ type: "refresh", sequence: 2, occurred_at_unix_ms: 1 })).toBe(true);
    expect(isDashboardEvent({ type: "refresh", sequence: 1, occurred_at_unix_ms: 1, token: "secret" })).toBe(false);
  });

  it("fails closed when embedded assets and API versions differ", async () => {
    vi.stubGlobal("fetch", vi.fn(async () => new Response(JSON.stringify({ ...dashboard(), api_version: "2.0" }), { status: 200 })));
    await expect(loadDashboard()).rejects.toBeInstanceOf(ApiVersionError);
  });

  it("rejects unsuccessful and malformed responses before exposing data", async () => {
    const fetch = vi
      .fn<typeof globalThis.fetch>()
      .mockResolvedValueOnce(new Response("no", { status: 401 }))
      .mockResolvedValueOnce(new Response(JSON.stringify({ ...dashboard(), projects: "invalid" }), { status: 200 }));
    vi.stubGlobal("fetch", fetch);

    await expect(loadDashboard()).rejects.toThrow("Dashboard request failed (401)");
    await expect(loadDashboard()).rejects.toThrow("Dashboard response was malformed");
    expect(fetch).toHaveBeenNthCalledWith(
      1,
      "/api/v1/dashboard",
      expect.objectContaining({ credentials: "same-origin", headers: { Accept: "application/json" } }),
    );
  });
});
