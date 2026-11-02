import { describe, expect, it } from "vitest";
import { COMMIT_UNKNOWN, ENDED_IN_PROCEDURE, auditStatus } from "../components/ProductionLog";
import type { AuditEntry } from "./api";

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
    expect(auditStatus(entry({ sql: ENDED_IN_PROCEDURE }))).toEqual({ text: "outcome unknown", tone: "warn" });
  });

  it("shows results as before", () => {
    expect(auditStatus(entry({ sql: "COMMIT" }))).toEqual({ text: "committed", tone: "accent" });
    expect(auditStatus(entry({ rows: 2, inTx: true }))).toEqual({ text: "2 rows · not committed yet", tone: "warn" });
    expect(auditStatus(entry({ error: "denied" })).tone).toBe("danger");
  });
});
