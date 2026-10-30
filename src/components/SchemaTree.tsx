import { connBg, connStripe, useTheme } from "../lib/theme";
import { createContext, useContext, useEffect, useId, useRef, useState } from "react";
import { api, ConnectionView, DbKind, DRIVER_MISSING, KIND_LABEL, redisDb, dbVisible, meta, RelationInfo, RoutineInfo, SchemaInfo, schemaVisible, Scope, TableDetail, targetId } from "../lib/api";
import { ScopePopup } from "./ScopePopup";
import { Button, Notice, TreeRow } from "../ui";
import { invalidateCompletion } from "../lib/completion";
import { TYPE_COLOR } from "./RedisBrowser";
import { keyTree, type KeyNode } from "../lib/keyTree";
import { ColumnIcon, DatabaseIcon, FkIcon, FolderIcon, IndexIcon, KeyIcon, MySqlIcon, PgIcon, RedisIcon, RoutineIcon, SchemaIcon, SequenceIcon, TableIcon, ViewIcon } from "../ui/icons";

/** What a tree node points to. The context menu and shortcuts (F4, ⌘b, ⌘⇧L) act on this. */
export interface NodeInfo {
  kind: "conn" | "db" | "schema" | "table" | "view" | "matview" | "foreign" | "partitioned" | "sequence" | "routine" | "key";
  conn: ConnectionView;
  db?: string;
  schema?: string;
  name?: string;
  refresh?: () => void;
  /** Object node: reload the parent (the schema's object list) (after DROP) */
  refreshParent?: () => void;
}

/** Select In Database Explorer (⌥F1, Search Everywhere results): expand the tree and select the node */
export interface RevealTarget { connId: string; db?: string; schema?: string; name?: string; kind?: string }
let pendingReveal: RevealTarget | null = null;
/** After running a structure-changing statement (DDL) in the console: reload the already-loaded parts of that database's tree */
export function schemaChanged(connId: string, db: string) {
  window.dispatchEvent(new CustomEvent("dbtool-schema-changed", { detail: { connId, db } }));
}
/** Explorer Collapse All: collapse every Data Source · DB · schema · folder (connections stay) */
export function collapseTree() {
  window.dispatchEvent(new CustomEvent("dbtool-collapse"));
}
function useCollapse(fn: () => void) {
  const ref = useRef(fn);
  ref.current = fn;
  useEffect(() => {
    const h = () => ref.current();
    window.addEventListener("dbtool-collapse", h);
    return () => window.removeEventListener("dbtool-collapse", h);
  }, []);
}
const nodeKey = (connId: string, db?: string, schema?: string, name?: string) => [connId, db ?? "", schema ?? "", name ?? ""].join("|");
export function revealInTree(t: RevealTarget) {
  pendingReveal = t;
  window.dispatchEvent(new CustomEvent("dbtool-reveal"));
  const key = nodeKey(t.connId, t.db, t.schema, t.name);
  let tries = 0;
  const tick = () => {
    const el = document.querySelector<HTMLElement>(`[data-node="${CSS.escape(key)}"]`);
    if (el) {
      el.dispatchEvent(new MouseEvent("mousedown", { bubbles: true }));
      el.scrollIntoView({ block: "center" });
      (el.closest("[data-tree]") as HTMLElement | null)?.focus();
      pendingReveal = null;
    } else if (++tries < 60) setTimeout(tick, 100);
  };
  setTimeout(tick, 50);
}
function useReveal(fn: (t: RevealTarget) => void) {
  // Attach the listener once on mount (not again on every render). fn uses the latest one via a ref
  const ref = useRef(fn);
  ref.current = fn;
  useEffect(() => {
    const h = () => pendingReveal && ref.current(pendingReveal);
    h();
    window.addEventListener("dbtool-reveal", h);
    return () => window.removeEventListener("dbtool-reveal", h);
  }, []);
}

export const isRelation = (k: NodeInfo["kind"]) => ["table", "view", "matview", "foreign", "partitioned"].includes(k);

