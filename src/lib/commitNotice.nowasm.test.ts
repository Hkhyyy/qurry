import { describe, expect, it, vi } from "vitest";
import rust from "../../crates/shared/src/errors.rs?raw";

// The shared WASM module didn't load (or hasn't yet)
vi.mock("./shared", () => ({
  sharedOr: <T>(fallback: T) => fallback,
  shared: () => {
    throw new Error("not ready");
  },
}));
const { COMMIT_UNKNOWN_MARK, commitNoticeTitle } = await import("./commitNotice");

const lostErr = rust.match(/pub const COMMIT_UNKNOWN_ERR: &str = "([^"]+)";/)![1];

describe("commitNoticeTitle without the WASM module", () => {
  it("says the outcome is unknown when the connection was lost during the commit, as the TUI does", () => {
    expect(lostErr.startsWith(COMMIT_UNKNOWN_MARK)).toBe(true);
    expect(commitNoticeTitle(`Connection reset by peer (os error 54) — ${lostErr}`)).toBe("Commit outcome unknown");
  });

  it("doesn't claim the commit failed for other errors either", () => {
    expect(commitNoticeTitle("Session closed")).toBe("Commit did not complete");
  });
});
