import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { ask, askText } from "../lib/dialogs";
import { useVirtualizer } from "@tanstack/react-virtual";
import { Panel, PanelGroup, PanelResizeHandle } from "react-resizable-panels";
import { api, history, QueryResult, redis, RedisDbInfo, RedisKeyDetail, RedisKeyRow, RedisOverview, RedisReply } from "../lib/api";
import { keyTree, parentPaths, visibleRows } from "../lib/keyTree";
import { sharedOr, useSharedReady, type RedisAssist, type RedisReplyTable } from "../lib/shared";
import { isRead } from "../lib/stmtKind";
import { FolderIcon } from "../ui/icons";
import { Badge, Button, EmptyState, FormGrid, IconButton, Modal, Named, Notice, Select, Spacer, Table, Tabs, Td, Text, TextArea, TextInput, Th, ToolButton, Toolbar, TreeRow } from "../ui";

const isMacOS = navigator.platform.toLowerCase().includes("mac");

/** Key list row height (same as App.css .rd-key) · max terminal output lines */
const KEY_ROW_H = 22;
const TERM_MAX = 1000;

/** Medis-style Redis browser, one tab per data source: the database picker and key filter on top, the key list on the left
 *  (pattern·type filter, SCAN load more), Content / Terminal / Info / Config on the right */
interface Props {
  target: string;
  connId: string;
  connName: string;
  database: string;
  readOnly: boolean;
  /** Production (confirm changes): the Terminal asks before a write */
  production: boolean;
  initialKey?: string;
  onSwitchDb: (db: string) => void;
  onStatus: (s: string) => void;
}

/** Why the Config tab is greyed out (its tooltip), and what Config says if CONFIG fails anyway */
const CONFIG_REFUSED =
  "This server doesn't allow CONFIG — managed Redis services (AWS ElastiCache / MemoryDB, Azure Cache, GCP Memorystore, Upstash …) disable or rename it. Its settings are in the provider's console (e.g. the ElastiCache parameter group). Server details are in Info.";

/** "db0 · 500 keys" (qurry_shared::redis_cmd::db_label, as the TUI); the name alone until the module loads */
const dbLabel = (d: RedisDbInfo) => sharedOr(d.name, (s) => s.redisDbLabel(d.name, d.keys));

const TYPES = ["string", "hash", "list", "set", "zset", "stream"];
const TYPE_COLOR: Record<string, string> = {
  string: "#5f9ea0",
  hash: "#b07cc6",
  list: "#6a9fd8",
  set: "#d08b4c",
  zset: "#c9605c",
  stream: "#5aa469",
};

function TypeBadge({ type }: { type: string }) {
  return (
    <Badge color={TYPE_COLOR[type] ?? "#777"} className="rd-badge">
      {type === "zset" ? "ZSET" : type.toUpperCase()}
    </Badge>
  );
}

/** A key's TTL as the key list shows it ("1d 4h"), — without expiry (qurry_shared::redis_cmd::ttl_label, as the TUI) */
const fmtTtl = (ms: number) => sharedOr("", (s) => s.redisTtl(ms));
const fmtBytes = (n: number | null) => (n == null ? "" : n < 1024 ? `${n} B` : n < 1048576 ? `${(n / 1024).toFixed(1)} KB` : `${(n / 1048576).toFixed(1)} MB`);

/** Same output as redis-cli */
export function formatReply(v: RedisReply, indent = ""): string {
  if (v === null) return "(nil)";
  if (typeof v === "number") return `(integer) ${v}`;
  if (typeof v === "boolean") return v ? "(true)" : "(false)";
  if (typeof v === "string") return JSON.stringify(v);
  if (Array.isArray(v)) {
    if (!v.length) return "(empty array)";
    const w = String(v.length).length;
    return v
      .map((x, i) => {
        const head = `${String(i + 1).padStart(w)}) `;
        const body = formatReply(x, indent + " ".repeat(head.length));
        return (i === 0 ? "" : indent) + head + body;
      })
      .join("\n");
  }
  return v.status;
}

