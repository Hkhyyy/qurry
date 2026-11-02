import { describe, expect, it, vi } from "vitest";
import rust from "../../crates/shared/src/audit.rs?raw";
import type { AuditEntry } from "./api";

// The shared WASM module didn't load (or hasn't yet): the Production log still doesn't read as done where it may not be
vi.mock("./shared", () => ({
  sharedReady: () => false,
  shared: () => {
    throw new Error("not ready");
  },
}));
const { auditStatus } = await import("../components/ProductionLog");

const word = (name: string) => rust.match(new RegExp(`pub const ${name}: &str = "([^"]+)";`))![1];
const entry = (e: Partial<AuditEntry>): AuditEntry => ({ id: "a", ts: 0, connId: "p", connName: "PROD", sql: "delete from t where id = 1", inTx: false, ...e });

describe("auditStatus without the WASM module", () => {
  it("keeps the safety words", () => {
    expect(auditStatus(entry({ started: true }))).toEqual({ text: "no result recorded", tone: "warn" });
    expect(auditStatus(entry({ started: true, sql: "COMMIT" }))).toEqual({ text: "no result recorded", tone: "warn" });
    expect(auditStatus(entry({ error: "denied" }))).toEqual({ text: "failed", tone: "danger" });
  });

  it("shows a transaction end as logged, as a warning (it may be of unknown outcome)", () => {
    const unknown = word("COMMIT_UNKNOWN");
    // Unknown-outcome entries always carry a note (what to do)
    expect(auditStatus(entry({ sql: unknown, error: "connection reset by peer", note: "check" }))).toEqual({ text: unknown, tone: "warn" });
    expect(auditStatus(entry({ sql: word("ENDED_UNKNOWN"), note: "check" })).tone).toBe("warn");
    expect(auditStatus(entry({ sql: "ROLLBACK (by the server)" }))).toEqual({ text: "ROLLBACK (by the server)", tone: "warn" });
  });

  it("shows a COMMIT the server refused as failed, not as a transaction end", () => {
    expect(auditStatus(entry({ sql: "COMMIT", error: "could not serialize access" }))).toEqual({ text: "failed", tone: "danger" });
  });

  it("shows rows as before", () => {
    expect(auditStatus(entry({ rows: 2 }))).toEqual({ text: "2 rows", tone: "neutral" });
    expect(auditStatus(entry({ rows: 1, inTx: true }))).toEqual({ text: "1 row · not committed yet", tone: "warn" });
  });
});