/** Tree selection state (DataGrip: click = select, ▸ / double-click = expand) */
export const TreeSelection = createContext<{ sel: string | null; setSel: (id: string, info?: NodeInfo) => void }>({
  sel: null,
  setSel: () => {},
});

interface Props {
  conn: ConnectionView;
  filter: string;
  showSystem: boolean;
  /** Table · view double-click = Edit Data (F4), routine double-click = DDL */
  onActivate: (info: NodeInfo) => void;
  onMenu: (e: React.MouseEvent, info: NodeInfo) => void;
  /** Apply from the schema picker popup */
  onScopeChange: (conn: ConnectionView, scope: Scope) => void;
}

type Load<T> = { state: "loading" } | { state: "error"; msg: string } | { state: "ok"; data: T };

const REL_GROUPS: { kind: RelationInfo["kind"][]; label: string; icon: string }[] = [
  { kind: ["table", "partitioned", "foreign"], label: "tables", icon: "▦" },
  { kind: ["view"], label: "views", icon: "◫" },
  { kind: ["matview"], label: "materialized views", icon: "◩" },
  { kind: ["sequence"], label: "sequences", icon: "#" },
  { kind: ["key"], label: "keys", icon: "🔑" },
];

function useSelectable(info?: NodeInfo) {
  const id = useId();
  const { sel, setSel } = useContext(TreeSelection);
  return { selected: sel === id, select: () => setSel(id, info) };
}

function Row({ depth, open, onToggle, icon, iconClass, label, sub, title, onDoubleClick, onMenu, leaf, info, style, className, badge }: {
  depth: number; open?: boolean; onToggle?: () => void; icon: React.ReactNode; iconClass?: string; label: string; sub?: string;
  title?: string; onDoubleClick?: () => void; onMenu?: (e: React.MouseEvent, info: NodeInfo) => void; leaf?: boolean;
  info?: NodeInfo; style?: React.CSSProperties; className?: string; badge?: React.ReactNode;
}) {
  const { selected, select } = useSelectable(info);
  return (
    <TreeRow
      depth={depth}
      open={open}
      leaf={leaf}
      icon={icon}
      iconClass={iconClass}
      label={label}
      sub={sub}
      badge={badge}
      selected={selected}
      title={title}
      className={className}
      style={style}
      dataNode={info ? nodeKey(info.conn.id, info.kind === "conn" ? undefined : info.db, info.schema, info.name) : undefined}
      onSelect={select}
      onToggle={onToggle}
      onDoubleClick={onDoubleClick}
      onContextMenu={(e) => {
        e.preventDefault();
        select();
        if (info && onMenu) onMenu(e, info);
      }}
    />
  );
}

function Status({ depth, load, kind, onRetry }: { depth: number; load: Load<unknown> | undefined; kind?: DbKind; onRetry?: () => void }) {
  const [installing, setInstalling] = useState(false);
  const [installErr, setInstallErr] = useState<string | null>(null);
  if (!load || load.state === "ok") return null;
  // So the driver can be downloaded right away if missing
  const missing = load.state === "error" && load.msg.includes(DRIVER_MISSING) && kind;
  return (
    <div className={"tree-status" + (load.state === "error" ? " err" : "")} style={{ paddingLeft: 24 + depth * 19 }}>
      {load.state === "loading" ? "Loading…" : load.msg}
      {missing && (
        <Button
          size="sm"
          style={{ marginLeft: 8 }}
          disabled={installing}
          onClick={async () => {
            setInstalling(true);
            try {
              await api.installDriver(kind);
              onRetry?.();
            } catch (e) {
              // The Tauri webview has no alert → show inline
              setInstallErr(String(e));
            } finally {
              setInstalling(false);
            }
          }}
        >
          {installing ? "Installing…" : `Install the ${KIND_LABEL[kind]} driver`}
        </Button>
      )}
      {installErr && <Notice tone="danger">{installErr}</Notice>}
    </div>
  );
}