export function RedisBrowser(p: Props) {
  const [pattern, setPattern] = useState("");
  const [typeFilter, setTypeFilter] = useState("");
  const [keys, setKeys] = useState<RedisKeyRow[]>([]);
  const [cursor, setCursor] = useState("0");
  const [scanning, setScanning] = useState(false);
  const [sel, setSel] = useState<string | null>(p.initialKey ?? null);
  const [tab, setTab] = useState<"content" | "terminal" | "info" | "config">("content");
  const [adding, setAdding] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Keys appear as folders grouped by ":" (Medis). open = expanded folders, cur = selected folder (null when a key is selected)
  const [open, setOpen] = useState<Set<string>>(new Set());
  const [cur, setCur] = useState<string | null>(null);
  const wasmReady = useSharedReady();
  const rows = useMemo(() => visibleRows(keyTree(keys, (k) => k.key), open), [keys, open, wasmReady]);
  const toggle = (path: string) =>
    setOpen((o) => {
      const n = new Set(o);
      if (n.has(path)) n.delete(path);
      else n.add(path);
      return n;
    });
  // Expand the parent folders so the selected key (first opened, newly created) is visible
  useEffect(() => {
    if (!sel) return;
    setOpen((o) => {
      const add = parentPaths(sel).filter((x) => !o.has(x));
      return add.length ? new Set([...o, ...add]) : o;
    });
  }, [sel]);

  const listRef = useRef<HTMLDivElement>(null);
  const keyVirt = useVirtualizer({ count: rows.length, getScrollElement: () => listRef.current, estimateSize: () => KEY_ROW_H, overscan: 20 });

  // The database picker's key counts and whether CONFIG works (checked on connect), read again with each new scan
  const [overview, setOverview] = useState<RedisOverview | null>(null);
  const loadOverview = () => redis.overview(p.target).then(setOverview, () => {});
  const configOff = overview?.configAllowed === false;
  useEffect(() => {
    if (configOff && tab === "config") setTab("info");
  }, [configOff]);

  // A scan of the database shown before that answers late is dropped (shownTarget: the database shown now)
  const scan = async (reset: boolean) => {
    const asked = p.target;
    setScanning(true);
    setError(null);
    if (reset) loadOverview();
    try {
      const page = await redis.scan(asked, reset ? "0" : cursor, pattern, 500, typeFilter);
      if (asked !== shownTarget.current) return;
      setKeys((ks) => {
        // Merge only the new (sorted) page into the already sorted list (instead of re-sorting everything each time)
        const have = new Set(reset ? [] : ks.map((k) => k.key));
        const add = page.keys.filter((k) => (have.has(k.key) ? false : (have.add(k.key), true))).sort((a, b) => a.key.localeCompare(b.key));
        const base = reset ? [] : ks;
        const out: RedisKeyRow[] = [];
        let i = 0, j = 0;
        while (i < base.length || j < add.length) out.push(j >= add.length || (i < base.length && base[i].key.localeCompare(add[j].key) <= 0) ? base[i++] : add[j++]);
        return out;
      });
      setCursor(page.cursor);
    } catch (e) {
      if (asked === shownTarget.current) setError(String(e));
    } finally {
      if (asked === shownTarget.current) setScanning(false);
    }
  };
  useEffect(() => {
    scan(true);
  }, [p.target, typeFilter]);
  // Another database: its own keys (the key and folders picked in the last one aren't there)
  const shownTarget = useRef(p.target);
  useEffect(() => {
    if (shownTarget.current === p.target) return;
    shownTarget.current = p.target;
    setSel(p.initialKey ?? null);
    setCur(null);
    setOpen(new Set());
  }, [p.target]);
  useEffect(() => {
    if (p.initialKey) setSel(p.initialKey);
  }, [p.initialKey]);

  const exec = async (args: string[]) => {
    if (p.readOnly) throw new Error("Data source is read-only");
    return redis.exec(p.target, args);
  };
  const removeKeyLocal = (k: string) => setKeys((ks) => ks.filter((x) => x.key !== k));
  const renameLocal = (from: string, to: string) => setKeys((ks) => ks.map((x) => (x.key === from ? { ...x, key: to } : x)).sort((a, b) => a.key.localeCompare(b.key)));

  return (
    <div
      className="redis-browser"
      onKeyDown={(e) => {
        // ⌘r = reload (same as the toolbar's Scan again)
        if ((isMacOS ? e.metaKey : e.ctrlKey) && !e.shiftKey && !e.altKey && e.code === "KeyR") {
          e.preventDefault();
          scan(true);
        }
      }}
    >
      <Toolbar className="rd-toolbar">
        <Select className="rd-db" value={p.database} onChange={(e) => p.onSwitchDb(e.target.value)} title="Database (the explorer's databases switch it too)" aria-label="Database">
          {(overview?.databases.some((d) => d.name === p.database) ? overview.databases : [{ name: p.database, keys: -1 }, ...(overview?.databases ?? [])]).map((d) => (
            <option key={d.name} value={d.name}>
              {dbLabel(d)}
            </option>
          ))}
        </Select>
        <Named about="key patterns" info={["SCAN MATCH pattern: * any characters, ? one, [ab] a or b.", "Enter applies it."]}>
          <TextInput
            className="rd-pattern"
            value={pattern}
            spellCheck={false}
            aria-label="Filter keys"
            placeholder="Filter keys (pattern, e.g. prod:user:*)"
            onChange={(e) => setPattern(e.target.value)}
            onKeyDown={(e) => {
              e.stopPropagation();
              if (e.key === "Enter") scan(true);
            }}
          />
        </Named>
        <Select value={typeFilter} onChange={(e) => setTypeFilter(e.target.value)} title="Filter by type">
          <option value="">All types</option>
          {TYPES.map((t) => (
            <option key={t} value={t}>
              {t}
            </option>
          ))}
        </Select>
        <IconButton title="Scan again (⌘r)" onClick={() => scan(true)}>
          ⟳
        </IconButton>
        <ToolButton title="Add a new key" onClick={() => setAdding(true)} disabled={p.readOnly}>
          + Key
        </ToolButton>
      </Toolbar>
      <PanelGroup direction="horizontal" autoSaveId="dbtool-redis">
        <Panel defaultSize={34} minSize={18}>
          <div className="rd-keys">
            <div className="rd-keys-head">
              <span>Key</span>
              <Spacer />
              <span>TTL</span>
            </div>
            <div className="rd-keys-list" ref={listRef} tabIndex={0} onKeyDown={(e) => {
              const i = rows.findIndex((r) => (cur ? r.kind === "folder" && r.path === cur : r.kind === "key" && r.item.key === sel));
              const r = rows[i];
              const go = (j: number) => {
                e.preventDefault();
                const t = rows[j];
                if (t.kind === "folder") setCur(t.path);
                else {
                  setCur(null);
                  setSel(t.item.key);
                }
                keyVirt.scrollToIndex(j);
              };
              if (e.key === "ArrowDown" && i < rows.length - 1) go(i + 1);
              if (e.key === "ArrowUp" && i > 0) go(i - 1);
              // Folders: → expand · ← collapse · Enter toggle. Otherwise ← goes to the parent folder
              if (r?.kind === "folder" && (e.key === "Enter" || (e.key === "ArrowRight" && !open.has(r.path)) || (e.key === "ArrowLeft" && open.has(r.path)))) {
                e.preventDefault();
                toggle(r.path);
              } else if (r && e.key === "ArrowLeft" && r.depth > 0) {
                for (let j = i - 1; j >= 0; j--) if (rows[j].kind === "folder" && rows[j].depth === r.depth - 1) return go(j);
              }
            }}>
              {/* Render only visible rows (light even with tens of thousands of keys) */}
              <div style={{ height: keyVirt.getTotalSize(), position: "relative" }}>
                {keyVirt.getVirtualItems().map((vi) => {
                  const r = rows[vi.index];
                  const style = { position: "absolute" as const, top: 0, left: 0, right: 0, transform: `translateY(${vi.start}px)` };
                  if (r.kind === "folder")
                    return (
                      <TreeRow
                        key={"f:" + r.path}
                        depth={r.depth}
                        open={open.has(r.path)}
                        icon={<FolderIcon />}
                        label={r.name}
                        sub={r.count.toLocaleString("en-US")}
                        selected={cur === r.path}
                        style={style}
                        title={`${r.path}:*`}
                        onSelect={() => setCur(r.path)}
                        onToggle={() => toggle(r.path)}
                        onClick={() => toggle(r.path)}
                        onDoubleClick={() => {}}
                      />
                    );
                  const k = r.item;
                  return (
                    <TreeRow
                      key={k.key}
                      depth={r.depth}
                      leaf
                      icon={<TypeBadge type={k.type} />}
                      iconClass="rd-type"
                      label={k.key}
                      trailing={<span className={k.ttl < 0 ? "rd-ttl none" : "rd-ttl"}>{fmtTtl(k.ttl)}</span>}
                      selected={!cur && k.key === sel}
                      style={style}
                      title={k.key}
                      onClick={() => {
                        setCur(null);
                        setSel(k.key);
                        setTab("content");
                      }}
                    />
                  );
                })}
              </div>
              {!keys.length && !scanning && <EmptyState>{error ?? "No keys match"}</EmptyState>}
            </div>
            <div className="rd-keys-foot">
              <span>
                {keys.length.toLocaleString()} key{keys.length === 1 ? "" : "s"}
                {cursor !== "0" ? "+" : ""}
              </span>
              <Spacer />
              {scanning ? <Text tone="muted">Scanning…</Text> : cursor !== "0" && <Button variant="link" onClick={() => scan(false)}>Scan more</Button>}
            </div>
          </div>
        </Panel>
        <PanelResizeHandle className="resize-h" />
        <Panel minSize={30}>
          <div className="rd-right">
            <Tabs
              size="sm"
              items={(["content", "terminal", "info", "config"] as const).map((t) => ({
                id: t,
                label: t[0].toUpperCase() + t.slice(1),
                ...(t === "config" && configOff ? { disabled: true, title: CONFIG_REFUSED } : {}),
              }))}
              value={tab}
              onChange={setTab}
            />
            <div className="rd-tab-body">
              {tab === "content" &&
                (sel ? (
                  <KeyContent
                    key={sel}
                    target={p.target}
                    keyName={sel}
                    readOnly={p.readOnly}
                    exec={exec}
                    onStatus={p.onStatus}
                    onDeleted={() => {
                      removeKeyLocal(sel);
                      setSel(null);
                    }}
                    onRenamed={(to) => {
                      renameLocal(sel, to);
                      setSel(to);
                    }}
                    onTtl={(ttl) => setKeys((ks) => ks.map((x) => (x.key === sel ? { ...x, ttl } : x)))}
                  />
                ) : (
                  <EmptyState>Select a key on the left.</EmptyState>
                ))}
              {tab === "terminal" && (
                <Terminal
                  target={p.target}
                  connId={p.connId}
                  connName={p.connName}
                  database={p.database}
                  readOnly={p.readOnly}
                  production={p.production}
                  prompt={`${p.database}>`}
                  onChanged={() => scan(true)}
                />
              )}
              {tab === "info" && <ServerTable target={p.target} command="INFO" />}
              {tab === "config" && <ServerTable target={p.target} command="CONFIG GET *" />}
            </div>
          </div>
        </Panel>
      </PanelGroup>
      {adding && (
        <NewKeyDialog
          onClose={() => setAdding(false)}
          onCreate={async (name, type, a, b) => {
            const args: Record<string, string[]> = {
              string: ["SET", name, a],
              hash: ["HSET", name, a, b],
              list: ["RPUSH", name, a],
              set: ["SADD", name, a],
              zset: ["ZADD", name, b || "0", a],
              stream: ["XADD", name, "*", a || "field", b],
            };
            await exec(args[type]);
            setAdding(false);
            setKeys((ks) => [...ks.filter((k) => k.key !== name), { key: name, type, ttl: -1 }].sort((x, y) => x.key.localeCompare(y.key)));
            setSel(name);
            setTab("content");
          }}
        />
      )}
    </div>
  );
}

