import { fireEvent, render, screen, waitFor } from "@testing-library/vue";
import { afterEach, describe, expect, it, vi } from "vitest";
import App from "./App.vue";
import { dashboard } from "./test-fixtures";

// ESLint's base rule treats a TypeScript function-type parameter as a runtime variable.
// eslint-disable-next-line no-unused-vars
type MessageHandler = (event: MessageEvent<string>) => void;

class FakeEventSource {
  static instance: FakeEventSource | undefined;
  readonly url: string;
  readonly options?: { withCredentials?: boolean };
  onerror: (() => unknown) | null = null;
  private readonly listeners = new Map<string, MessageHandler>();
  closed = false;

  constructor(url: string, options?: { withCredentials?: boolean }) {
    this.url = url;
    this.options = options;
    FakeEventSource.instance = this;
  }

  addEventListener(type: string, listener: MessageHandler) {
    this.listeners.set(type, listener);
  }

  emit(type: string, payload: unknown) {
    this.listeners.get(type)?.(new MessageEvent(type, { data: JSON.stringify(payload) }));
  }

  emitRaw(type: string, data: string) {
    this.listeners.get(type)?.(new MessageEvent(type, { data }));
  }

  disconnect() {
    this.onerror?.();
  }

  close() {
    this.closed = true;
  }
}

