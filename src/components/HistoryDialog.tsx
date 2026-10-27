import { useEffect, useMemo, useState } from "react";
import { history, HistoryItem, Snippet } from "../lib/api";

/**
 * DataGrip "Browse Query History" (⌥⌘e) + 스니펫 (이름 붙여 저장한 쿼리). 기록은 설정 폴더에 있어 TUI 와 같이 쓴다.
 * 타이핑 = 필터, 더블클릭/↩ = 콘솔에 붙여넣기, ⌦ = 삭제
 */
const OLD_KEY = "dbtool.queryHistory";

/** 예전 기록(localStorage)은 한 번 옮기고 지운다 (앱이 뜬 뒤에 부른다: 백엔드가 준비된 다음) */
export function migrateOldHistory() {
  try {
    const raw = localStorage.getItem(OLD_KEY);
    if (!raw) return;
    const items: HistoryItem[] = JSON.parse(raw);
    history.import(items).then(() => localStorage.removeItem(OLD_KEY), () => {});
  } catch {
    /* 옮기지 못해도 앱은 돈다 */
  }
}

/** 실행한 문장을 기록한다 (비밀번호는 백엔드가 가린다) */
export function pushHistory(h: { connId: string; database?: string; sql: string }) {
  history.add(h.connId, h.database, h.sql).catch(() => {});
}

/** 지금 콘솔의 쿼리를 스니펫으로 (이름을 묻는다). 이 Data Source 에서만 쓸지도 */
export async function saveSnippet(sql: string, connId: string, connName: string): Promise<string | null> {
  const text = sql.trim();
  if (!text) return null;
  const name = prompt("Snippet name", text.replace(/\s+/g, " ").slice(0, 40));
  if (!name) return null;
  const onlyHere = confirm(`Show this snippet only for "${connName}"?\n\nOK = only this data source · Cancel = everywhere`);
  await history.saveSnippet({ id: "", name, sql: text, connId: onlyHere ? connId : null });
  return name;
}

type Row = { id: string; title: string; sql: string; sub: string };

export function HistoryDialog({ connId, title, initialTab = "history", onPick, onClose }: { connId: string; title: string; initialTab?: "history" | "snippets"; onPick: (sql: string) => void; onClose: () => void }) {
  const [tab, setTab] = useState<"history" | "snippets">(initialTab);
  const [items, setItems] = useState<HistoryItem[]>([]);
  const [snips, setSnips] = useState<Snippet[]>([]);
  const [q, setQ] = useState("");
  const [idx, setIdx] = useState(0);
  const reload = () => {
    history.list(connId, "", 2000).then(setItems, () => {});
    history.snippets(connId).then(setSnips, () => {});
  };
  useEffect(reload, [connId]);

  const fmt = (ts: number) => new Date(ts).toLocaleString();
  const list: Row[] = useMemo(() => {
    const f = q.toLowerCase();
    if (tab === "history") return items.filter((h) => !f || h.sql.toLowerCase().includes(f)).map((h) => ({ id: h.id, title: h.sql.replace(/\s+/g, " ").slice(0, 120), sql: h.sql, sub: fmt(h.ts) }));
    return snips.filter((s) => !f || s.name.toLowerCase().includes(f) || s.sql.toLowerCase().includes(f)).map((s) => ({ id: s.id, title: s.name, sql: s.sql, sub: s.connId ? "this data source" : "everywhere" }));
  }, [tab, items, snips, q]);
  const cur = list[Math.min(idx, list.length - 1)];
  const del = async (r: Row) => {
    if (tab === "history") await history.remove(r.id);
    else if (confirm(`Delete snippet "${r.title}"?`)) await history.deleteSnippet(r.id);
    reload();
  };
  const toSnippet = async (r: Row) => {
    if (await saveSnippet(r.sql, connId, title.replace(/ Query History$/, ""))) {
      reload();
      setTab("snippets");
    }
  };

  return (
    <div className="modal-backdrop" onMouseDown={onClose}>
      <div
        className="modal history"
        onMouseDown={(e) => e.stopPropagation()}
        onKeyDown={(e) => {
          if (e.key === "ArrowDown") setIdx((i) => Math.min(i + 1, list.length - 1));
          else if (e.key === "ArrowUp") setIdx((i) => Math.max(i - 1, 0));
          else if (e.key === "Enter" && cur) onPick(cur.sql);
          else if (e.key === "Escape") onClose();
          else if ((e.key === "Delete" || (e.metaKey && e.key === "Backspace")) && cur) del(cur);
          else return;
          e.preventDefault();
        }}
      >
        <div className="modal-title">{title}</div>
        <div className="dlg-tabs">
          <span className={tab === "history" ? "on" : ""} onClick={() => (setTab("history"), setIdx(0))}>
            History
          </span>
          <span className={tab === "snippets" ? "on" : ""} onClick={() => (setTab("snippets"), setIdx(0))}>
            Snippets ({snips.length})
          </span>
        </div>
        <input autoFocus className="hist-filter" placeholder="Type to filter" value={q} onChange={(e) => { setQ(e.target.value); setIdx(0); }} />
        <div className="hist-body">
          <div className="hist-list">
            {list.length === 0 && <div className="muted pad">{tab === "history" ? "No queries yet." : "No snippets yet. Save one from the history, or ⌘k › Save Query as Snippet."}</div>}
            {list.map((r, i) => (
              <div key={r.id} className={"hist-item" + (r === cur ? " on" : "")} onClick={() => setIdx(i)} onDoubleClick={() => onPick(r.sql)}>
                <span className="hist-sql">{r.title}</span>
                <span className="hist-ts">{r.sub}</span>
              </div>
            ))}
          </div>
          <pre className="hist-preview">{cur?.sql ?? ""}</pre>
        </div>
        <div className="modal-actions">
          <span className="muted">Double-click or ↩ to paste into the console · ⌦ to delete</span>
          <span className="grow" />
          {tab === "history" && (
            <button disabled={!cur} onClick={() => cur && toSnippet(cur)}>
              Save as Snippet…
            </button>
          )}
          <button onClick={onClose}>Close</button>
          <button className="primary" disabled={!cur} onClick={() => cur && onPick(cur.sql)}>
            Paste to Console
          </button>
        </div>
      </div>
    </div>
  );
}