// ───────── Key content (by type) ─────────

type Row = { id: string; cols: string[]; raw: unknown };

function KeyContent({ target, keyName, readOnly, exec, onStatus, onDeleted, onRenamed, onTtl }: {
  target: string;
  keyName: string;
  readOnly: boolean;
  exec: (args: string[]) => Promise<RedisReply>;
  onStatus: (s: string) => void;
  onDeleted: () => void;
  onRenamed: (to: string) => void;
  onTtl: (ttl: number) => void;
}) {
  const [d, setD] = useState<RedisKeyDetail | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [name, setName] = useState(keyName);
  const [ttl, setTtl] = useState("");
  const [limit, setLimit] = useState(1000);
  const [selRow, setSelRow] = useState<number | null>(null);
  const [text, setText] = useState("");
  const [fmt, setFmt] = useState<"raw" | "json">("raw");
  const [newItem, setNewItem] = useState<{ a: string; b: string } | null>(null);

  const load = async (lim = limit) => {
    setErr(null);
    try {
      const r = await redis.key(target, keyName, lim);
      setD(r);
      setTtl(r.ttl >= 0 ? String(Math.round(r.ttl / 1000)) : "");
      if (r.type === "string") setText(String(r.value ?? ""));
    } catch (e) {
      setErr(String(e));
    }
  };
  useEffect(() => {
    load();
  }, [keyName]);

  const run = async (args: string[], msg?: string) => {
    try {
      await exec(args);
      if (msg) onStatus(msg);
      await load();
      return true;
    } catch (e) {
      onStatus(String(e));
      setErr(String(e));
      return false;
    }
  };

  const rows: Row[] = useMemo(() => {
    if (!d || !Array.isArray(d.value)) return [];
    const v = d.value as unknown[];
    switch (d.type) {
      case "hash":
        return v.map((x, i) => ({ id: String(i), cols: [String((x as string[])[0]), String((x as string[])[1])], raw: x }));
      case "list":
        return v.map((x, i) => ({ id: String(i), cols: [String(i), String(x)], raw: x }));
      case "set":
        return v.map((x, i) => ({ id: String(i), cols: [String(x)], raw: x }));
      case "zset":
        return v.map((x, i) => ({ id: String(i), cols: [String((x as string[])[0]), String((x as string[])[1])], raw: x }));
      case "stream":
        return v.map((x, i) => ({ id: String(i), cols: [String((x as unknown[])[0]), JSON.stringify((x as unknown[])[1])], raw: x }));
    }
    return [];
  }, [d]);
  const heads: Record<string, string[]> = { hash: ["field", "value"], list: ["index", "value"], set: ["member"], zset: ["member", "score"], stream: ["id", "fields"] };
  // Value shown in the editor below (selected element)
  const valueCol = d?.type === "set" || d?.type === "zset" ? 0 : 1;
  useEffect(() => {
    if (d && d.type !== "string" && selRow !== null && rows[selRow]) setText(rows[selRow].cols[valueCol]);
  }, [selRow, rows]);

  const pretty = (s: string) => {
    if (fmt !== "json") return s;
    try {
      return JSON.stringify(JSON.parse(s), null, 2);
    } catch {
      return s;
    }
  };
  const saveText = async (input: string) => {
    if (!d) return;
    setText(input);
    let v = input;
    if (fmt === "json") {
      try {
        v = JSON.stringify(JSON.parse(text));
      } catch {
        return onStatus("Invalid JSON");
      }
    }
    if (d.type === "string") {
      // Keep TTL
      await run(d.ttl > 0 ? ["SET", keyName, v, "KEEPTTL"] : ["SET", keyName, v], "Saved");
      return;
    }
    if (selRow === null) return;
    const r = rows[selRow];
    if (d.type === "hash") await run(["HSET", keyName, r.cols[0], v], `Saved field ${r.cols[0]}`);
    else if (d.type === "list") await run(["LSET", keyName, r.cols[0], v], `Saved index ${r.cols[0]}`);
    else if (d.type === "set") {
      if (v !== r.cols[0]) await run(["SREM", keyName, r.cols[0]]) && (await run(["SADD", keyName, v], "Member replaced"));
    } else if (d.type === "zset") {
      if (v !== r.cols[0]) {
        await exec(["ZREM", keyName, r.cols[0]]).catch(() => {});
        await run(["ZADD", keyName, r.cols[1], v], "Member replaced");
      }
    }
  };
  const deleteRow = async (i: number) => {
    if (!d) return;
    const r = rows[i];
    if (d.type === "hash") await run(["HDEL", keyName, r.cols[0]], `Deleted field ${r.cols[0]}`);
    else if (d.type === "set") await run(["SREM", keyName, r.cols[0]], "Member removed");
    else if (d.type === "zset") await run(["ZREM", keyName, r.cols[0]], "Member removed");
    else if (d.type === "stream") await run(["XDEL", keyName, r.cols[0]], "Entry deleted");
    else if (d.type === "list") {
      // Delete by index: replace with a marker, then LREM
      const mark = `__dbtool_deleted_${crypto.randomUUID()}`;
      (await run(["LSET", keyName, r.cols[0], mark])) && (await run(["LREM", keyName, "1", mark], `Removed index ${r.cols[0]}`));
    }
    setSelRow(null);
  };
  const editScore = async (i: number) => {
    const r = rows[i];
    const s = (await askText(`Score of ${r.cols[0]}`, r.cols[1]));
    if (s != null && s.trim() && !Number.isNaN(Number(s))) await run(["ZADD", keyName, s.trim(), r.cols[0]], "Score updated");
  };
  const addItem = async () => {
    if (!d || !newItem) return;
    const { a, b } = newItem;
    const ok =
      d.type === "hash" ? await run(["HSET", keyName, a, b])
      : d.type === "list" ? await run([b === "head" ? "LPUSH" : "RPUSH", keyName, a])
      : d.type === "set" ? await run(["SADD", keyName, a])
      : d.type === "zset" ? await run(["ZADD", keyName, b || "0", a])
      : d.type === "stream" ? await run(["XADD", keyName, "*", a || "field", b])
      : false;
    if (ok) setNewItem(null);
  };

  if (err && !d) return <Notice tone="danger">{err}</Notice>;
  if (!d) return <EmptyState>Loading {keyName}…</EmptyState>;

  return (
    <div className="rd-content">
      <div className="rd-keybar">
        <TypeBadge type={d.type} />
        <TextInput
          className="rd-keyname"
          value={name}
          disabled={readOnly}
          spellCheck={false}
          title="Enter to rename"
          onChange={(e) => setName(e.target.value)}
          onKeyDown={async (e) => {
            e.stopPropagation();
            if (e.key === "Enter" && name && name !== keyName) {
              try {
                await exec(["RENAME", keyName, name]);
                onStatus(`Renamed to ${name}`);
                onRenamed(name);
              } catch (er) {
                onStatus(String(er));
              }
            }
            if (e.key === "Escape") setName(keyName);
          }}
        />
        <label className="rd-ttl-edit" title="TTL in seconds. Empty = no expiry. Enter to apply">
          TTL
          <TextInput
            value={ttl}
            placeholder="∞"
            disabled={readOnly}
            onChange={(e) => setTtl(e.target.value.replace(/[^0-9]/g, ""))}
            onKeyDown={async (e) => {
              e.stopPropagation();
              if (e.key !== "Enter") return;
              const ok = ttl ? await run(["EXPIRE", keyName, ttl], `TTL set to ${ttl}s`) : await run(["PERSIST", keyName], "Expiry removed");
              if (ok) onTtl(ttl ? Number(ttl) * 1000 : -1);
            }}
          />
        </label>
        <Spacer />
        <Text tone="muted" className="rd-meta">
          {d.type === "string" ? `${d.length.toLocaleString()} bytes` : `${d.length.toLocaleString()} items`}
          {d.encoding ? ` · ${d.encoding}` : ""}
          {d.memory != null ? ` · ${fmtBytes(d.memory)}` : ""}
        </Text>
        <IconButton title="Reload" onClick={() => load()}>
          ⟳
        </IconButton>
        <IconButton tone="danger"
          title="Delete key"
          disabled={readOnly}
          onClick={async () => {
            if (!(await ask(`Delete key "${keyName}"?`, { okLabel: "Delete", danger: true }))) return;
            try {
              await exec(["DEL", keyName]);
              onStatus(`Deleted ${keyName}`);
              onDeleted();
            } catch (e) {
              onStatus(String(e));
            }
          }}
        >
          🗑
        </IconButton>
      </div>
      {err && <Notice tone="danger">{err}</Notice>}
      {d.type === "string" ? (
        <ValueEditor text={text} fmt={fmt} setFmt={setFmt} pretty={pretty} readOnly={readOnly} onSave={saveText} big />
      ) : (
        <PanelGroup direction="vertical" autoSaveId="dbtool-redis-content">
          <Panel defaultSize={62} minSize={20}>
            <div className="rd-table-wrap">
              <Table className="rd-table">
                <thead>
                  <tr>
                    {heads[d.type]?.map((h) => (
                      <Th key={h}>{h}</Th>
                    ))}
                    <Th className="act" />
                  </tr>
                </thead>
                <tbody>
                  {rows.map((r, i) => (
                    <tr key={r.id} className={selRow === i ? "sel" : ""} onClick={() => setSelRow(i)} onDoubleClick={() => d.type === "zset" && !readOnly && editScore(i)}>
                      {r.cols.map((c, j) => (
                        <Td key={j}>{c.length > 200 ? c.slice(0, 200) + "…" : c}</Td>
                      ))}
                      <Td className="act">
                        {!readOnly && (
                          <span className="rd-del" title="Delete" onClick={(e) => (e.stopPropagation(), deleteRow(i))}>
                            ×
                          </span>
                        )}
                      </Td>
                    </tr>
                  ))}
                </tbody>
              </Table>
              {d.truncated && (
                <div className="rd-more">
                  Showing {rows.length.toLocaleString()} of {d.length.toLocaleString()}{" "}
                  <Button variant="link" onClick={() => { setLimit(limit * 5); load(limit * 5); }}>Load more</Button>
                </div>
              )}
              {!readOnly &&
                (newItem ? (
                  <div className="rd-add">
                    <TextInput autoFocus placeholder={d.type === "hash" ? "field" : d.type === "stream" ? "field" : d.type === "zset" ? "member" : "value"} value={newItem.a} onChange={(e) => setNewItem({ ...newItem, a: e.target.value })} onKeyDown={(e) => (e.stopPropagation(), e.key === "Enter" && addItem())} />
                    {d.type === "list" ? (
                      <Select value={newItem.b || "tail"} onChange={(e) => setNewItem({ ...newItem, b: e.target.value })}>
                        <option value="tail">Append (RPUSH)</option>
                        <option value="head">Prepend (LPUSH)</option>
                      </Select>
                    ) : d.type !== "set" ? (
                      <TextInput placeholder={d.type === "zset" ? "score" : "value"} value={newItem.b} onChange={(e) => setNewItem({ ...newItem, b: e.target.value })} onKeyDown={(e) => (e.stopPropagation(), e.key === "Enter" && addItem())} />
                    ) : null}
                    <Button variant="primary" onClick={addItem}>Add</Button>
                    <Button onClick={() => setNewItem(null)}>Cancel</Button>
                  </div>
                ) : (
                  <Button variant="link" className="rd-add-btn" onClick={() => setNewItem({ a: "", b: "" })}>
                    + Add {d.type === "hash" ? "field" : d.type === "stream" ? "entry" : d.type === "list" ? "element" : "member"}
                  </Button>
                ))}
            </div>
          </Panel>
          <PanelResizeHandle className="resize-v" />
          <Panel minSize={15}>
            {selRow !== null && rows[selRow] ? (
              <ValueEditor
                text={text}
                fmt={fmt}
                setFmt={setFmt}
                pretty={pretty}
                readOnly={readOnly || d.type === "stream"}
                onSave={saveText}
                label={d.type === "hash" ? `Field: ${rows[selRow].cols[0]}` : d.type === "list" ? `Index ${rows[selRow].cols[0]}` : d.type === "zset" ? `Score ${rows[selRow].cols[1]}` : d.type === "stream" ? rows[selRow].cols[0] : "Member"}
              />
            ) : (
              <EmptyState>Select an element to view or edit its value.</EmptyState>
            )}
          </Panel>
        </PanelGroup>
      )}
    </div>
  );
}

