import { useEffect, useMemo, useRef, useState } from "react";
import { ConnectionView, dbShown, defaultDbOf, meta, SearchScope, targetId } from "../lib/api";
import { nextScope, SEARCH_SCOPES, scopeConns, shownScope } from "../features/search/scope";
import { compareRanked } from "../features/search/rank";
import { fuzzyScore } from "../lib/fuzzy";
import { useSharedReady } from "../lib/shared";
import { objectIcon } from "../ui/icons";
import { bindable, cmd, eventSpec, hit } from "../lib/shortcuts";
import { InfoTip, Kbd, KeyHints, List, ListGroup, ListItem, Notice, Overlay, Popover, Row, SearchField, Select, Spacer, Tabs } from "../ui";

/** First key of a command, for the key hints */
const sk = (id: string) => cmd(id).shortcut;

export type SeTab = "all" | "database" | "files" | "symbols" | "actions";
const TABS: { id: SeTab; label: string }[] = [
  { id: "all", label: "All" },
  { id: "database", label: "Database" },
  { id: "files", label: "Files" },
  { id: "symbols", label: "Symbols" },
  { id: "actions", label: "Actions" },
];

export interface DbObject {
  connId: string;
  db: string;
  schema: string;
  name: string;
  /** schema | table | view | matview | sequence | foreign | function | procedure | column */
  kind: string;
  /** Owning table, for a column */
  table?: string;
  type?: string;
  /** Unconnected Data Source: from the saved list */
  cached?: boolean;
}
export interface FileItem {
  key: string;
  icon: React.ReactNode;
  title: string;
  hint: string;
  open: boolean;
}
export interface ActionItem {
  /** The keymap command it runs (lib/shortcuts.ts): its key runs it too */
  id?: string;
  name: string;
  shortcut?: string;
  group?: string;
  run: () => void;
  disabled?: boolean;
}

type Row =
  | { t: "db"; o: DbObject; score: number }
  | { t: "file"; f: FileItem; score: number }
  | { t: "action"; a: ActionItem; score: number };

// ── Object index cache (per target DB). Invalidated on Refresh(⌥⌘y)·reconnect ──
const objCache = new Map<string, Promise<DbObject[]>>();
const colCache = new Map<string, Promise<DbObject[]>>();
const dbCache = new Map<string, Promise<string[]>>();
export function invalidateSearchIndex(connId?: string) {
  for (const m of [objCache, colCache, dbCache] as Map<string, unknown>[])
    for (const k of [...m.keys()]) if (!connId || k === connId || k.startsWith(connId + "::")) m.delete(k);
}
/** The databases searched (as the TUI's ⌃p): the ones the explorer shows, the default one first, at most 8. Redis: its default database's keys */
async function targetsOf(c: ConnectionView): Promise<string[]> {
  const def = defaultDbOf(c);
  if (c.options?.singleDb || c.kind === "redis") return [def];
  if (!dbCache.has(c.id)) dbCache.set(c.id, meta.databases(c.id).catch(() => [def]));
  const dbs = [...new Set([def, ...(await dbCache.get(c.id)!).filter((d) => dbShown(c, d, def))])].filter(Boolean);
  return (dbs.length ? dbs : [def]).slice(0, 8);
}
function objectsOf(c: ConnectionView, db: string): Promise<DbObject[]> {
  const key = targetId(c.id, db);
  if (!objCache.has(key))
    objCache.set(
      key,
      meta.objectIndex(key).then(
        (xs) => xs.map((x) => ({ connId: c.id, db, schema: x.schema, name: x.name, kind: x.kind })),
        (e) => {
          objCache.delete(key);
          throw e;
        },
      ),
    );
  return objCache.get(key)!;
}
function columnsOf(c: ConnectionView, db: string): Promise<DbObject[]> {
  const key = targetId(c.id, db);
  if (!colCache.has(key))
    colCache.set(
      key,
      meta.completion(key).then(
        (ts) => ts.flatMap((t) => t.columns.map(([name, type]) => ({ connId: c.id, db, schema: t.schema, name, kind: "column", table: t.name, type }))),
        (e) => {
          colCache.delete(key);
          throw e;
        },
      ),
    );
  return colCache.get(key)!;
}

interface Props {
  initialTab: SeTab;
  conns: ConnectionView[];
  /** Context Data Sources: connections currently in use */
  contextConnId?: string;
  /** This Workspace: the data sources in the current workspace's explorer */
  wsConnIds: string[];
  /** The scope picked last (settings.json, shared with the TUI) */
  scope: SearchScope;
  onScopeChange: (s: SearchScope) => void;
  files: FileItem[];
  actions: ActionItem[];
  onClose: () => void;
  onSelectInTree: (o: DbObject) => void;
  onOpenData: (o: DbObject) => void;
  onDdl: (o: DbObject) => void;
  onQuickDoc: (o: DbObject) => void;
  onOpenFile: (key: string) => void;
}

