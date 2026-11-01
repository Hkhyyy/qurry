import { describe, expect, it, vi } from "vitest";
import { shared } from "./shared";

describe("shared wasm", () => {
  it("answers synchronously (initialized by the vitest setup file)", () => {
    expect(shared().ping("héllo 한글")).toEqual({ text: "héllo 한글", chars: 8 });
  });

  it("titles a failed Commit's notice from the Rust error text", () => {
    const lost = "Connection reset by peer (os error 54) — connection lost while committing, so it is unknown whether the changes were committed. Check the data before you run them again (the next run reconnects).";
    expect(shared().commitFailedTitle(lost)).toBe("Commit outcome unknown");
    expect(shared().commitFailedTitle("Session closed")).toBe("Commit failed");
  });

  it("throws before init, saying how to fix it", async () => {
    vi.resetModules();
    const fresh = await import("./shared");
    expect(() => fresh.shared()).toThrow(/not ready.*pnpm wasm/);
  });
});
