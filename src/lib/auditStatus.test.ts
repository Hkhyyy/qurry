import { describe, expect, it } from "vitest";
import { auditStatus } from "../components/ProductionLog";
import rust from "../../crates/shared/src/audit.rs?raw";
import type { AuditEntry } from "./api";

// The log words as core writes them (crates/shared/src/audit.rs), not a copy
const word = (name: string) => rust.match(new RegExp(`pub const ${name}: &str = "([^"]+)";`))![1];
const COMMIT_UNKNOWN = word("COMMIT_UNKNOWN");
const ENDED_UNKNOWN = word("ENDED_UNKNOWN");

const entry = (e: Partial<AuditEntry>): AuditEntry => ({ id: "a", ts: 0, connId: "p", connName: "PROD", sql: "delete from t where id = 1", inTx: false, ...e });

describe("auditStatus", () => {
  it("shows an entry with no result recorded as unknown, even a COMMIT", () => {
    expect(auditStatus(entry({ started: true }))).toEqual({ text: "no result recorded", tone: "warn" });
    expect(auditStatus(entry({ started: true, sql: "COMMIT" }))).toEqual({ text: "no result recorded", tone: "warn" });
  });

  it("shows a COMMIT cut off by a lost connection as of unknown outcome, not as failed", () => {
    expect(auditStatus(entry({ sql: COMMIT_UNKNOWN, error: "connection reset by peer" }))).toEqual({ text: "outcome unknown", tone: "warn" });
    expect(auditStatus(entry({ sql: COMMIT_UNKNOWN }))).toEqual({ text: "outcome unknown", tone: "warn" });
  });

  it("shows a transaction a CALL ended as of unknown outcome, not as done", () => {
    expect(auditStatus(entry({ sql: ENDED_UNKNOWN }))).toEqual({ text: "outcome unknown", tone: "warn" });
  });

  it("shows how a transaction ended, also when the server or a statement ended it", () => {
    expect(auditStatus(entry({ sql: "COMMIT (implicit)" }))).toEqual({ text: "committed", tone: "accent" });
    expect(auditStatus(entry({ sql: "ROLLBACK (by the server)" }))).toEqual({ text: "rolled back", tone: "neutral" });
  });

  it("shows results as before", () => {
    expect(auditStatus(entry({ sql: "COMMIT" }))).toEqual({ text: "committed", tone: "accent" });
    expect(auditStatus(entry({ rows: 2, inTx: true }))).toEqual({ text: "2 rows · not committed yet", tone: "warn" });
    expect(auditStatus(entry({ error: "denied" })).tone).toBe("danger");
  });
});