export function SchemaTree({ conn, filter, showSystem, onActivate, onMenu, onScopeChange }: Props) {
  const [popup, setPopup] = useState<{ x: number; y: number; focusDb?: string } | null>(null);
  const [schemaCounts, setSchemaCounts] = useState<Record<string, { shown: number; total: number }>>({});
  const [opened, setOpened] = useState(false);
  const [dbs, setDbs] = useState<Load<string[]>>();
  const [openDbs, setOpenDbs] = useState<Set<string>>(new Set());
  const [gen, setGen] = useState(0); // Remount child nodes on refresh
  const theme = useTheme();
  const defaultDb = conn.kind === "redis" ? redisDb(conn.database) : conn.kind === "sqlite" || conn.kind === "duckdb" ? "main" : conn.database || (conn.kind === "postgres" ? "postgres" : "");

  // Single database mode: only the default DB
  const MY_SYS = ["information_schema", "performance_schema", "mysql", "sys"];
  const dbShown = (d: string) =>
    conn.options?.singleDb ? d === defaultDb : (conn.kind !== "mysql" || showSystem || !MY_SYS.includes(d) || d === defaultDb) && dbVisible(conn.scope, d, defaultDb);
  const loadDbs = (force = false) => {
    if (dbs?.state === "ok" && !force) return;
    setDbs({ state: "loading" });
    meta.databases(conn.id).then(
      (data) => {
        setDbs({ state: "ok", data });
        // Connected: fill the search cache so Search Everywhere finds things next time without connecting
        if (conn.kind !== "redis") meta.warmSearchCache(conn.id).catch(() => {});
        const others = data.filter((d) => !["postgres", "rdsadmin", "template0", "template1"].includes(d));
        const auto = conn.database ? defaultDb : others.length === 1 ? others[0] : others.length === 0 ? defaultDb : null;
        if (auto) setOpenDbs((s) => new Set(s).add(auto));
      },
      (e) => setDbs({ state: "error", msg: String(e) }),
    );
  };
  useCollapse(() => {
    setOpened(false);
    setOpenDbs(new Set());
  });
  useReveal((t) => {
    if (t.connId !== conn.id) return;
    if (!opened) {
      setOpened(true);
      loadDbs();
    }
    if (t.db && !openDbs.has(t.db)) setOpenDbs((x) => new Set(x).add(t.db!));
  });
  const toggleConn = () =>
    setOpened((o) => {
      if (!o) loadDbs();
      return !o;
    });
  const refreshAll = () => {
    // The schema may have changed, so make autocomplete metadata reload too
    invalidateCompletion(conn.id);
    setGen((g) => g + 1);
    loadDbs(true);
  };

  useEffect(() => {
    if (conn.connected && !opened) {
      setOpened(true);
      loadDbs();
    }
  }, [conn.connected]);

  return (
    <div className="conn-tree" style={conn.color ? { background: connBg(conn.color, theme), boxShadow: `inset 3px 0 0 ${connStripe(conn.color, theme)}` } : undefined}>
      <Row
        depth={0}
        open={opened}
        onToggle={toggleConn}
        icon={conn.kind === "postgres" ? <PgIcon /> : conn.kind === "mysql" ? <MySqlIcon /> : <RedisIcon />}
        label={conn.name}
        className="conn"
        title={`${conn.user}@${conn.host}:${conn.port}${conn.database ? "/" + conn.database : ""}`}
        badge={
          <>
            {conn.ssh?.enabled && <span className="ssh-badge" title={`SSH via ${conn.ssh.user}@${conn.ssh.host}:${conn.ssh.port}`}>SSH</span>}
            {dbs?.state === "ok" && (
              <span
                className="scope-badge"
                title="Choose databases and schemas to show"
                onMouseDown={(e) => e.stopPropagation()}
                onClick={(e) => {
                  e.stopPropagation();
                  setPopup({ x: e.clientX, y: e.clientY + 10 });
                }}
              >
                {dbs.data.filter((d) => dbShown(d)).length}
                {conn.scope?.databases?.length ? ` of ${dbs.data.length}` : ""}
              </span>
            )}
          </>
        }
        info={{ kind: "conn", conn, db: defaultDb, refresh: refreshAll }}
        onMenu={onMenu}
      />
      {opened && (
        <div key={gen}>
          <Status depth={1} load={dbs} kind={conn.kind} onRetry={() => loadDbs(true)} />
          {dbs?.state === "ok" &&
            dbs.data.filter((db) => dbShown(db)).map((db) => {
              const dOpen = openDbs.has(db);
              return (
                <div key={db}>
                  <Row
                    depth={1}
                    open={dOpen}
                    icon={<DatabaseIcon />}
                    label={db}
                    badge={
                      schemaCounts[db] && conn.kind === "postgres" ? (
                        <span
                          className="scope-badge"
                          title="Choose schemas to show"
                          onMouseDown={(e) => e.stopPropagation()}
                          onClick={(e) => {
                            e.stopPropagation();
                            setPopup({ x: e.clientX, y: e.clientY + 10, focusDb: db });
                          }}
                        >
                          {schemaCounts[db].shown} of {schemaCounts[db].total}
                        </span>
                      ) : null
                    }
                    onToggle={() =>
                      setOpenDbs((s) => {
                        const n = new Set(s);
                        if (n.has(db)) n.delete(db);
                        else n.add(db);
                        return n;
                      })
                    }
                    info={{ kind: "db", conn, db, refresh: refreshAll }}
                    onMenu={onMenu}
                  />
                  {dOpen && (
                    <DbNode
                      conn={conn}
                      db={db}
                      filter={filter}
                      showSystem={showSystem}
                      onActivate={onActivate}
                      onMenu={onMenu}
                      onCount={(shown, total) => setSchemaCounts((c) => (c[db]?.shown === shown && c[db]?.total === total ? c : { ...c, [db]: { shown, total } }))}
                    />
                  )}
                </div>
              );
            })}
        </div>
      )}
      {popup && dbs?.state === "ok" && (
        <ScopePopup
          conn={conn}
          dbs={dbs.data}
          defaultDb={defaultDb}
          focusDb={popup.focusDb}
          x={popup.x}
          y={popup.y}
          onClose={() => setPopup(null)}
          onApply={(scope) => {
            setPopup(null);
            onScopeChange(conn, scope);
          }}
        />
      )}
    </div>
  );
}