describe("dashboard event stream", () => {
  afterEach(() => {
    FakeEventSource.instance = undefined;
    vi.unstubAllGlobals();
  });

  it("rides out brief reconnects and refetches what it missed", async () => {
    vi.useFakeTimers();
    const fetch = vi.fn(async () =>
      new Response(JSON.stringify(dashboard()), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      }),
    );
    vi.stubGlobal("fetch", fetch);
    vi.stubGlobal("EventSource", FakeEventSource);
    const view = render(App);
    await vi.runAllTimersAsync();

    expect(screen.getByRole("heading", { name: "web frontend" })).toBeTruthy();
    const events = FakeEventSource.instance;
    expect(events?.url).toBe("/api/v1/events");
    expect(events?.options?.withCredentials).toBe(true);

    events?.disconnect();
    await vi.advanceTimersByTimeAsync(4_000);
    events?.disconnect();
    expect(screen.getByRole("status").textContent).toBe("live");
    events?.emit("connected", { type: "connected", api_version: "1", sequence: 2 });
    await vi.runAllTimersAsync();
    expect(fetch).toHaveBeenCalledTimes(2);
    expect(screen.getByRole("status").textContent).toBe("live");

    events?.emit("connected", { type: "connected", api_version: "1", sequence: 3 });
    expect(fetch).toHaveBeenCalledTimes(2);

    view.unmount();
    vi.useRealTimers();
  });

  it("shows a lasting disconnect until a valid refresh reconnects it", async () => {
    vi.useFakeTimers();
    const fetch = vi.fn(async () => new Response(JSON.stringify(dashboard()), { status: 200 }));
    vi.stubGlobal("fetch", fetch);
    vi.stubGlobal("EventSource", FakeEventSource);
    const view = render(App);
    await vi.runAllTimersAsync();

    const events = FakeEventSource.instance;
    events?.disconnect();
    await vi.advanceTimersByTimeAsync(5_000);
    expect(screen.getByRole("status").textContent).toBe("reconnecting");
    events?.disconnect();
    await vi.advanceTimersByTimeAsync(5_000);
    expect(screen.getByRole("status").textContent).toBe("reconnecting");

    events?.emit("refresh", { type: "refresh", sequence: 2, token: "secret" });
    expect(fetch).toHaveBeenCalledTimes(1);
    expect(screen.getByRole("status").textContent).toBe("reconnecting");

    events?.emit("refresh", {
      type: "refresh",
      sequence: 3,
      occurred_at_unix_ms: 1_700_000_000_000,
    });
    await vi.runAllTimersAsync();
    expect(fetch).toHaveBeenCalledTimes(2);
    expect(screen.getByRole("status").textContent).toBe("live");

    view.unmount();
    vi.useRealTimers();
  });

  it("aborts an in-flight request when the app unmounts", async () => {
    let signal: AbortSignal | undefined;
    vi.stubGlobal(
      "fetch",
      vi.fn((...args: Parameters<typeof fetch>) => {
        const init = args[1];
        signal = init?.signal ?? undefined;
        return new Promise<Response>(() => {});
      }),
    );
    vi.stubGlobal("EventSource", FakeEventSource);
    const view = render(App);
    await waitFor(() => expect(signal).toBeDefined());

    const events = FakeEventSource.instance;
    view.unmount();

    expect(signal?.aborted).toBe(true);
    expect(events?.closed).toBe(true);
  });

  it("fails visibly, retries, and preserves a safe snapshot after refresh errors", async () => {
    const fetch = vi
      .fn<typeof globalThis.fetch>()
      .mockResolvedValueOnce(new Response("unavailable", { status: 503 }))
      .mockResolvedValueOnce(new Response(JSON.stringify(dashboard()), { status: 200 }))
      .mockRejectedValueOnce(new Error("refresh failed"));
    vi.stubGlobal("fetch", fetch);
    vi.stubGlobal("EventSource", FakeEventSource);
    render(App);

    expect(await screen.findByText("Dashboard request failed (503)")).toBeTruthy();
    await fireEvent.click(screen.getByRole("button", { name: "Try again" }));
    expect(await screen.findByRole("heading", { name: "web frontend" })).toBeTruthy();

    FakeEventSource.instance?.emit("refresh", {
      type: "refresh",
      sequence: 2,
      occurred_at_unix_ms: 1_700_000_000_000,
    });
    expect(await screen.findByText("refresh failed")).toBeTruthy();
    expect(screen.getByRole("heading", { name: "web frontend" })).toBeTruthy();
  });

  it("rejects malformed, mismatched, and non-error stream and request data", async () => {
    const fetch = vi
      .fn<typeof globalThis.fetch>()
      .mockRejectedValueOnce("offline")
      .mockResolvedValueOnce(new Response(JSON.stringify(dashboard()), { status: 200 }));
    vi.stubGlobal("fetch", fetch);
    vi.stubGlobal("EventSource", FakeEventSource);
    render(App);

    expect(await screen.findByText("The local dashboard is unavailable.")).toBeTruthy();
    await fireEvent.click(screen.getByRole("button", { name: "Try again" }));
    expect(await screen.findByRole("heading", { name: "web frontend" })).toBeTruthy();

    const events = FakeEventSource.instance;
    events?.emitRaw("connected", "not-json");
    expect(await screen.findByText("reconnecting")).toBeTruthy();
    events?.emit("connected", { type: "refresh", sequence: 1, occurred_at_unix_ms: 1 });
    expect(screen.getByText("reconnecting")).toBeTruthy();
    events?.emit("connected", { type: "connected", api_version: "1", sequence: 2 });
    await waitFor(() => expect(screen.queryByText("reconnecting")).toBeNull());
  });

  it("aborts the previous refresh when another arrives", async () => {
    vi.useFakeTimers();
    const signals: AbortSignal[] = [];
    vi.stubGlobal(
      "fetch",
      vi.fn((...args: Parameters<typeof fetch>) => {
        const signal = args[1]?.signal;
        if (signal) signals.push(signal);
        return signals.length === 1
          ? Promise.resolve(new Response(JSON.stringify(dashboard()), { status: 200 }))
          : new Promise<Response>((_resolve, reject) => {
              signal?.addEventListener("abort", () => reject(new DOMException("aborted", "AbortError")));
            });
      }),
    );
    vi.stubGlobal("EventSource", FakeEventSource);
    const view = render(App);
    await vi.runAllTimersAsync();
    expect(screen.getByRole("heading", { name: "web frontend" })).toBeTruthy();

    FakeEventSource.instance?.emit("connected", { type: "connected", api_version: "1", sequence: 1 });
    FakeEventSource.instance?.emit("refresh", { type: "refresh", sequence: 2, occurred_at_unix_ms: 1 });
    expect(signals).toHaveLength(2);
    FakeEventSource.instance?.emit("refresh", { type: "refresh", sequence: 3, occurred_at_unix_ms: 2 });
    expect(signals[1]?.aborted).toBe(true);
    await vi.advanceTimersByTimeAsync(60_000);
    expect(screen.getByRole("status").textContent).toBe("live");

    view.unmount();
    vi.useRealTimers();
  });
});
