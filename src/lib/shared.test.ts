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

  it("words a failed load-more as the TUI does", () => {
    expect(shared().moreRowsFailed("Session closed")).toBe("Couldn't load more rows: Session closed");
  });

  it("hints a local Host with AWS SSM (the instance itself) in the TUI's words", () => {
    expect(shared().ssmLocalHost(" localhost ")).toContain("With AWS SSM, localhost is the instance itself: Host and Port are the database as seen from the instance");
    expect(shared().ssmLocalHost("::1")).toContain("::1 is the instance itself");
    expect(shared().ssmLocalHost("mydb.abc.ap-northeast-2.rds.amazonaws.com")).toBeUndefined();
  });

  it("a freed completion index finds nothing instead of throwing (a request may still hold it)", () => {
    const idx = shared().completionIndex([{ schema: "public", name: "users", columns: [["id", "int8"]] }]);
    const text = "select * from users where status = ";
    expect(idx.suggest("select * from u", 15, "", false).items.length).toBeGreaterThan(0);
    idx.free();
    expect(idx.suggest("select * from u", 15, "", false).items).toEqual([]);
    expect(idx.hintAt(text, text.length, "", false)).toBeUndefined();
    idx.free();
  });

  it("throws before init, saying how to fix it", async () => {
    vi.resetModules();
    const fresh = await import("./shared");
    expect(() => fresh.shared()).toThrow(/not ready.*pnpm wasm/);
  });
});