function DbNode({ conn, db, filter, showSystem, onActivate, onMenu, onCount }: {
  conn: ConnectionView; db: string; filter: string; showSystem: boolean;
  onActivate: Props["onActivate"]; onMenu: Props["onMenu"];
  onCount: (shown: number, total: number) => void;
}) {
  const target = targetId(conn.id, db);
  // MySQL: database == schema → show object folders directly without a schema row (same as DataGrip)
  // DBs that are the schema: tables without a schema level
  const flat = ["mysql", "redis", "sqlite", "clickhouse", "mongodb", "qdrant", "chroma", "milvus"].includes(conn.kind);
  const dd = flat ? -1 : 0;
  const [open, setOpen] = useState<Set<string>>(new Set());
  useCollapse(() => setOpen(new Set()));
  const [schemas, setSchemas] = useState<Load<SchemaInfo[]>>();
  const [objects, setObjects] = useState<Record<string, Load<{ relations: RelationInfo[]; routines: RoutineInfo[] }>>>({});
  const [details, setDetails] = useState<Record<string, Load<TableDetail>>>({});

  const toggle = (key: string, onOpen?: () => void) =>
    setOpen((s) => {
      const n = new Set(s);
      if (n.has(key)) n.delete(key);
      else {
        n.add(key);
        onOpen?.();
      }
      return n;
    });

  const loadObjects = (schema: string, force = false) => {
    if (objects[schema]?.state === "ok" && !force) return;
    setObjects((o) => ({ ...o, [schema]: { state: "loading" } }));
    meta.objects(target, schema).then(
      (data) => setObjects((o) => ({ ...o, [schema]: { state: "ok", data } })),
      (e) => setObjects((o) => ({ ...o, [schema]: { state: "error", msg: String(e) } })),
    );
  };
  const loadSchemas = () => {
    setSchemas({ state: "loading" });
    meta.schemas(target).then(
      (data) => {
        setSchemas({ state: "ok", data });
        const user = data.filter((x) => !x.isSystem);
        const auto = flat ? data[0]?.name : user.length === 1 ? user[0].name : user.find((x) => x.name === "public")?.name;
        if (auto) {
          setOpen((s) => new Set(s).add(`s:${auto}`).add(`s:${auto}/g:${conn.kind === "redis" ? "keys" : "tables"}`));
          loadObjects(auto);
        }
      },
      (e) => setSchemas({ state: "error", msg: String(e) }),
    );
  };
  const loadDetail = (schema: string, table: string, force = false) => {
    const key = `${schema}.${table}`;
    if (details[key]?.state === "ok" && !force) return;
    setDetails((d) => ({ ...d, [key]: { state: "loading" } }));
    meta.table(target, schema, table).then(
      (data) => setDetails((d) => ({ ...d, [key]: { state: "ok", data } })),
      (e) => setDetails((d) => ({ ...d, [key]: { state: "error", msg: String(e) } })),
    );
  };

  useEffect(() => {
    loadSchemas();
  }, []);
  // After DDL: keep the expanded layout and quietly reload the loaded schema lists · objects · table details
  const latest = useRef({ objects, details });
  latest.current = { objects, details };
  useEffect(() => {
    const h = (e: Event) => {
      const d = (e as CustomEvent<{ connId: string; db: string }>).detail;
      if (d.connId !== conn.id || d.db !== db) return;
      invalidateCompletion(conn.id);
      meta.schemas(target).then((data) => setSchemas({ state: "ok", data }), () => {});
      for (const [schema, l] of Object.entries(latest.current.objects))
        if (l.state === "ok") meta.objects(target, schema).then((data) => setObjects((o) => ({ ...o, [schema]: { state: "ok", data } })), () => {});
      for (const [key, l] of Object.entries(latest.current.details)) {
        if (l.state !== "ok") continue;
        const i = key.indexOf(".");
        meta.table(target, key.slice(0, i), key.slice(i + 1)).then(
          (data) => setDetails((x) => ({ ...x, [key]: { state: "ok", data } })),
          // If the table is gone, clear its details (it also drops from the list)
          () => setDetails((x) => { const n = { ...x }; delete n[key]; return n; }),
        );
      }
    };
    window.addEventListener("dbtool-schema-changed", h);
    return () => window.removeEventListener("dbtool-schema-changed", h);
  }, [target]);
  useReveal((t) => {
    if (t.connId !== conn.id || t.db !== db || !t.schema) return;
    const sk = `s:${t.schema}`;
    const group =
      t.kind === "view" ? "views" : t.kind === "matview" ? "materialized views" : t.kind === "sequence" ? "sequences" : t.kind === "function" || t.kind === "procedure" || t.kind === "routine" ? "routines" : "tables";
    const want = t.name && t.kind !== "schema" ? [sk, `${sk}/g:${group}`] : [sk];
    if (want.some((k) => !open.has(k))) setOpen((x) => new Set([...x, ...want]));
    if (objects[t.schema]?.state !== "ok" && objects[t.schema]?.state !== "loading") loadObjects(t.schema);
  });

  // Schema count for the DB node badge ("1 of 3")
  useEffect(() => {
    if (schemas?.state === "ok")
      onCount(schemas.data.filter((s) => schemaVisible(conn.scope, db, s.name, s.isSystem, showSystem)).length, schemas.data.length);
  }, [schemas, conn.scope, showSystem]);

  const f = filter.trim().toLowerCase();
  const match = (name: string) => !f || name.toLowerCase().includes(f);

  return (
    <>
      <Status depth={2} load={schemas} />
      {schemas?.state === "ok" &&
        schemas.data
          .filter((s) => flat || schemaVisible(conn.scope, db, s.name, s.isSystem, showSystem))
          .map((s) => {
            const sk = `s:${s.name}`;
            const sOpen = flat || open.has(sk);
            const objs = objects[s.name];
            const item = (r: RelationInfo, depth: number) => {
              const tk = `${sk}/t:${r.name}`;
              const tOpen = open.has(tk);
              const det = details[`${s.name}.${r.name}`];
              const isRel = r.kind !== "sequence" && r.kind !== "key";
              const info: NodeInfo = {
                kind: r.kind,
                conn,
                db,
                schema: s.name,
                name: r.name,
                refresh: () => (isRel ? loadDetail(s.name, r.name, true) : loadObjects(s.name, true)),
                refreshParent: () => loadObjects(s.name, true),
              };
              return (
                <div key={tk}>
                  <Row
                    depth={depth}
                    open={tOpen}
                    leaf={!isRel}
                    icon={r.kind === "key" ? <span className="rd-dot" style={{ background: TYPE_COLOR[r.comment ?? ""] ?? "#777" }} title={r.comment ?? ""}>{({ string: "T", hash: "H", list: "L", set: "S", zset: "Z", stream: "X" } as Record<string, string>)[r.comment ?? ""] ?? "?"}</span> : r.kind === "sequence" ? <SequenceIcon /> : r.kind === "view" || r.kind === "matview" ? <ViewIcon /> : <TableIcon />}
                    label={r.name}
                    title={r.kind === "key" ? `${r.name} (${r.comment})` : r.comment ? `${r.name} — ${r.comment}` : r.name}
                    onToggle={isRel ? () => toggle(tk, () => loadDetail(s.name, r.name)) : undefined}
                    onDoubleClick={() => onActivate(info)}
                    info={info}
                    onMenu={onMenu}
                  />
                  {tOpen && isRel && (
                    <>
                      <Status depth={depth + 1} load={det} />
                      {det?.state === "ok" && <TableChildren depth={depth + 1} base={tk} d={det.data} open={open} toggle={toggle} />}
                    </>
                  )}
                </div>
              );
            };
            // Redis keys: folders split on ":" (like Medis)
            const keyRows = (nodes: KeyNode<RelationInfo>[], depth: number, base: string): React.ReactNode =>
              nodes.map((n) => {
                if (n.kind === "key") return item(n.item, depth);
                const fk = `${base}/f:${n.path}`;
                const fOpen = open.has(fk) || !!f;
                return (
                  <div key={fk}>
                    <Row depth={depth} open={fOpen} icon={<FolderIcon />} label={n.name} sub={String(n.count)} title={`${n.path}:*`} onToggle={() => toggle(fk)} />
                    {fOpen && keyRows(n.children, depth + 1, base)}
                  </div>
                );
              });
            return (
              <div key={sk}>
                {!flat && <Row
                  depth={2}
                  open={sOpen}
                  icon={<SchemaIcon />}
                  label={s.name}
                  title={s.name === "public" ? "standard public schema" : s.name}
                  onToggle={() => toggle(sk, () => loadObjects(s.name))}
                  info={{ kind: "schema", conn, db, schema: s.name, refresh: () => loadObjects(s.name, true) }}
                  onMenu={onMenu}
                />}
                {sOpen && (
                  <>
                    <Status depth={3 + dd} load={objs} />
                    {objs?.state === "ok" &&
                      REL_GROUPS.map((g) => ({ ...g, items: objs.data.relations.filter((r) => g.kind.includes(r.kind) && match(r.name)) }))
                        .filter((g) => g.items.length > 0)
                        .map((g) => {
                          const gk = `${sk}/g:${g.label}`;
                          const gOpen = open.has(gk) || !!f;
                          return (
                            <div key={gk}>
                              <Row depth={3 + dd} open={gOpen} icon={<FolderIcon />} label={g.label} sub={String(g.items.length)} onToggle={() => toggle(gk)} />
                              {gOpen && (g.label === "keys" ? keyRows(keyTree(g.items, (r) => r.name), 4 + dd, gk) : g.items.map((r) => item(r, 4 + dd)))}
                            </div>
                          );
                        })}
                    {objs?.state === "ok" && objs.data.routines.filter((r) => match(r.name)).length > 0 && (
                      <RoutineGroup
                        depth={3 + dd}
                        base={sk}
                        items={objs.data.routines.filter((r) => match(r.name))}
                        open={open}
                        toggle={toggle}
                        forceOpen={!!f}
                        conn={conn}
                        db={db}
                        schema={s.name}
                        onActivate={onActivate}
                        onMenu={onMenu}
                      />
                    )}
                  </>
                )}
              </div>
            );
          })}
    </>
  );
}

