import { useEffect, useMemo, useState } from "react";
import { ask, askTextChoice } from "../lib/dialogs";
import { Button, CodeBlock, List, ListItem, Modal, SearchField, Tabs } from "../ui";
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
  const r = await askTextChoice("Snippet name", text.replace(/\s+/g, " ").slice(0, 40), [`Only for "${connName}"`, "For all data sources"]);
  if (!r?.text) return null;
  await history.saveSnippet({ id: "", name: r.text, sql: text, connId: r.choice === 0 ? connId : null });
  return r.text;
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
    else if (await ask(`Delete snippet "${r.title}"?`, { okLabel: "Delete", danger: true })) await history.deleteSnippet(r.id);
    reload();
  };
  const toSnippet = async (r: Row) => {
    if (await saveSnippet(r.sql, connId, title.replace(/ Query History$/, ""))) {
      reload();
      setTab("snippets");
    }
  };

  return (
    <Modal
      title={title}
      size="xl"
      className="history"
      onClose={onClose}
      onKeyDown={(e) => {
        if (e.key === "ArrowDown") setIdx((i) => Math.min(i + 1, list.length - 1));
        else if (e.key === "ArrowUp") setIdx((i) => Math.max(i - 1, 0));
        else if (e.key === "Enter" && cur && !(e.target as HTMLElement).closest("button")) onPick(cur.sql);
        else if ((e.key === "Delete" || (e.metaKey && e.key === "Backspace")) && cur && (e.target as HTMLElement).tagName !== "INPUT") del(cur);
        else return;
        e.preventDefault();
      }}
      footer={
        <>
          <span className="ui-modal-foot-hint">Double-click or ↩ to paste into the console · ⌦ to delete</span>
          {tab === "history" && (
            <Button disabled={!cur} onClick={() => cur && toSnippet(cur)}>
              Save as Snippet…
            </Button>
          )}
          <Button onClick={onClose}>Close</Button>
          <Button variant="primary" disabled={!cur} onClick={() => cur && onPick(cur.sql)}>
            Paste to Console
          </Button>
        </>
      }
    >
      <Tabs
        items={[
          { id: "history", label: "History" },
          { id: "snippets", label: `Snippets (${snips.length})` },
        ]}
        value={tab}
        onChange={(t) => (setTab(t), setIdx(0))}
      />
      <SearchField autoFocus className="hist-filter" placeholder="Type to filter" value={q} onChange={(e) => (setQ(e.target.value), setIdx(0))} />
      <div className="hist-body">
        <List boxed className="hist-list" empty={tab === "history" ? "No queries yet." : "No snippets yet. Save one from the history, or ⌘k › Save Query as Snippet."}>
          {list.map((r, i) => (
            <ListItem key={r.id} mono title={r.title} trailing={r.sub} selected={r === cur} onClick={() => setIdx(i)} onDoubleClick={() => onPick(r.sql)} />
          ))}
        </List>
        <CodeBlock className="hist-preview">{cur?.sql ?? ""}</CodeBlock>
      </div>
    </Modal>
  );
}
