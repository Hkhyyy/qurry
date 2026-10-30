import { useEffect, useMemo, useState } from "react";
import { AuditEntry, auditLog } from "../lib/api";
import { Badge, Button, CodeBlock, List, ListItem, Modal, Notice, SearchField, SectionTitle, Text } from "../ui";

/** One-line summary: changed rows · error · uncommitted */
export function auditStatus(e: AuditEntry): { text: string; tone: "neutral" | "warn" | "danger" | "accent" } {
  if (e.error) return { text: "failed", tone: "danger" };
  if (e.sql === "COMMIT") return { text: "committed", tone: "accent" };
  if (e.sql.startsWith("ROLLBACK")) return { text: "rolled back", tone: "neutral" };
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
              {cur.error && <Notice tone="danger">{cur.error}</Notice>}
              <SectionTitle>Undo</SectionTitle>
              {cur.undo ? <CodeBlock className="audit-undo">{cur.undo}</CodeBlock> : <Text tone="muted">{cur.note ?? "No undo for this statement."}</Text>}
            </>
          )}
        </div>
      </div>
    </Modal>
  );
}