function TableChildren({ depth, base, d, open, toggle }: {
  depth: number; base: string; d: TableDetail; open: Set<string>; toggle: (k: string) => void;
}) {
  const groups: { key: string; label: string; count: number; render: () => React.ReactNode }[] = [
    {
      key: "columns",
      label: "columns",
      count: d.columns.length,
      render: () =>
        d.columns.map((c) => (
          <Row
            key={c.name}
            depth={depth + 1}
            leaf
            icon={<ColumnIcon pk={c.isPk} notNull={!c.nullable} />}
            label={c.name}
            sub={c.dataType}
            title={[`${c.name} ${c.dataType}${c.nullable ? "" : " not null"}${c.defaultValue ? " default " + c.defaultValue : ""}`, c.comment].filter(Boolean).join("\n")}
          />
        )),
    },
    {
      key: "keys",
      label: "keys",
      count: d.constraints.filter((c) => c.kind === "p" || c.kind === "u").length,
      render: () =>
        d.constraints
          .filter((c) => c.kind === "p" || c.kind === "u")
          .map((c) => <Row key={c.name} depth={depth + 1} leaf icon={<KeyIcon />} label={c.name} sub={c.definition} title={c.definition} />),
    },
    {
      key: "fks",
      label: "foreign keys",
      count: d.constraints.filter((c) => c.kind === "f").length,
      render: () =>
        d.constraints
          .filter((c) => c.kind === "f")
          .map((c) => <Row key={c.name} depth={depth + 1} leaf icon={<FkIcon />} label={c.name} sub={c.definition} title={c.definition} />),
    },
    {
      key: "indexes",
      label: "indexes",
      count: d.indexes.length,
      render: () =>
        d.indexes.map((i) => (
          <Row key={i.name} depth={depth + 1} leaf icon={<IndexIcon />} label={i.name} sub={i.isUnique ? "unique" : undefined} title={i.definition} />
        )),
    },
  ];
  return (
    <>
      {groups
        .filter((g) => g.count > 0)
        .map((g) => {
          const k = `${base}/${g.key}`;
          const o = g.key === "columns" ? !open.has(k + ":closed") : open.has(k);
          return (
            <div key={k}>
              <Row depth={depth} open={o} icon={<FolderIcon />} label={g.label} sub={String(g.count)} onToggle={() => toggle(g.key === "columns" ? k + ":closed" : k)} />
              {o && g.render()}
            </div>
          );
        })}
    </>
  );
}

function RoutineGroup({ depth, base, items, open, toggle, forceOpen, conn, db, schema, onActivate, onMenu }: {
  depth: number; base: string; items: RoutineInfo[]; open: Set<string>; toggle: (k: string) => void; forceOpen: boolean;
  conn: ConnectionView; db: string; schema: string; onActivate: Props["onActivate"]; onMenu: Props["onMenu"];
}) {
  const k = `${base}/g:routines`;
  const o = open.has(k) || forceOpen;
  return (
    <>
      <Row depth={depth} open={o} icon={<FolderIcon />} label="routines" sub={String(items.length)} onToggle={() => toggle(k)} />
      {o &&
        items.map((r, i) => {
          const info: NodeInfo = { kind: "routine", conn, db, schema, name: r.name };
          return (
            <Row
              key={r.name + i}
              depth={depth + 1}
              leaf
              icon={<RoutineIcon />}
              label={`${r.name}(${r.args})`}
              sub={r.returns ?? r.kind}
              title={`${r.kind} ${r.name}(${r.args})${r.returns ? " → " + r.returns : ""}`}
              onDoubleClick={() => onActivate(info)}
              info={info}
              onMenu={onMenu}
            />
          );
        })}
    </>
  );
}
