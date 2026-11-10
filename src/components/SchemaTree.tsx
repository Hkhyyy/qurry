import { connBg, connStripe } from "../lib/theme";
import { createContext, useContext, useEffect, useId, useLayoutEffect, useRef, useState } from "react";
import { api, ConnectionView, DbKind, DRIVER_MISSING, KIND_LABEL, redis, redisDb, dbShown, meta, RelationInfo, RoutineInfo, SchemaInfo, schemaVisible, Scope, TableDetail, targetId } from "../lib/api";
import { ScopePopup } from "./ScopePopup";
import { Button, Notice, TreeRow } from "../ui";
import { invalidateCompletion } from "../lib/completion";
import { waitOn } from "../lib/reconnect";
import { TlsFix } from "./TlsFix";
import { ColumnIcon, DatabaseIcon, FkIcon, FolderIcon, IndexIcon, KeyIcon, MySqlIcon, objectIcon, PgIcon, RedisIcon, RoutineIcon, SchemaIcon, TableIcon } from "../ui/icons";

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

/** Tree selection state (DataGrip: click = select one, ⌘-click / ⇧-click = several, ▸ / double-click = expand).
 *  infos: what each selectable row on screen points to (rows add themselves), so the explorer can act on several.
 *  gone: a row left the screen (collapsed, filtered out, dropped), so it leaves the selection */
export const TreeSelection = createContext<{ ids: string[]; select: (id: string, info?: NodeInfo, e?: React.MouseEvent) => void; infos: Map<string, NodeInfo>; gone: (id: string) => void }>({
  ids: [],
  select: () => {},
  infos: new Map(),
  gone: () => {},
});

interface Props {
  conn: ConnectionView;
  filter: string;
  showSystem: boolean;
  /** Table · view double-click = Edit Data (F4), routine double-click = DDL. `click`: a single click (a Redis database switches the open Redis tab) */
  onActivate: (info: NodeInfo, click?: boolean) => void;
  onMenu: (e: React.MouseEvent, info: NodeInfo) => void;
  /** Apply from the schema picker popup */
  onScopeChange: (conn: ConnectionView, scope: Scope) => void;
  /** The saved source changed here (Allow unencrypted): read the data sources again */
  onConnsChanged?: () => void;
}

type Load<T> = { state: "loading" } | { state: "error"; msg: string } | { state: "ok"; data: T };

const REL_GROUPS: { kind: RelationInfo["kind"][]; label: string; icon: string }[] = [
  { kind: ["table", "partitioned", "foreign"], label: "tables", icon: "▦" },
  { kind: ["view"], label: "views", icon: "◫" },
  { kind: ["matview"], label: "materialized views", icon: "◩" },
  { kind: ["sequence"], label: "sequences", icon: "#" },
];

function useSelectable(info?: NodeInfo) {
  const id = useId();
  const { ids, select, infos, gone } = useContext(TreeSelection);
  useEffect(() => {
    if (info) infos.set(id, info);
    return () => void infos.delete(id);
  });
  // Unmount only (gone reads the explorer's current selection), in the commit that removes the row: a key pressed right after
  // already finds the selection without it
  useLayoutEffect(() => () => gone(id), []);
  return { id, selected: ids.includes(id), select: (e?: React.MouseEvent) => select(id, info, e) };
}

function Row({ depth, open, onToggle, icon, iconClass, label, sub, title, onClick, onDoubleClick, onMenu, leaf, info, style, className, badge }: {
  depth: number; open?: boolean; onToggle?: () => void; icon: React.ReactNode; iconClass?: string; label: string; sub?: string;
  title?: string; onClick?: () => void; onDoubleClick?: () => void; onMenu?: (e: React.MouseEvent, info: NodeInfo) => void; leaf?: boolean;
  info?: NodeInfo; style?: React.CSSProperties; className?: string; badge?: React.ReactNode;
}) {
  const { id, selected, select } = useSelectable(info);
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
      id={info ? id : undefined}
      onSelect={select}
      onToggle={onToggle}
      onClick={onClick}
      onDoubleClick={onDoubleClick}
      onContextMenu={(e) => {
        e.preventDefault();
        // A right-click on a node of the selection keeps the selection (the menu acts on all of it)
        if (!selected) select();
        if (info && onMenu) onMenu(e, info);
      }}
    />
  );
}

function Status({ depth, load, kind, conn, onRetry, onConnsChanged }: { depth: number; load: Load<unknown> | undefined; kind?: DbKind; conn?: ConnectionView; onRetry?: () => void; onConnsChanged?: () => void }) {
  const ref = useRef<HTMLDivElement>(null);
  const [installing, setInstalling] = useState(false);
  const [installErr, setInstallErr] = useState<string | null>(null);
  if (!load || load.state === "ok") return null;
  // So the driver can be downloaded right away if missing
  const missing = load.state === "error" && load.msg.includes(DRIVER_MISSING) && kind;
  return (
    <div ref={ref} className={"tree-status" + (load.state === "error" ? " err" : "")} style={{ paddingLeft: 24 + depth * 19 }}>
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
      {load.state === "error" && (
        <TlsFix
          conn={conn}
          msg={load.msg}
          then={() => {
            // The button goes away with the error: keep the keys in the tree (its rows still have the selection)
            ref.current?.closest<HTMLElement>(".tree")?.focus();
            onConnsChanged?.();
            onRetry?.();
          }}
        />
      )}
    </div>
  );
}