export function SearchEverywhere(p: Props) {
  const [tab, setTab] = useState<SeTab>(p.initialTab);
  const [q, setQ] = useState("");
  const [sel, setSel] = useState(0);
  const [objects, setObjects] = useState<DbObject[]>([]);
  const [columns, setColumns] = useState<DbObject[]>([]);
  const [loading, setLoading] = useState(0);
  const [errors, setErrors] = useState<string[]>([]);
  const listRef = useRef<HTMLDivElement>(null);

  // Every kind with objects: PostgreSQL, MySQL / MariaDB, driver databases, Redis keys (the TUI's ⌃p searches the same)
  const scope = shownScope(p.scope, p.contextConnId);
  const inScope = scopeConns(scope, p.conns, p.wsConnIds, p.contextConnId);
  const liveConns = inScope.filter((c) => c.connected);
  // Unconnected Data Sources are searched in the list read last time (connects when picked; Redis keys are never kept)
  const cachedConns = inScope.filter((c) => !c.connected);
  const scopeKey = inScope.map((c) => c.id + (c.connected ? "+" : "")).join(",");

  // Load the target DB index
  useEffect(() => {
    let alive = true;
    const wantCols = tab === "symbols" || tab === "all";
    setErrors([]);
    (async () => {
      const jobs: Promise<void>[] = [];
      const objs: DbObject[][] = [];
      const cols: DbObject[][] = [];
      if (cachedConns.length) {
        const all = await meta.cachedIndex().catch(() => []);
        for (const ci of all) {
          const i = ci.target.indexOf("::");
          const [cid, db] = i < 0 ? [ci.target, ""] : [ci.target.slice(0, i), ci.target.slice(i + 2)];
          if (!cachedConns.some((c) => c.id === cid)) continue;
          objs.push(ci.objects.map((x) => ({ connId: cid, db, schema: x.schema, name: x.name, kind: x.kind, cached: true })));
          if (wantCols) cols.push(ci.columns.flatMap((t) => t.columns.map(([name, type]) => ({ connId: cid, db, schema: t.schema, name, kind: "column", table: t.name, type, cached: true }))));
        }
      }
      for (const c of liveConns) {
        for (const db of await targetsOf(c)) {
          // The scope changed while the list was read (the cached index, the databases): the next run counts its own loads
          if (!alive) return;
          setLoading((n) => n + 1);
          jobs.push(
            objectsOf(c, db)
              .then((xs) => void objs.push(xs))
              .catch((e) => {
                if (alive) setErrors((es) => [...es, `${c.name}/${db}: ${e}`]);
              })
              .finally(() => {
                if (alive) setLoading((n) => n - 1);
              }),
          );
          if (wantCols && c.kind !== "redis") jobs.push(columnsOf(c, db).then((xs) => void cols.push(xs)).catch(() => {}));
        }
      }
      // Show whichever finishes first
      const flush = () => {
        if (!alive) return;
        setObjects(objs.flat());
        setColumns(cols.flat());
      };
      jobs.forEach((j) => j.then(flush));
      await Promise.allSettled(jobs);
      flush();
    })();
    return () => {
      alive = false;
      setLoading(0);
    };
  }, [scopeKey, tab]);

  const connName = (id: string) => p.conns.find((c) => c.id === id)?.name ?? "";
  // Scores come from the WASM module (unranked substring matches until it has loaded): rank again once it has
  const wasmReady = useSharedReady();
  const rows: Row[] = useMemo(() => {
    const query = q.trim();
    const out: Row[] = [];
    const matchDb = (o: DbObject) => {
      const s1 = fuzzyScore(query, o.name);
      const s2 = query.includes(".") ? fuzzyScore(query, `${o.table ? o.table : o.schema}.${o.name}`) : null;
      const s = Math.max(s1 ?? -Infinity, s2 ?? -Infinity);
      return s === -Infinity ? null : s;
    };
    if (tab === "all" || tab === "database") {
      if (query || tab === "database")
        for (const o of objects) {
          const s = matchDb(o);
          if (s !== null) out.push({ t: "db", o, score: s + (o.kind === "table" ? 3 : o.kind === "schema" ? 1 : 0) });
        }
    }
    if (tab === "symbols" || (tab === "all" && query))
      for (const o of tab === "symbols" ? [...columns, ...objects.filter((x) => x.kind !== "schema")] : columns) {
        const s = matchDb(o);
        if (s !== null) out.push({ t: "db", o, score: s - (tab === "all" ? 50 : 0) });
      }
    if (tab === "all" || tab === "files")
      for (const f of p.files) {
        const s = fuzzyScore(query, f.title);
        if (s !== null) out.push({ t: "file", f, score: s + (query ? 0 : 1000) });
      }
    if (tab === "all" || tab === "actions") {
      if (query || tab === "actions")
        for (const a of p.actions) {
          const s = fuzzyScore(query, a.name);
          if (s !== null) out.push({ t: "action", a, score: s - (tab === "all" ? 20 : 0) });
        }
    }
    // Same score: database results first (as they were added) in a fixed order, not the order their sources answered in; files and
    // actions after them in their own order
    const ranked = (o: DbObject) => ({ name: o.name, kind: o.kind, where: o.table ? `${o.schema}.${o.table}` : o.schema, source: connName(o.connId), db: o.db });
    const at = new Map(out.map((r, i) => [r, i]));
    const tie = (a: Row, b: Row) =>
      a.t === "db" && b.t === "db" ? compareRanked(ranked(a.o), ranked(b.o)) : a.t === "db" ? -1 : b.t === "db" ? 1 : at.get(a)! - at.get(b)!;
    return out.sort((a, b) => b.score - a.score || tie(a, b)).slice(0, 200);
  }, [q, tab, objects, columns, p.files, p.actions, wasmReady]);

  useEffect(() => setSel(0), [q, tab]);
  useEffect(() => {
    listRef.current?.querySelector(".se-row.sel")?.scrollIntoView({ block: "nearest" });
  }, [sel]);

  const cur = rows[Math.min(sel, rows.length - 1)];
  const run = (r: Row | undefined, how: "enter" | "data" | "ddl" | "doc" | "select" = "enter") => {
    if (!r) return;
    if (r.t === "action") {
      if (r.a.disabled) return;
      p.onClose();
      return r.a.run();
    }
    if (r.t === "file") {
      p.onClose();
      return p.onOpenFile(r.f.key);
    }
    const o = r.o.kind === "column" ? { ...r.o, kind: "table", name: r.o.table!, table: undefined } : r.o;
    // A Redis key isn't in the tree: it opens in the Redis tab (as the explorer's databases do), or its DDL
    if (o.kind === "key") {
      p.onClose();
      return how === "ddl" ? p.onDdl(o) : p.onOpenData(o);
    }
    if (how === "doc") return p.onQuickDoc(o);
    p.onClose();
    const relation = ["table", "view", "matview", "foreign"].includes(o.kind);
    if (how === "data" && relation) return p.onOpenData(o);
    if (how === "ddl" && o.kind !== "schema") return p.onDdl(o);
    p.onSelectInTree(o);
  };

  const onKey = (e: React.KeyboardEvent) => {
    e.stopPropagation();
    const mod = e.metaKey || e.ctrlKey;
    const go = (fn: () => void) => {
      e.preventDefault();
      fn();
    };
    if (e.key === "Escape") return go(p.onClose);
    if (e.key === "Tab") return go(() => setTab(TABS[(TABS.findIndex((t) => t.id === tab) + (e.shiftKey ? -1 : 1) + TABS.length) % TABS.length].id));
    // The Database Explorer's keys act on the selected result (DataGrip): Edit Data, Go to DDL, Quick Documentation, Select In.
    // Only command shortcuts (a modifier or an F key): Backspace, Delete, Insert or ⇧↩ given to a tree command stay the search field's
    const s = eventSpec(e);
    const spec = s && bindable(s) ? s : null;
    // The key that opens Go to Database Object, again: the next scope (DataGrip: the key again includes non-project items)
    if (hit(spec, "goto.object")) return go(() => p.onScopeChange(nextScope(scope, p.contextConnId)));
    if (hit(spec, "tree.editData")) return go(() => run(cur, "data"));
    if (hit(spec, "tree.ddl")) return go(() => run(cur, "ddl"));
    if (hit(spec, "selectIn")) return go(() => run(cur, "select"));
    if (hit(spec, "tree.quickDoc")) return go(() => run(cur, "doc"));
    if (e.key === "ArrowDown" && !mod) return go(() => setSel((x) => Math.min(x + 1, rows.length - 1)));
    if (e.key === "ArrowUp") return go(() => setSel((x) => Math.max(x - 1, 0)));
    if (e.key === "PageDown") return go(() => setSel((x) => Math.min(x + 10, rows.length - 1)));
    if (e.key === "PageUp") return go(() => setSel((x) => Math.max(x - 10, 0)));
    if (e.key === "Enter") return go(() => run(cur));
  };

  /** One result row: icon · name · description ……… right side (shortcut · location) */
  const item = (r: Row, selected: boolean, onEnter: () => void, onClick: () => void) => {
    const common = { selected, onMouseEnter: onEnter, onClick };
    if (r.t === "action") return <ListItem {...common} icon="⚡" title={r.a.name} dim={r.a.disabled} hint={r.a.group} trailing={r.a.shortcut && <Kbd>{r.a.shortcut}</Kbd>} />;
    if (r.t === "file") return <ListItem {...common} icon={r.f.icon} title={r.f.title} dim={!r.f.open} trailing={r.f.hint} />;
    const o = r.o;
    const Icon = objectIcon(o.kind);
    const where = o.kind === "schema" || o.kind === "key" ? "" : o.kind === "column" ? `${o.schema}.${o.table}` : o.schema;
    return (
      <ListItem
        {...common}
        icon={Icon ? <Icon /> : "•"}
        title={o.name}
        hint={o.kind === "column" ? `${o.type} · ${where}` : where}
        trailing={
          <span title={o.cached ? "Not connected — from the last time it was loaded. Opening it connects." : undefined}>
            {connName(o.connId)} · {o.db}
            {o.cached && " (cached)"}
          </span>
        }
      />
    );
  };

  const groupOf = (r: Row) => (r.t === "action" ? "Actions" : r.t === "file" ? "Files" : r.o.kind === "column" ? "Symbols" : "Database");

  return (
    <Overlay variant="clear" onClose={p.onClose}>
      <Popover
        className="search-everywhere"
        head={
          <>
            <Row gap={2} className="se-head">
              <Tabs size="sm" items={TABS.map((t) => ({ id: t.id, label: t.label }))} value={tab} onChange={setTab} />
              <Spacer />
              {(tab === "database" || tab === "symbols" || tab === "all") && (
                <>
                  <Select value={scope} onChange={(e) => p.onScopeChange(e.target.value as SearchScope)} aria-label="Search scope">
                    {SEARCH_SCOPES.map((s) => (
                      <option key={s.id} value={s.id} disabled={s.id === "context" && !p.contextConnId}>
                        {s.label}
                      </option>
                    ))}
                  </Select>
                  <InfoTip
                    about="Search scope"
                    info={[
                      "This Workspace: the data sources in this workspace's Database Explorer.",
                      "All Places: every data source.",
                      "Context Data Sources: the data source of the selected explorer node or the active tab; off when there is none.",
                      "Data sources that aren't connected are searched in the list read the last time they were.",
                      `${sk("goto.object") ?? "Go to Database Object"} again: the next scope. The last one picked is kept, for the TUI too.`,
                    ]}
                  />
                </>
              )}
            </Row>
            <SearchField bare autoFocus busy={loading > 0} value={q} onChange={(e) => setQ(e.target.value)} onKeyDown={onKey} placeholder={tab === "actions" ? "Type action name" : "Type / to see commands"} />
          </>
        }
        foot={
          <KeyHints
            items={
              cur?.t === "db" && cur.o.kind === "key"
                ? ([["↩", "Open"], [sk("tree.ddl"), "Go to DDL"], ["⇥", "Next tab"]].filter((h): h is [string, string] => !!h[0]))
                : cur?.t === "db"
                ? ([["↩", "Select in Database Explorer"], [sk("tree.editData"), "Edit Data"], [sk("tree.ddl"), "Go to DDL"], [sk("tree.quickDoc"), "Quick Documentation"], ["⇥", "Next tab"]].filter((h): h is [string, string] => !!h[0]))
                : [["↩", "Open"], ["⇥", "Next tab"], ["⇧⇧", "Search Everywhere"]]
            }
          />
        }
      >
        <List
          listRef={listRef}
          empty={loading > 0 ? "Loading…" : !liveConns.length && !objects.length && tab !== "files" && tab !== "actions" ? (scope === "workspace" && !inScope.length ? "No data sources in this workspace. Pick All Places to search the others." : "No connected data sources. Connect one to search database objects.") : scope === "workspace" && tab !== "files" && tab !== "actions" ? "Nothing found in This Workspace" : "Nothing found"}
        >
          {rows.map((r, i) => {
            const g = groupOf(r);
            const showGroup = tab === "all" && (i === 0 || groupOf(rows[i - 1]) !== g);
            return (
              <div key={i}>
                {showGroup && <ListGroup>{g}</ListGroup>}
                {item(r, i === sel, () => setSel(i), () => run(r))}
              </div>
            );
          })}
          {errors.map((er) => (
            <Notice key={er} tone="danger">
              {er}
            </Notice>
          ))}
        </List>
      </Popover>
    </Overlay>
  );
}
