import { describe, expect, it, vi } from "vitest";

// The shared WASM module didn't load (or hasn't yet)
vi.mock("./shared", () => ({ sharedOr: <T>(fallback: T) => fallback }));
const { commitNoticeTitle } = await import("./commitNotice");

describe("commitNoticeTitle without the WASM module", () => {
  it("doesn't claim the commit failed when the connection was lost and it may have gone through", () => {
    const lost = "Connection reset by peer (os error 54) — connection lost while committing, so it is unknown whether the changes were committed.";
    expect(commitNoticeTitle(lost)).toBe("Commit did not complete");
    expect(commitNoticeTitle("Session closed")).toBe("Commit did not complete");
  });
});