function ValueEditor({ text, fmt, setFmt, pretty, readOnly, onSave, label, big }: {
  text: string; fmt: "raw" | "json"; setFmt: (f: "raw" | "json") => void;
  pretty: (s: string) => string; readOnly: boolean; onSave: (v: string) => void; label?: string; big?: boolean;
}) {
  const shown = useRef<string>("");
  const [buf, setBuf] = useState(pretty(text));
  useEffect(() => {
    const v = pretty(text);
    shown.current = v;
    setBuf(v);
  }, [text, fmt]);
  const isJson = useMemo(() => {
    try {
      const v = JSON.parse(text);
      return typeof v === "object" && v !== null;
    } catch {
      return false;
    }
  }, [text]);
  return (
    <div className={"rd-editor" + (big ? " big" : "")}>
      <Toolbar className="rd-editor-bar">
        {label && <Text tone="muted">{label}</Text>}
        <Spacer />
        <Select value={fmt} onChange={(e) => setFmt(e.target.value as "raw" | "json")} title="View as">
          <option value="raw">Plain Text</option>
          <option value="json">JSON{isJson ? "" : " (invalid)"}</option>
        </Select>
        {!readOnly && (
          <Button variant="primary" onClick={() => onSave(buf)} disabled={buf === shown.current} title="Save (⌘s)">
            Save
          </Button>
        )}
      </Toolbar>
      <TextArea
        value={buf}
        readOnly={readOnly}
        spellCheck={false}
        onChange={(e) => setBuf(e.target.value)}
        onKeyDown={(e) => {
          e.stopPropagation();
          if ((e.metaKey || e.ctrlKey) && e.key === "s") {
            e.preventDefault();
            onSave(buf);
          }
        }}
      />
    </div>
  );
}