export function SchemaTree({ conn, filter, showSystem, onActivate, onMenu, onScopeChange, onConnsChanged }: Props) {
  const [popup, setPopup] = useState<{ x: number; y: number; focusDb?: string } | null>(null);
  const [schemaCounts, setSchemaCounts] = useState<Record<string, { shown: number; total: number }>>({});
  const [opened, setOpened] = useState(false);
  const [dbs, setDbs] = useState<Load<string[]>>();
  const [keyCounts, setKeyCounts] = useState<Record<string, number>>({});
  const [openDbs, setOpenDbs] = useState<Set<string>>(new Set());
  const [gen, setGen] = useState(0); // Remount child nodes on refresh
  const defaultDb = conn.kind === "redis" ? redisDb(conn.database) : conn.kind === "sqlite" || conn.kind === "duckdb" ? "main" : conn.database || (conn.kind === "postgres" ? "postgres" : "");

  // Single database mode: only the default DB
  const shown = (d: string) => dbShown(conn, d, defaultDb, showSystem);
  const loadDbs = (force = false) => {
    if (dbs?.state === "ok" && !force) return;
    setDbs({ state: "loading" });
    // Redis: its databases with their key counts (keys are browsed in the Redis tab, not in the tree)
    const list =
      conn.kind === "redis"
        ? redis.overview(conn.id).then((o) => {
            setKeyCounts(Object.fromEntries(o.databases.map((d) => [d.name, d.keys])));
            return o.databases.map((d) => d.name);
          })
        : meta.databases(conn.id);
    list.then(
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
    } else if (conn.connected && dbs?.state === "error") loadDbs(true); // Connected from elsewhere (a console's Allow unencrypted): the error is stale
  }, [conn.connected]);

  return (
    <div className="conn-tree" style={conn.color ? { background: connBg(conn.color), boxShadow: `inset 3px 0 0 ${connStripe(conn.color)}` } : undefined}>
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
            {conn.ssm?.enabled && <span className="ssh-badge" title={`AWS SSM via ${conn.ssm.target}${conn.ssm.profile || conn.ssm.region ? ` (${conn.ssm.profile || "default"}/${conn.ssm.region || "default"})` : ""}`}>SSM</span>}
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
                {dbs.data.filter(shown).length}
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
          <Status depth={1} load={dbs} kind={conn.kind} conn={conn} onRetry={() => loadDbs(true)} onConnsChanged={onConnsChanged} />
          {dbs?.state === "ok" &&
            dbs.data.filter(shown).map((db) => {
              const dOpen = openDbs.has(db);
              const info: NodeInfo = { kind: "db", conn, db, refresh: refreshAll };
              // Redis: a database is a leaf with its key count; a click switches the source's open Redis tab, a double-click opens it
              if (conn.kind === "redis")
                return (
                  <Row
                    key={db}
                    depth={1}
                    leaf
                    icon={<DatabaseIcon />}
                    label={db}
                    sub={keyCounts[db] >= 0 ? keyCounts[db].toLocaleString("en-US") : undefined}
                    onClick={() => onActivate(info, true)}
                    onDoubleClick={() => onActivate(info)}
                    info={info}
                    onMenu={onMenu}
                  />
                );
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
                    info={info}
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
  const flat = ["mysql", "sqlite", "clickhouse", "mongodb", "qdrant", "chroma", "milvus"].includes(conn.kind);
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
    // A reconnect it waits on shows in the toast and the status bar (Cancel, Retry); the tree shows only Loading…
    waitOn(target, (callId) => meta.objects(target, schema, callId), () => loadObjects(schema, true)).then(
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
          setOpen((s) => new Set(s).add(`s:${auto}`).add(`s:${auto}/g:tables`));
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
    waitOn(target, (callId) => meta.table(target, schema, table, callId), () => loadDetail(schema, table, true)).then(
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
              const isRel = r.kind !== "sequence";
              const info: NodeInfo = {
                kind: r.kind,
                conn,
                db,
                schema: s.name,
                name: r.name,
                refresh: () => (isRel ? loadDetail(s.name, r.name, true) : loadObjects(s.name, true)),
                refreshParent: () => loadObjects(s.name, true),
              };
              const Icon = objectIcon(r.kind) ?? TableIcon;
              return (
                <div key={tk}>
                  <Row
                    depth={depth}
                    open={tOpen}
                    leaf={!isRel}
                    icon={<Icon />}
                    label={r.name}
                    title={r.comment ? `${r.name} — ${r.comment}` : r.name}
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
                              {gOpen && g.items.map((r) => item(r, 4 + dd))}
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
