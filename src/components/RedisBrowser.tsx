import { useEffect, useMemo, useRef, useState } from "react";
import { ask, askText } from "../lib/dialogs";
import { useVirtualizer } from "@tanstack/react-virtual";
import { Panel, PanelGroup, PanelResizeHandle } from "react-resizable-panels";
import { api, QueryResult, redis, RedisKeyDetail, RedisKeyRow, RedisReply } from "../lib/api";
import { keyTree, parentPaths, visibleRows } from "../lib/keyTree";
import { FolderIcon } from "../ui/icons";
import { Badge, Button, EmptyState, FormGrid, IconButton, Modal, Notice, Select, Spacer, Table, Tabs, Td, Text, TextArea, TextInput, Th, ToolButton, Toolbar, TreeRow } from "../ui";

const isMacOS = navigator.platform.toLowerCase().includes("mac");

/** 키 목록 한 줄 높이 (App.css .rd-key 와 같다) · 터미널 출력 최대 줄 수 */
const KEY_ROW_H = 22;
const TERM_MAX = 1000;

/** Medis 식 Redis 브라우저: 왼쪽 키 목록(패턴·타입 필터, SCAN 더 보기), 오른쪽 Content / Terminal / Info / Config */
interface Props {
  target: string;
  connName: string;
  database: string;
  dbList: string[];
  readOnly: boolean;
  initialKey?: string;
  onSwitchDb: (db: string) => void;
  onLoadDbs: () => void;
  onStatus: (s: string) => void;
}

const TYPES = ["string", "hash", "list", "set", "zset", "stream"];
export const TYPE_COLOR: Record<string, string> = {
  string: "#5f9ea0",
  hash: "#b07cc6",
  list: "#6a9fd8",
  set: "#d08b4c",
  zset: "#c9605c",
  stream: "#5aa469",
};

export function TypeBadge({ type }: { type: string }) {
  return (
    <Badge color={TYPE_COLOR[type] ?? "#777"} className="rd-badge">
      {type === "zset" ? "ZSET" : type.toUpperCase()}
    </Badge>
  );
}

const fmtTtl = (ms: number) => {
  if (ms < 0) return "";
  const s = Math.round(ms / 1000);
  if (s < 60) return `${s}s`;
  if (s < 3600) return `${Math.floor(s / 60)}m ${s % 60}s`;
  if (s < 86400) return `${Math.floor(s / 3600)}h ${Math.floor((s % 3600) / 60)}m`;
  return `${Math.floor(s / 86400)}d ${Math.floor((s % 86400) / 3600)}h`;
};
const fmtBytes = (n: number | null) => (n == null ? "" : n < 1024 ? `${n} B` : n < 1048576 ? `${(n / 1024).toFixed(1)} KB` : `${(n / 1048576).toFixed(1)} MB`);

/** redis-cli 와 같은 출력 */
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
  const [pattern, setPattern] = useState("*");
  const [typeFilter, setTypeFilter] = useState("");
  const [keys, setKeys] = useState<RedisKeyRow[]>([]);
  const [cursor, setCursor] = useState("0");
  const [scanning, setScanning] = useState(false);
  const [sel, setSel] = useState<string | null>(p.initialKey ?? null);
  const [tab, setTab] = useState<"content" | "terminal" | "info" | "config">("content");
  const [adding, setAdding] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // 키는 ":" 로 묶은 폴더로 보인다 (Medis). open = 펼친 폴더, cur = 고른 폴더 (키를 고르면 null)
  const [open, setOpen] = useState<Set<string>>(new Set());
  const [cur, setCur] = useState<string | null>(null);
  const rows = useMemo(() => visibleRows(keyTree(keys, (k) => k.key), open), [keys, open]);
  const toggle = (path: string) =>
    setOpen((o) => {
      const n = new Set(o);
      if (n.has(path)) n.delete(path);
      else n.add(path);
      return n;
    });
  // 고른 키(처음 연 키, 새로 만든 키)가 보이도록 위 폴더를 펼친다
  useEffect(() => {
    if (!sel) return;
    setOpen((o) => {
      const add = parentPaths(sel).filter((x) => !o.has(x));
      return add.length ? new Set([...o, ...add]) : o;
    });
  }, [sel]);

  const listRef = useRef<HTMLDivElement>(null);
  const keyVirt = useVirtualizer({ count: rows.length, getScrollElement: () => listRef.current, estimateSize: () => KEY_ROW_H, overscan: 20 });

  const scan = async (reset: boolean) => {
    setScanning(true);
    setError(null);
    try {
      const page = await redis.scan(p.target, reset ? "0" : cursor, pattern, 500, typeFilter);
      setKeys((ks) => {
        // 이미 정렬된 목록에 새 페이지(정렬)만 합친다 (매번 전체를 다시 정렬하지 않게)
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
      setError(String(e));
    } finally {
      setScanning(false);
    }
  };
  useEffect(() => {
    scan(true);
  }, [p.target, typeFilter]);
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
        // ⌘r = 다시 읽기 (툴바 Scan again 과 같다)
        if ((isMacOS ? e.metaKey : e.ctrlKey) && !e.shiftKey && !e.altKey && e.code === "KeyR") {
          e.preventDefault();
          scan(true);
        }
      }}
    >
      <Toolbar className="rd-toolbar">
        <Text strong>{p.connName}</Text>
        <Select value={p.database} onMouseDown={p.onLoadDbs} onFocus={p.onLoadDbs} onChange={(e) => p.onSwitchDb(e.target.value)} title="Database">
          {[...new Set([p.database, ...p.dbList])].map((d) => (
            <option key={d} value={d}>
              {d}
            </option>
          ))}
        </Select>
        <TextInput
          className="rd-pattern"
          value={pattern}
          spellCheck={false}
          placeholder="Key name or pattern (e.g. user:*)"
          onChange={(e) => setPattern(e.target.value)}
          onKeyDown={(e) => {
            e.stopPropagation();
            if (e.key === "Enter") scan(true);
          }}
        />
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
              // 폴더: → 펼치기 · ← 접기 · Enter 펼치고 접기. 그 밖의 ← 는 부모 폴더로
              if (r?.kind === "folder" && (e.key === "Enter" || (e.key === "ArrowRight" && !open.has(r.path)) || (e.key === "ArrowLeft" && open.has(r.path)))) {
                e.preventDefault();
                toggle(r.path);
              } else if (r && e.key === "ArrowLeft" && r.depth > 0) {
                for (let j = i - 1; j >= 0; j--) if (rows[j].kind === "folder" && rows[j].depth === r.depth - 1) return go(j);
              }
            }}>
              {/* 보이는 줄만 그린다 (키가 수만 개여도 가볍게) */}
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
                        trailing={r.count.toLocaleString()}
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
                      trailing={fmtTtl(k.ttl)}
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
              items={(["content", "terminal", "info", "config"] as const).map((t) => ({ id: t, label: t[0].toUpperCase() + t.slice(1) }))}
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
              {tab === "terminal" && <Terminal target={p.target} prompt={`${p.connName}[${p.database.replace(/^db/, "")}]>`} onChanged={() => scan(true)} />}
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