// ───────── Terminal (redis-cli) ─────────

type TermLine = { kind: "in" | "out" | "err" | "note" | "nil"; text: string } | { kind: "table"; table: RedisReplyTable };
type Choice = { insert: string; label: string; args?: string; summary?: string; write?: boolean };

/** How many key names the completion asks a bounded SCAN for */
const KEY_CHOICES = 50;

/**
 * redis-cli in a tab: command and key-name completion with argument hints (the bundled command table, qurry_shared through WASM),
 * ↑↓ history per data source (Query History, passwords masked), HELP <command>, replies shown by type. Same rules as the console:
 * a read-only source runs reads only, Production asks before a write, FLUSHALL / FLUSHDB / DEL … always ask
 */
function Terminal({ target, connId, connName, database, readOnly, production, prompt, onChanged }: {
  target: string; connId: string; connName: string; database: string; readOnly: boolean; production: boolean; prompt: string; onChanged: () => void;
}) {
  const [lines, setLines] = useState<TermLine[]>([]);
  const [input, setInput] = useState("");
  const [caret, setCaret] = useState(0);
  const [hist, setHist] = useState<string[]>([]);
  const hi = useRef(-1);
  const [keys, setKeys] = useState<{ prefix: string; names: string[] } | null>(null);
  const [pick, setPick] = useState(0);
  const [closed, setClosed] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);
  const end = useRef<HTMLDivElement>(null);
  const ready = useSharedReady();
  useEffect(() => {
    end.current?.scrollIntoView({ block: "end" });
  }, [lines]);
  const loadHist = () => history.list(connId, "", 200).then((h) => setHist(h.map((x) => x.sql)), () => {});
  useEffect(() => {
    loadHist();
  }, [connId]);

  // What the word before the caret can become: commands (with their arguments) or key names (from a bounded SCAN)
  const assist = useMemo(() => sharedOr<RedisAssist | null>(null, (s) => s.redisAssist(input.slice(0, caret))), [input, caret, ready]);
  useEffect(() => {
    if (!assist?.key) return setKeys(null);
    const prefix = assist.prefix;
    const t = setTimeout(() => redis.keyNames(target, prefix, KEY_CHOICES).then((names) => setKeys({ prefix, names }), () => setKeys(null)), 120);
    return () => clearTimeout(t);
  }, [assist?.key, assist?.prefix, target]);
  const choices: Choice[] =
    closed || !assist
      ? []
      : assist.commands.length
        ? assist.commands.map((c) => ({ insert: c.insert, label: c.doc.name, args: c.doc.args, summary: c.doc.summary, write: !c.read }))
        : assist.key && keys?.prefix === assist.prefix
          ? sharedOr<Choice[]>([], (s) => s.redisKeyChoices(assist.prefix, keys.names))
          : [];
  // Key names: ↩ picks one only after ↑↓ moved to it, else it runs the line (⇥ always picks), so a key typed out in full
  // (`GET user:1`) isn't swapped for a longer one the SCAN found (`user:10`)
  const [moved, setMoved] = useState(false);
  const enterPicks = !!assist?.commands.length || moved;
  const sel = Math.min(pick, Math.max(choices.length - 1, 0));
  // The arguments still to type, after the text (a highlighted command shows its own)
  const hint = choices.length && choices[sel].args !== undefined ? choices[sel].args : (assist?.hint ?? "");
  const edit = (v: string, at = v.length) => {
    setInput(v);
    setCaret(at);
    setPick(0);
    setMoved(false);
    setClosed(false);
  };
  const choose = (c: Choice) => {
    if (!assist) return;
    const head = input.slice(0, caret - assist.prefix.length) + c.insert + " ";
    edit(head + input.slice(caret).replace(/^\S*\s?/, ""), head.length);
    placeCaret.current = head.length;
  };
  // After a pick the caret goes after the inserted word, before the next key is typed
  const placeCaret = useRef<number | null>(null);
  useLayoutEffect(() => {
    if (placeCaret.current === null) return;
    inputRef.current?.setSelectionRange(placeCaret.current, placeCaret.current);
    placeCaret.current = null;
  }, [input]);

  // Keep only the last TERM_MAX lines of output
  const push = (...xs: TermLine[]) => setLines((l) => [...l, ...xs].slice(-TERM_MAX));
  const submit = async () => {
    const line = input.trim();
    if (!line) return;
    hi.current = -1;
    edit("");
    if (line.toLowerCase() === "clear") return setLines([]);
    const echo: TermLine = { kind: "in", text: `${prompt} ${line}` };
    // This source's history (the core masks AUTH / HELLO … AUTH / requirepass), also for a HELP or a refused line; not for a cancelled one
    const remember = () => history.add(connId, database, line).then(loadHist, () => {});
    const help = sharedOr<string | undefined>(undefined, (s) => s.redisHelp(line));
    if (help) return remember(), push(echo, { kind: "note", text: help });
    const write = !isRead("redis", line);
    // Read-only runs reads only (the backend refuses the rest too; said here, nothing is asked first)
    if (readOnly && write)
      return (
        remember(),
        push(echo, { kind: "err", text: `(error) "${connName}" is read-only: ${line.split(/\s+/)[0].toUpperCase()} changes data. A read-only data source runs read commands only (Data Source Properties ▸ Read-only).` })
      );
    // FLUSHALL · FLUSHDB · DEL … always ask; on Production every write asks (one question for both)
    const why = await api.unsafeReason("redis", line).catch(() => null);
    if (why || (production && write)) {
      const ok = await ask(why ? `${why}. Run it anyway?` : "This command changes data. Run it?", {
        title: production ? `Production: ${connName}` : `${connName}: dangerous command`,
        detail: [{ text: line.slice(0, 300), value: database, tone: "danger" }],
        okLabel: why ? "Run Anyway" : "Run",
        danger: true,
      });
      inputRef.current?.focus();
      if (!ok) return;
    }
    remember();
    push(echo);
    try {
      const r = await redis.execLine(target, line);
      const table = sharedOr<RedisReplyTable | undefined>(undefined, (s) => s.redisReplyTable(line, r));
      push(table ? { kind: "table", table } : r === null ? { kind: "nil", text: "(nil)" } : { kind: "out", text: formatReply(r) });
      if (write) onChanged();
    } catch (e) {
      push({ kind: "err", text: `(error) ${String(e).replace(/^Redis: /, "")}` });
    }
  };
  return (
    <div className="rd-terminal" onClick={(e) => (e.target as HTMLElement).closest(".rd-term-table, pre") || inputRef.current?.focus()}>
      <div className="rd-term-out">
        {lines.map((l, i) =>
          l.kind === "table" ? (
            <table key={i} className="rd-term-table">
              <thead>
                <tr>
                  {l.table.heads.map((h) => (
                    <th key={h}>{h}</th>
                  ))}
                </tr>
              </thead>
              <tbody>
                {l.table.rows.map((r, j) => (
                  <tr key={j}>
                    {r.map((c, k) => (
                      <td key={k}>{c}</td>
                    ))}
                  </tr>
                ))}
              </tbody>
            </table>
          ) : (
            <pre key={i} className={l.kind}>
              {l.text}
            </pre>
          ),
        )}
        <div ref={end} />
      </div>
      <div className="rd-term-in">
        <Text tone="muted">{prompt}</Text>
        <div className="rd-term-field">
          {choices.length > 0 && (
            <div className="rd-term-pop" role="listbox" aria-label="Completions">
              {choices.slice(0, 200).map((c, i) => (
                <div
                  key={c.label}
                  role="option"
                  aria-selected={i === sel}
                  className={i === sel ? "rd-term-choice sel" : "rd-term-choice"}
                  onMouseDown={(e) => {
                    e.preventDefault();
                    choose(c);
                  }}
                >
                  <b>{c.label}</b>
                  {c.args && <span className="args">{c.args}</span>}
                  {c.summary && <span className="sum">{c.summary}</span>}
                  {c.write && (
                    <span className="rd-term-w" title="Changes data: a read-only data source refuses it, Production asks first">
                      write
                    </span>
                  )}
                </div>
              ))}
            </div>
          )}
          {/* The arguments still to type, right after the text (a hidden copy of the text pushes them there) */}
          {hint && caret === input.length && (
            <span className="rd-term-ghost" aria-hidden>
              <span className="pad">{input}</span>
              {input.endsWith(" ") || !input ? "" : " "}
              {hint}
            </span>
          )}
          <TextInput
            ref={inputRef}
            autoFocus
            value={input}
            spellCheck={false}
            aria-label="Redis command"
            onChange={(e) => edit(e.target.value, e.target.selectionStart ?? e.target.value.length)}
            onSelect={(e) => setCaret(e.currentTarget.selectionStart ?? input.length)}
            onBlur={() => setClosed(true)}
            onKeyDown={(e) => {
              e.stopPropagation();
              const open = choices.length > 0;
              if (open && (e.key === "ArrowDown" || e.key === "ArrowUp")) {
                e.preventDefault();
                setPick((sel + (e.key === "ArrowDown" ? 1 : choices.length - 1)) % choices.length);
                setMoved(true);
              } else if (open && (e.key === "Tab" || (e.key === "Enter" && enterPicks))) {
                e.preventDefault();
                choose(choices[sel]);
              } else if (open && e.key === "Escape") {
                e.preventDefault();
                setClosed(true);
              } else if (e.key === "Tab") {
                // ⇥ asks for the list again after ⎋ closed it
                e.preventDefault();
                setClosed(false);
              } else if (e.key === "Enter") {
                // No keypress after it: the confirm that may open would take it as a click on its focused button
                e.preventDefault();
                submit();
              } else if (e.key === "ArrowUp" && hist.length) {
                e.preventDefault();
                hi.current = Math.min(hi.current + 1, hist.length - 1);
                edit(hist[hi.current]);
                setClosed(true);
              } else if (e.key === "ArrowDown") {
                e.preventDefault();
                hi.current = Math.max(hi.current - 1, -1);
                edit(hi.current < 0 ? "" : hist[hi.current]);
                setClosed(true);
              } else if (e.key === "l" && e.ctrlKey) {
                e.preventDefault();
                setLines([]);
              }
            }}
          />
        </div>
        <span className="rd-term-keys">↑↓ history · ⇥ complete · HELP command</span>
      </div>
    </div>
  );
}

