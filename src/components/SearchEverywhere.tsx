import { useEffect, useMemo, useRef, useState } from "react";
import { ConnectionView, dbVisible, meta, targetId } from "../lib/api";
import { fuzzyScore } from "../lib/fuzzy";
import { ColumnIcon, RoutineIcon, SchemaIcon, SequenceIcon, TableIcon, ViewIcon } from "../ui/icons";
import { Kbd, KeyHints, List, ListGroup, ListItem, Notice, Overlay, Popover, Row, SearchField, Select, Spacer, Tabs } from "../ui";

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
  icon: string;
  title: string;
  hint: string;
  open: boolean;
}
export interface ActionItem {
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
const defaultDbOf = (c: ConnectionView) => c.database || (c.kind === "postgres" ? "postgres" : "");
async function targetsOf(c: ConnectionView): Promise<string[]> {
  if (c.options?.singleDb) return [defaultDbOf(c)];
  if (!dbCache.has(c.id)) dbCache.set(c.id, meta.databases(c.id).catch(() => [defaultDbOf(c)]));
  const dbs = await dbCache.get(c.id)!;
  return dbs.filter((d) => dbVisible(c.scope, d, defaultDbOf(c)) && !["template0", "template1", "rdsadmin"].includes(d)).slice(0, 8);
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

const KIND_ICON: Record<string, React.ReactNode> = {
  schema: <SchemaIcon />,
  table: <TableIcon />,
  foreign: <TableIcon />,
  view: <ViewIcon />,
  matview: <ViewIcon />,
  sequence: <SequenceIcon />,
  function: <RoutineIcon />,
  procedure: <RoutineIcon />,
  column: <ColumnIcon />,
};

interface Props {
  initialTab: SeTab;
  conns: ConnectionView[];
  /** Context Data Sources: connections currently in use */
  contextConnId?: string;
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
  const [onlyContext, setOnlyContext] = useState(false);
  const [objects, setObjects] = useState<DbObject[]>([]);
  const [columns, setColumns] = useState<DbObject[]>([]);
  const [loading, setLoading] = useState(0);
  const [errors, setErrors] = useState<string[]>([]);
  const listRef = useRef<HTMLDivElement>(null);

  const scopeConns = p.conns.filter((c) => c.connected && c.kind === "postgres" && (!onlyContext || c.id === p.contextConnId));
  // Unconnected Data Sources are searched in the list read last time (connects when picked)
  const cachedConns = p.conns.filter((c) => !c.connected && c.kind === "postgres" && (!onlyContext || c.id === p.contextConnId));
  const scopeKey = [...scopeConns, ...cachedConns].map((c) => c.id + (c.connected ? "+" : "")).join(",");

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
      for (const c of scopeConns) {
        for (const db of await targetsOf(c)) {
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
          if (wantCols) jobs.push(columnsOf(c, db).then((xs) => void cols.push(xs)).catch(() => {}));
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
    // Same score: shorter name first (stable sort)
    return out.sort((a, b) => b.score - a.score).slice(0, 200);
  }, [q, tab, objects, columns, p.files, p.actions]);

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
    if (e.key === "ArrowDown" && !mod) return go(() => setSel((x) => Math.min(x + 1, rows.length - 1)));
    if (e.key === "ArrowUp") return go(() => setSel((x) => Math.max(x - 1, 0)));
    if (e.key === "PageDown") return go(() => setSel((x) => Math.min(x + 10, rows.length - 1)));
    if (e.key === "PageUp") return go(() => setSel((x) => Math.max(x - 10, 0)));
    if (e.key === "ArrowDown" && mod) return go(() => run(cur, "data"));
    if (e.key === "Enter") return go(() => run(cur));
    if (mod && e.key.toLowerCase() === "b") return go(() => run(cur, "ddl"));
    if (e.key === "F1" && e.altKey) return go(() => run(cur, "select"));
    if (e.key === "F1") return go(() => run(cur, "doc"));
  };

  /** One result row: icon · name · description ……… right side (shortcut · location) */
  const item = (r: Row, selected: boolean, onEnter: () => void, onClick: () => void) => {
    const common = { selected, onMouseEnter: onEnter, onClick };
    if (r.t === "action") return <ListItem {...common} icon="⚡" title={r.a.name} dim={r.a.disabled} hint={r.a.group} trailing={r.a.shortcut && <Kbd>{r.a.shortcut}</Kbd>} />;
    if (r.t === "file") return <ListItem {...common} icon={r.f.icon} title={r.f.title} dim={!r.f.open} trailing={r.f.hint} />;
    const o = r.o;
    const where = o.kind === "schema" ? "" : o.kind === "column" ? `${o.schema}.${o.table}` : o.schema;
    return (
      <ListItem
        {...common}
        icon={KIND_ICON[o.kind] ?? "•"}
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
                <Select value={onlyContext ? "ctx" : "all"} onChange={(e) => setOnlyContext(e.target.value === "ctx")} title="Search scope">
                  <option value="all">All Places</option>
                  <option value="ctx" disabled={!p.contextConnId}>
                    Context Data Sources
                  </option>
                </Select>
              )}
            </Row>
            <SearchField bare autoFocus busy={loading > 0} value={q} onChange={(e) => setQ(e.target.value)} onKeyDown={onKey} placeholder={tab === "actions" ? "Type action name" : "Type / to see commands"} />
          </>
        }
        foot={
          <KeyHints
            items={
              cur?.t === "db"
                ? [["↩", "Select in Database Explorer"], ["⌘↓", "Edit Data"], ["⌘b", "Go to DDL"], ["F1", "Quick Documentation"], ["⇥", "Next tab"]]
                : [["↩", "Open"], ["⇥", "Next tab"], ["⇧⇧", "Search Everywhere"]]
            }
          />
        }
      >
        <List
          listRef={listRef}
          empty={loading > 0 ? "Loading…" : !scopeConns.length && !objects.length && tab !== "files" && tab !== "actions" ? "No connected data sources. Connect one to search database objects." : "Nothing found"}
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
