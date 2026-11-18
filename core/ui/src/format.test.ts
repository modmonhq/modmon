import { describe, expect, it } from "vitest";
import { componentTone, duration, formatClock, formatTime, friendly, host, issueSummary, plural, shareTone } from "./format";

describe("dashboard formatting", () => {
  it("maps shared service state to the terminal's tones", () => {
    expect(componentTone({ kind: "cap", version: "1", running: true, healthy: true })).toBe("ok");
    expect(componentTone({ kind: "cap", version: "1", running: true, healthy: false })).toBe("warn");
    expect(componentTone({ kind: "cap", version: "1", running: false, healthy: false })).toBe("bad");
  });

  it("grades a share as all, some, none, or nothing to count", () => {
    expect(shareTone(0, 0)).toBe("idle");
    expect(shareTone(3, 3)).toBe("ok");
    expect(shareTone(1, 3)).toBe("warn");
    expect(shareTone(0, 3)).toBe("bad");
  });

  it("counts errors and warnings but not informational findings", () => {
    const finding = (severity: "info" | "warning" | "error") => ({ code: "x", severity, summary: "x" });
    expect(issueSummary([])).toEqual({ count: 0, tone: "ok", detail: "all checks passed" });
    expect(issueSummary([finding("info")])).toEqual({ count: 0, tone: "ok", detail: "all checks passed" });
    expect(issueSummary([finding("warning"), finding("warning")])).toEqual({ count: 2, tone: "warn", detail: "0\u00a0errors · 2\u00a0warnings" });
    expect(issueSummary([finding("warning"), finding("error")])).toEqual({ count: 2, tone: "bad", detail: "1\u00a0error · 1\u00a0warning" });
  });

  it("formats names, counts, hosts and times", () => {
    expect(friendly("docker-compose")).toBe("docker compose");
    expect(friendly("cap_valkey")).toBe("cap valkey");
    expect(plural(1, "service")).toBe("1 service");
    expect(plural(0, "service")).toBe("0 services");
    expect(host("https://web.example.test:8443/path")).toBe("web.example.test:8443");
    expect(host("not a url")).toBe("not a url");
    expect(formatTime(1_700_000_000_000)).toMatch(/1[45]\D+\d{1,2}[:.]\d{2}/);
    expect(formatClock(1_700_000_000_000)).toMatch(/\d{1,2}[:.]\d{2}[:.]\d{2}/);
  });

  it("times finished operations and leaves running ones untimed", () => {
    const operation = { kind: "deploy" as const, state: "succeeded" as const, requested_at_unix_ms: 10_000 };
    expect(duration(operation)).toBeUndefined();
    expect(duration({ ...operation, finished_at_unix_ms: 52_000 })).toBe("42s");
    expect(duration({ ...operation, finished_at_unix_ms: 93_000 })).toBe("1m 23s");
    // A clock that moved backwards never yields a negative duration.
    expect(duration({ ...operation, finished_at_unix_ms: 9_000 })).toBe("0s");
  });
});
