import { useEffect, useMemo, useState } from "react";
import { AuditEntry, auditLog } from "../lib/api";
import { shared } from "../lib/shared";
import { Badge, Button, CodeBlock, List, ListItem, Modal, Notice, SearchField, SectionTitle, Text } from "../ui";

type Tone = "neutral" | "warn" | "danger" | "accent";
/** Tone of each result word qurry-shared's audit::status gives */
const TONES: Record<string, Tone> = { "no result recorded": "warn", "outcome unknown": "warn", failed: "danger", committed: "accent", "rolled back": "neutral" };

/** Result word decided in Rust like the TUI's (before the WASM has loaded: none, the rows say it) */
const statusWord = (e: AuditEntry) => {
  try {
    return shared().auditStatus(!!e.started, e.sql, !!e.error);
  } catch {
    return undefined;
  }
};

/** One-line summary: changed rows · error · uncommitted · no result recorded */
export function auditStatus(e: AuditEntry): { text: string; tone: Tone } {
  const word = statusWord(e);
  if (word) return { text: word, tone: TONES[word] ?? "warn" };
  const rows = e.rows == null ? "done" : `${e.rows.toLocaleString("en-US")} row${e.rows === 1 ? "" : "s"}`;
  return { text: e.inTx ? `${rows} · not committed yet` : rows, tone: e.inTx ? "warn" : "neutral" };
}

/**
 * Production log: modifying statements run on Production Data Sources (same file as the TUI).
 * Simple UPDATE / DELETE leave undo SQL built from rows captured right before running — open it in a console and run it (never run automatically)
 */
export function ProductionLog({ connId, onOpenUndo, onClose }: { connId: string | null; onOpenUndo: (e: AuditEntry) => void; onClose: () => void }) {
  const [items, setItems] = useState<AuditEntry[]>([]);
  const [q, setQ] = useState("");
  const [idx, setIdx] = useState(0);
  const [err, setErr] = useState<string | null>(null);
  useEffect(() => {
    auditLog(connId, "", 2000).then(setItems, (e) => setErr(String(e)));
  }, [connId]);
  const list = useMemo(() => {
    const f = q.toLowerCase();
    return items.filter((e) => !f || e.sql.toLowerCase().includes(f) || e.connName.toLowerCase().includes(f));
  }, [items, q]);
  const cur = list[Math.min(idx, list.length - 1)];
  // The changes may or may not be saved (a COMMIT cut off by a lost connection, a transaction a statement ended either way): the note says what to do
  const unknown = cur && statusWord(cur) === "outcome unknown";
  const where = (e: AuditEntry) => (e.database ? `${e.connName} · ${e.database}` : e.connName);

  return (
    <Modal
      title="Production Log"
      size="xl"
      className="history"
      fixed
      onClose={onClose}
      onKeyDown={(e) => {
        if (e.key === "ArrowDown") setIdx((i) => Math.min(i + 1, list.length - 1));
        else if (e.key === "ArrowUp") setIdx((i) => Math.max(i - 1, 0));
        else if (e.key === "Enter" && cur?.undo && !(e.target as HTMLElement).closest("button")) onOpenUndo(cur);
        else return;
        e.preventDefault();
      }}
      footer={
        <>
          <span className="ui-modal-foot-hint">Changes run on Production data sources · undo is kept for simple UPDATE / DELETE (up to 1,000 rows)</span>
          <Button onClick={onClose}>Close</Button>
          <Button variant="primary" disabled={!cur?.undo} onClick={() => cur && onOpenUndo(cur)}>
            Open Undo in Console
          </Button>
        </>
      }
    >
      <SearchField autoFocus className="hist-filter" placeholder="Type to filter" value={q} onChange={(e) => (setQ(e.target.value), setIdx(0))} />
      {err && <Notice tone="danger">{err}</Notice>}
      <div className="hist-body">
        <List boxed className="hist-list" empty="Nothing yet. Changes you run on a Production data source show up here.">
          {list.map((e, i) => {
            const s = auditStatus(e);
            return (
              <ListItem
                key={e.id}
                mono
                title={e.sql.replace(/\s+/g, " ").slice(0, 120)}
                subtitle={`${new Date(e.ts).toLocaleString()} · ${where(e)}`}
                trailing={
                  <>
                    {e.undo && <Badge tone="accent">undo</Badge>} <Badge tone={s.tone}>{s.text}</Badge>
                  </>
                }
                selected={e === cur}
                onClick={() => setIdx(i)}
                onDoubleClick={() => e.undo && onOpenUndo(e)}
              />
            );
          })}
        </List>
        <div className="hist-preview audit-detail">
          {cur && (
            <>
              <Text tone="muted" size="sm">
                {new Date(cur.ts).toLocaleString()} · {where(cur)} · {auditStatus(cur).text}
              </Text>
              <CodeBlock>{cur.sql}</CodeBlock>
              {unknown ? (
                // It may have committed: what to do first, then the driver error
                <Notice tone="warn">
                  {cur.note}
                  {cur.error && ` (${cur.error})`}
                </Notice>
              ) : (
                cur.error && <Notice tone="danger">{cur.error}</Notice>
              )}
              <SectionTitle>Undo</SectionTitle>
              {cur.undo ? <CodeBlock className="audit-undo">{cur.undo}</CodeBlock> : <Text tone="muted">{(!unknown && cur.note) || "No undo for this statement."}</Text>}
            </>
          )}
        </div>
      </div>
    </Modal>
  );
}