// ───────── 키 내용 (타입별) ─────────

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
  // 아래 편집기에 보이는 값 (선택한 요소)
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
      // TTL 유지
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
      // 인덱스로 지우기: 표식으로 바꾼 뒤 LREM
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

function Terminal({ target, prompt, onChanged }: { target: string; prompt: string; onChanged: () => void }) {
  const [lines, setLines] = useState<{ kind: "in" | "out" | "err"; text: string }[]>([]);
  const [input, setInput] = useState("");
  const hist = useRef<string[]>([]);
  const hi = useRef(-1);
  const end = useRef<HTMLDivElement>(null);
  useEffect(() => {
    end.current?.scrollIntoView({ block: "end" });
  }, [lines]);
  const submit = async () => {
    const line = input.trim();
    if (!line) return;
    hist.current = [line, ...hist.current.filter((h) => h !== line)].slice(0, 200);
    hi.current = -1;
    setInput("");
    if (line.toLowerCase() === "clear") return setLines([]);
    // FLUSHALL·FLUSHDB·DEL 같은 지우는 명령은 한 번 더 묻는다
    const why = await api.unsafeReason("redis", line).catch(() => null);
    if (why && !(await ask(`${why}. Run it anyway?`, { detail: [{ text: line, tone: "danger" }], okLabel: "Run Anyway", danger: true }))) return;
    // 출력은 최근 TERM_MAX 줄만 남긴다
    const push = (x: { kind: "in" | "out" | "err"; text: string }) => setLines((l) => [...l, x].slice(-TERM_MAX));
    push({ kind: "in", text: `${prompt} ${line}` });
    try {
      const r = await redis.execLine(target, line);
      push({ kind: "out", text: formatReply(r) });
      if (!/^(get|hget|hgetall|lrange|smembers|zrange|scan|keys|type|ttl|pttl|exists|info|config|dbsize|ping|strlen|llen|scard|zcard|hlen|xrange|xlen|memory|object)\b/i.test(line)) onChanged();
    } catch (e) {
      push({ kind: "err", text: `(error) ${String(e).replace(/^Redis: /, "")}` });
    }
  };
  return (
    <div className="rd-terminal" onClick={(e) => (e.currentTarget.querySelector("input") as HTMLInputElement | null)?.focus()}>
      <div className="rd-term-out">
        {lines.map((l, i) => (
          <pre key={i} className={l.kind}>
            {l.text}
          </pre>
        ))}
        <div ref={end} />
      </div>
      <div className="rd-term-in">
        <Text tone="muted">{prompt}</Text>
        <TextInput
          autoFocus
          value={input}
          spellCheck={false}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={(e) => {
            e.stopPropagation();
            if (e.key === "Enter") submit();
            else if (e.key === "ArrowUp" && hist.current.length) {
              e.preventDefault();
              hi.current = Math.min(hi.current + 1, hist.current.length - 1);
              setInput(hist.current[hi.current]);
            } else if (e.key === "ArrowDown") {
              e.preventDefault();
              hi.current = Math.max(hi.current - 1, -1);
              setInput(hi.current < 0 ? "" : hist.current[hi.current]);
            } else if (e.key === "l" && e.ctrlKey) {
              e.preventDefault();
              setLines([]);
            }
          }}
        />
      </div>
    </div>
  );
}

/** Info / Config: 검색되는 표 */
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
  // 관리형 Redis (AWS ElastiCache · MemoryDB, Azure, GCP Memorystore, Upstash …) 는 CONFIG 를 이름을 바꾸거나 막아 둔다
  if (err && /^CONFIG/i.test(command) && /unknown command|NOPERM|disabled|not allowed/i.test(err))
    return (
      <Notice>
        This server doesn't allow <code>CONFIG</code> — managed Redis services (AWS ElastiCache / MemoryDB, Azure Cache, GCP Memorystore, Upstash …) disable or rename it. Its settings are in the provider's console (e.g. the ElastiCache parameter group). Server details are still in Info.
      </Notice>
    );
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