/** Info / Config: searchable table */
function ServerTable({ target, command }: { target: string; command: string }) {
  const [r, setR] = useState<QueryResult | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [q, setQ] = useState("");
  const load = () => {
    setErr(null);
    api.runQuery(target, command).then(setR, (e) => setErr(String(e)));
  };
  useEffect(() => {
    load();
  }, [target, command]);
  // Managed Redis (AWS ElastiCache · MemoryDB, Azure, GCP Memorystore, Upstash …) renames or blocks CONFIG
  if (err && /^CONFIG/i.test(command) && /unknown command|NOPERM|disabled|not allowed/i.test(err))
    return <Notice>{CONFIG_REFUSED}</Notice>;
  if (err) return <Notice tone="danger">{err}</Notice>;
  if (!r) return <EmptyState>Loading…</EmptyState>;
  const rows = q ? r.rows.filter((row) => row.some((c) => String(c ?? "").toLowerCase().includes(q.toLowerCase()))) : r.rows;
  return (
    <div className="rd-server">
      <Toolbar className="rd-editor-bar">
        <TextInput placeholder="Filter" value={q} onChange={(e) => setQ(e.target.value)} onKeyDown={(e) => e.stopPropagation()} />
        <Spacer />
        <Text tone="muted">{rows.length} entries</Text>
        <IconButton onClick={load} title="Reload">
          ⟳
        </IconButton>
      </Toolbar>
      <div className="rd-table-wrap">
        <Table className="rd-table">
          <thead>
            <tr>
              {r.columns.map((c) => (
                <Th key={c.name}>{c.name}</Th>
              ))}
            </tr>
          </thead>
          <tbody>
            {rows.map((row, i) => (
              <tr key={i}>
                {row.map((c, j) => (
                  <Td key={j}>{String(c ?? "")}</Td>
                ))}
              </tr>
            ))}
          </tbody>
        </Table>
      </div>
    </div>
  );
}

