import { describe, expect, it, vi } from "vitest";

const mount = vi.fn();
const createApp = vi.fn(() => ({ mount }));

vi.mock("vue", async (importOriginal) => ({
  ...(await importOriginal<typeof import("vue")>()),
  createApp,
}));

describe("dashboard entrypoint", () => {
  it("mounts the application at the owned root element", async () => {
    await import("./main");

    expect(createApp).toHaveBeenCalledOnce();
    expect(mount).toHaveBeenCalledWith("#app");
  });
});