function NewKeyDialog({ onClose, onCreate }: { onClose: () => void; onCreate: (name: string, type: string, a: string, b: string) => Promise<void> }) {
  const [name, setName] = useState("");
  const [type, setType] = useState("string");
  const [a, setA] = useState("");
  const [b, setB] = useState("");
  const [err, setErr] = useState<string | null>(null);
  const labels: Record<string, [string, string?]> = {
    string: ["Value"],
    hash: ["Field", "Value"],
    list: ["Element"],
    set: ["Member"],
    zset: ["Member", "Score"],
    stream: ["Field", "Value"],
  };
  const [la, lb] = labels[type];
  const go = async () => {
    if (!name.trim()) return setErr("Key name is required");
    try {
      await onCreate(name.trim(), type, a, b);
    } catch (e) {
      setErr(String(e));
    }
  };
  return (
    <Modal
      title="Add Key"
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose}>Cancel</Button>
          <Button variant="primary" onClick={go}>
            Add
          </Button>
        </>
      }
    >
      <FormGrid labelWidth={90}>
        <label>Key</label>
        <TextInput autoFocus value={name} onChange={(e) => setName(e.target.value)} placeholder="e.g. user:42" />
        <label>Type</label>
        <Select value={type} onChange={(e) => setType(e.target.value)}>
          {TYPES.map((t) => (
            <option key={t} value={t}>
              {t}
            </option>
          ))}
        </Select>
        <label>{la}</label>
        {type === "string" ? <TextArea rows={4} value={a} onChange={(e) => setA(e.target.value)} /> : <TextInput value={a} onChange={(e) => setA(e.target.value)} />}
        {lb && (
          <>
            <label>{lb}</label>
            <TextInput value={b} onChange={(e) => setB(e.target.value)} placeholder={type === "zset" ? "0" : ""} />
          </>
        )}
      </FormGrid>
      {err && <Notice tone="danger">{err}</Notice>}
    </Modal>
  );
}
