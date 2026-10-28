import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { ask, askText } from "./lib/dialogs";
import { listen } from "@tauri-apps/api/event";
import { Panel, PanelGroup, PanelResizeHandle } from "react-resizable-panels";
import { analyzePlan, api, backup, ConnectionView, previewImpact, dialectOf, meta, VECTOR_KINDS, QueryResult, redisDb, setUpdateCheckEnabled, targetId, UpdateInfo, updateCheckEnabled } from "./lib/api";
import { RedisBrowser } from "./components/RedisBrowser";
import { KeymapDialog } from "./components/KeymapDialog";
import { KeymapEditor } from "./components/KeymapEditor";
import { UpdateDialog } from "./components/UpdateDialog";
import { ContextMenu, Explorer, MenuItem } from "./components/Explorer";
import { ConnectionDialog } from "./components/ConnectionDialog";
import { ConsoleEditor, EditorApi } from "./components/ConsoleEditor";
import { ConsolePane, OutputLine, ResultEntry } from "./components/ConsolePane";
import { HistoryDialog, migrateOldHistory, pushHistory, saveSnippet } from "./components/HistoryDialog";
import { EditTarget } from "./components/ResultGrid";
import { TableEditor } from "./components/TableEditor";
import { RagView } from "./components/RagView";
import { WorkspaceBar, WorkspaceInfo } from "./components/WorkspaceBar";
import { forgetConsoleModel } from "./lib/monaco";
import { TextDialog } from "./components/TextDialog";
import { RecentFiles, Switcher, SwitchItem } from "./components/Switcher";
import { ActionItem, DbObject, invalidateSearchIndex, SearchEverywhere, SeTab } from "./components/SearchEverywhere";
import { FindPanel, FindRun, FtsScope, FullTextSearchDialog } from "./components/FullTextSearch";
import { DEFAULT_FONT, editorFontSize, setEditorFontSize } from "./lib/prefs";
import { bindings, eventSpec, formatSpec, hit, loadKeymap, onKeymapChange } from "./lib/shortcuts";
import { invalidateCompletion } from "./lib/completion";
import { useStatusWidgets } from "./lib/statusbar";
import type { FtsRequest } from "./lib/api";
import { isRelation, NodeInfo, revealInTree } from "./components/SchemaTree";
import { leaf, leaves, MAX_PANES, Pane, prunePanes, splitPane } from "./lib/panes";
import { connBg, getThemePref, setThemePref, THEME_LABEL, ThemePref, useTheme } from "./lib/theme";
import { BackupDialog, ExportDialog, ImportDialog, TransferBar, type TransferJob } from "./components/Transfer";
import { DbaReport } from "./components/DbaReport";
import { ErdView } from "./components/ErdView";
import { MigrateView } from "./components/MigrateView";
import type { DataFormat } from "./lib/transfer";
import "./App.css";

/** 칸 id (0~3). 칸 배치는 Pane 트리 */
type Group = number;

/** 워크스페이스 하나의 탭·분할 화면 (지금 것이 아닐 때 보관한다) */
interface WsLayout {
  tabs: Tab[];
  panes: Pane;
  activeIds: (string | null)[];
  focused: Group;
  mru: string[];
}
const emptyLayout = (): WsLayout => ({ tabs: [], panes: leaf(0), activeIds: [null, null, null, null], focused: 0, mru: [] });

interface Tab {
  id: string;
  /** console: SQL 콘솔 / table: 테이블 데이터 에디터 / ddl: 읽기 전용 DDL */
  kind: "console" | "table" | "ddl" | "redis" | "dba" | "erd" | "migrate" | "rag";
  group: Group;
  connId: string;
  /** 접속한 DB (없으면 Data Source 기본 DB) */
  database?: string;
  /** 백엔드 접속 대상 id (connId 또는 connId::database) */
  target: string;
  title: string;
  sql: string;
  running: boolean;
  result: QueryResult | null;
  error: string | null;
  lastSql: string | null;
  loadingMore: boolean;
  where: string;
  orderBy: string;
  /** 콘솔의 현재 스키마 (search_path) */
  schema?: string;
  tableSql?: string;
  edit?: EditTarget;
  /** 콘솔: 결과 탭들, Output 로그, 트랜잭션 */
  results: ResultEntry[];
  activeResult: string;
  output: OutputLine[];
  txManual: boolean;
  inTx: boolean;
  /** 서버 커서가 열린 결과 탭 (더 불러오기 대상) */
  cursorEntry?: string;
  /** Pin Tab */
  pinned?: boolean;
  /** Redis 브라우저: 처음 선택할 키 */
  redisKey?: string;
  /** dba: 보고 있는 리포트 (sessions, slow_queries, …) */
  dbaReport?: string;
  /** erd: 그릴 스키마들 */
  erdSchemas?: string[];
  /** 다시 켤 때 되살린 탭: 아직 불러오지 않았다 (처음 보일 때 불러온다) */
  stale?: boolean;
}

/** 닫은 탭 / 최근 파일 (Reopen Closed Tab, ⌘e Recent Files) */
interface TabSnapshot {
  key: string;
  kind: Tab["kind"];
  title: string;
  connId: string;
  database?: string;
  sql: string;
  edit?: EditTarget;
  where: string;
  orderBy: string;
}
const TAB_LIMIT = 10; // DataGrip 기본 Tab limit
const snapKey = (t: { kind: string; connId: string; database?: string; title: string; edit?: EditTarget }) =>
  `${t.kind}|${t.connId}|${t.database ?? ""}|${t.edit ? t.edit.schema + "." + t.edit.table : t.title}`;

const isMac = navigator.platform.toLowerCase().includes("mac");
/** 결과 그리드에 쌓아 두는 최대 행 수 (TUI 와 같다). 그 이상은 CSV 로 내보낸다 */
const MAX_ROWS = 100_000;
const capMsg = (n: number) => `Showing the first ${n.toLocaleString()} rows — export to CSV for all of them`;
/** 메뉴·Find Action 에 보일 단축키 (첫 번째 키) */
const sk = (id: string) => {
  const b = bindings(id);
  return b.length ? formatSpec(b[0]) : undefined;
};
const last = <T,>(a: T[]): T | undefined => a[a.length - 1];

export default function App() {
  const theme = useTheme();
  // 키맵이 바뀌면 메뉴의 단축키 표시를 다시 그린다
  const [, setKeymapVer] = useState(0);
  useEffect(() => {
    const off = onKeymapChange(() => setKeymapVer((n) => n + 1));
    void loadKeymap();
    return () => void off();
  }, []);
  const [settingsOpen, setSettingsOpen] = useState<false | "appearance" | "keymap">(false);
  const [schemeOpen, setSchemeOpen] = useState(false);
  const [keymapOpen, setKeymapOpen] = useState(false);
  // 창을 닫을 때 커밋 안 한 변경이 있으면 묻는다 (닫으면 롤백된다)
  useEffect(() => {
    let off: (() => void) | undefined;
    import("@tauri-apps/api/window")
      .then(({ getCurrentWindow }) =>
        getCurrentWindow().onCloseRequested(async (e) => {
          if (!(await readyToQuit())) e.preventDefault();
        }),
      )
      .then((u) => (off = u))
      .catch(() => {});
    return () => off?.();
  }, []);
  // 새 버전 (시작할 때 + 1시간마다 확인. 실패는 조용히 넘긴다)
  const [update, setUpdate] = useState<UpdateInfo | null>(null);
  const [updateOpen, setUpdateOpen] = useState(false);
  useEffect(() => {
    const check = () => {
      if (!updateCheckEnabled()) return;
      api
        .checkUpdate(false)
        .then((u) => u.latest && setUpdate(u))
        .catch(() => {});
    };
    check();
    const t = setInterval(check, 60 * 60 * 1000);
    return () => clearInterval(t);
  }, []);
  const [conns, setConns] = useState<ConnectionView[]>([]);
  const [tabs, setTabs] = useState<Tab[]>([]);
  /** 칸마다 보이는 탭 */
  const [activeIds, setActiveIds] = useState<(string | null)[]>([null, null, null, null]);
  const activeIdsRef = useRef(activeIds);
  activeIdsRef.current = activeIds;
  // 화면 분할 (최대 4칸)
  const [panes, setPanes] = useState<Pane>(leaf(0));
  const panesRef = useRef(panes);
  panesRef.current = panes;
  const split = panes.t === "split";
  const [focused, setFocused] = useState<Group>(0);
  const focusedRef = useRef(focused);
  focusedRef.current = focused;
  // ───────── 워크스페이스 (Orca 처럼): 각자 탭·분할 화면. 지금 것은 위 상태, 나머지는 stash (탭 객체·콘솔 세션·트랜잭션 그대로) ─────────
  const [workspaces, setWorkspaces] = useState<WorkspaceInfo[]>([{ id: "ws1", name: "Default", connIds: [] }]);
  const workspacesRef = useRef(workspaces);
  workspacesRef.current = workspaces;
  const [curWs, setCurWs] = useState("ws1");
  const curWsRef = useRef(curWs);
  curWsRef.current = curWs;
  const stash = useRef(new Map<string, WsLayout>());
  const [dialog, setDialog] = useState<{ edit?: ConnectionView } | null>(null);
  const [status, setStatus] = useState("Ready");
  const [treeSel, setTreeSel] = useState<NodeInfo | null>(null);
  const [tabMenu, setTabMenu] = useState<{ x: number; y: number; items: MenuItem[] } | null>(null);
  const editorApis = useRef(new Map<string, EditorApi>());
  const [historyFor, setHistoryFor] = useState<string | null>(null);
  const [historyTab, setHistoryTab] = useState<"history" | "snippets">("history");
  const tabsRef = useRef(tabs);
  tabsRef.current = tabs;

  // 쿼리를 실행할 때마다 불리므로, 실제로 바뀐 게 있을 때만 상태를 바꾼다 (트리 전체 다시 그리기·단축키 핸들러 재등록 방지)
  const connsJson = useRef("");
  const refresh = useCallback(async () => {
    try {
      const cs = await api.listConnections();
      const j = JSON.stringify(cs);
      if (j !== connsJson.current) {
        connsJson.current = j;
        setConns(cs);
      }
    } catch (e) {
      setStatus(String(e));
    }
  }, []);
  useEffect(() => {
    refresh();
  }, [refresh]);
  // Qurry › Check for Updates…
  useEffect(() => {
    let off: (() => void) | undefined;
    listen("check-update", async () => {
      setStatus("Checking for updates…");
      try {
        const u = await api.checkUpdate(true);
        if (u.latest) {
          setUpdate(u);
          setUpdateOpen(true);
          setStatus("");
        } else setStatus(`You have the latest version (${u.current}).`);
      } catch (e) {
        setStatus(String(e));
      }
    })
      .then((u) => (off = u))
      .catch(() => {});
    return () => off?.();
  }, []);
  // Help 메뉴 › Keymap Reference
  useEffect(() => {
    let off: (() => void) | undefined;
    listen("open-keymap", () => setKeymapOpen(true))
      .then((u) => (off = u))
      .catch(() => {});
    return () => off?.();
  }, []);
  // Qurry › Settings…
  useEffect(() => {
    let off: (() => void) | undefined;
    listen("open-settings", () => setSettingsOpen("appearance"))
      .then((u) => (off = u))
      .catch(() => {});
    return () => off?.();
  }, []);
  useEffect(migrateOldHistory, []);
  // Auto-disconnect 등으로 백엔드가 접속을 닫으면 트리 상태를 갱신
  useEffect(() => {
    let off: (() => void) | undefined;
    listen<string>("conn-disconnected", (e) => {
      invalidateSearchIndex(e.payload);
      invalidateCompletion(e.payload);
      refresh();
      setStatus(`Disconnected (idle): ${e.payload}`);
    })
      .then((u) => (off = u))
      .catch(() => {});
    return () => off?.();
  }, [refresh]);
  useEffect(() => {
    const onErr = (e: ErrorEvent) => setStatus(`Error: ${e.message}`);
    const onRej = (e: PromiseRejectionEvent) => setStatus(`Error: ${String(e.reason)}`);
    const closeMenu = () => setTabMenu(null);
    window.addEventListener("error", onErr);
    window.addEventListener("unhandledrejection", onRej);
    window.addEventListener("mousedown", closeMenu);
    return () => {
      window.removeEventListener("error", onErr);
      window.removeEventListener("unhandledrejection", onRej);
      window.removeEventListener("mousedown", closeMenu);
    };
  }, []);

  const groupActive = (g: Group) => tabs.find((t) => t.id === activeIds[g] && t.group === g) ?? null;
  const active = groupActive(focused) ?? leaves(panes).map(groupActive).find(Boolean) ?? null;
  const activeConn = conns.find((c) => c.id === active?.connId);
  /** 다른 워크스페이스에 있는 탭이면 거기서 바꾼다 (그 사이 끝난 쿼리 결과가 사라지지 않게) */
  const updStashed = (id: string, fn: (t: Tab) => Tab) => {
    for (const w of stash.current.values()) {
      if (w.tabs.some((t) => t.id === id)) {
        w.tabs = w.tabs.map((t) => (t.id === id ? fn(t) : t));
        return true;
      }
    }
    return false;
  };
  /** 모든 워크스페이스의 탭 */
  const allTabs = () => [...tabsRef.current, ...[...stash.current.values()].flatMap((w) => w.tabs)];
  const patch = (id: string, p: Partial<Tab>) => {
    if (!updStashed(id, (t) => ({ ...t, ...p }))) setTabs((ts) => ts.map((t) => (t.id === id ? { ...t, ...p } : t)));
  };
  const mru = useRef<string[]>([]);
  const recent = useRef<TabSnapshot[]>([]);
  const closedStack = useRef<TabSnapshot[]>([]);
  // 콘솔에서 치는 SQL 은 상태(setTabs)에 넣지 않고 여기 둔다 → 키 입력마다 앱 전체를 다시 그리지 않는다
  const liveSql = useRef(new Map<string, string>());
  const sqlOf = (t: Tab) => liveSql.current.get(t.id) ?? t.sql;
  const snapshot = (t: Tab): TabSnapshot => ({
    key: snapKey(t),
    kind: t.kind,
    title: t.title,
    connId: t.connId,
    database: t.database,
    sql: sqlOf(t),
    edit: t.edit,
    where: t.where,
    orderBy: t.orderBy,
  });
  const remember = (t: Tab) => {
    const snap = snapshot(t);
    recent.current = [snap, ...recent.current.filter((r) => r.key !== snap.key)].slice(0, 50);
  };
  const activate = (t: Tab) => {
    const cur = tabsRef.current.find((x) => x.id === t.id);
    if (cur?.stale) loadStale(cur);
    setFocused(t.group);
    setActiveIds((a) => a.map((x, g) => (g === t.group ? t.id : x)));
    mru.current = [t.id, ...mru.current.filter((x) => x !== t.id)];
    remember(t);
  };

  // ───────── 탭 열기 ─────────
  const addTab = (
    c: ConnectionView,
    kind: Tab["kind"],
    opts: { title?: string; sql?: string; database?: string; tableSql?: string; edit?: EditTarget; group?: Group; where?: string; orderBy?: string },
  ) => {
    const database = opts.database && opts.database !== (c.database || "postgres") ? opts.database : undefined;
    const base = database ? `${c.name}/${database}` : c.name;
    const n = tabsRef.current.filter((t) => t.kind === "console" && t.connId === c.id && t.database === database).length;
    const ls = leaves(panesRef.current);
    const group: Group = opts.group ?? (ls.includes(focused) ? focused : ls[0]);
    const tab = buildTab(c, kind, opts, group, opts.title ?? (n === 0 ? `console [${base}]` : `console_${n + 1} [${base}]`));
    // Transaction control: Manual 이 기본이면 세션도 Manual 로 연다
    if (tab.txManual) api.setTxMode(tab.target, tab.id, true).catch((e) => setStatus(String(e)));
    setTabs((ts) => [...ts, tab]);
    activate(tab);
    // Tab limit: 고정되지 않은 탭이 10개를 넘으면 가장 오래 안 쓴 탭부터 닫는다 (DataGrip)
    setTimeout(() => {
      const inGroup = tabsRef.current.filter((t) => t.group === group && !t.pinned && t.id !== tab.id);
      const over = inGroup.length + 1 - TAB_LIMIT;
      if (over > 0) {
        const byAge = [...inGroup].sort((a, b) => {
          const ia = mru.current.indexOf(a.id), ib = mru.current.indexOf(b.id);
          return (ib < 0 ? -1 : ib) - (ia < 0 ? -1 : ia);
        });
        closeTabs(byAge.slice(0, over).map((t) => t.id));
      }
    });
    return tab;
  };

  /** 탭 하나 (상태만. 열기·실행은 하지 않는다) */
  const buildTab = (
    c: ConnectionView,
    kind: Tab["kind"],
    opts: { sql?: string; database?: string; tableSql?: string; edit?: EditTarget; where?: string; orderBy?: string },
    group: Group,
    title: string,
  ): Tab => {
    const database = opts.database && opts.database !== (c.database || "postgres") ? opts.database : undefined;
    return {
      id: crypto.randomUUID(),
      kind,
      group,
      connId: c.id,
      database,
      target: targetId(c.id, database),
      title,
      sql: opts.sql ?? "",
      running: false,
      result: null,
      error: null,
      lastSql: null,
      loadingMore: false,
      where: opts.where ?? "",
      orderBy: opts.orderBy ?? "",
      tableSql: opts.tableSql,
      edit: opts.edit,
      results: [],
      activeResult: "output",
      output: [],
      txManual: kind === "console" && !!c.options?.txManual,
      inTx: false,
    };
  };

  // ───────── 다시 켤 때 되살리기 (workspace-gui.json) ─────────
  const restored = useRef(false);
  const savedWs = useRef("");
  /** 저장할 내용: 탭(콘솔 SQL·테이블 WHERE/ORDER BY·DDL·Redis)과 분할·칸마다 보이는 탭·포커스 */
  const wsJson = () => {
    const ser = (l: WsLayout) => ({
      tabs: l.tabs.map((t) => ({
        kind: t.kind,
        title: t.title,
        connId: t.connId,
        database: t.database,
        group: t.group,
        sql: t.kind === "console" || t.kind === "ddl" ? sqlOf(t) : "",
        where: t.where,
        orderBy: t.orderBy,
        edit: t.edit ? { schema: t.edit.schema, table: t.edit.table } : undefined,
        pinned: t.pinned,
        redisKey: t.redisKey,
        dbaReport: t.dbaReport,
        erdSchemas: t.erdSchemas,
      })),
      panes: l.panes,
      activeIds: l.activeIds.map((id) => l.tabs.findIndex((t) => t.id === id)),
      focused: l.focused,
    });
    const live: WsLayout = { tabs: tabsRef.current, panes: panesRef.current, activeIds: activeIdsRef.current, focused: focusedRef.current, mru: [] };
    return JSON.stringify({
      version: 2,
      current: curWsRef.current,
      workspaces: workspacesRef.current.map((w) => ({ id: w.id, name: w.name, connIds: w.connIds, ...ser(w.id === curWsRef.current ? live : stash.current.get(w.id) ?? emptyLayout()) })),
    });
  };
  /** 앱을 닫거나 업데이트로 다시 켜기 전: 커밋 안 한 변경이 있으면 묻고, 열어 둔 탭을 저장한다 */
  const readyToQuit = async () => {
    const n = allTabs().filter((t) => t.inTx).length;
    if (n && !(await ask(`${n} console(s) have uncommitted changes. Quit and roll them back?`))) return false;
    saveWs();
    return true;
  };
  const saveWs = () => {
    if (!restored.current) return;
    const j = wsJson();
    if (j === savedWs.current) return;
    savedWs.current = j;
    api.saveWorkspace(JSON.parse(j)).catch(() => {});
  };
  /** 되살린 탭을 처음 볼 때: 테이블은 데이터를 불러오고, 콘솔은 트랜잭션 모드를 맞춘다 (Redis 는 화면이 스스로 불러온다) */
  const loadStale = async (t: Tab) => {
    patch(t.id, { stale: false });
    if (t.kind === "console" && t.txManual) api.setTxMode(t.target, t.id, true).catch((e) => setStatus(String(e)));
    if (t.kind === "table") loadTable(t);
  };
  /** 테이블 탭의 PK 와 데이터를 같이 불러온다 (접속이 안 되면 둘 다 기다리느라 두 배로 걸린다) */
  const loadTable = (t: Tab) => {
    const { edit, tableSql } = t;
    if (!edit || !tableSql) return;
    meta
      .table(t.target, edit.schema, edit.table)
      .then((d) => updTab(t.id, (x) => (x.edit ? { ...x, edit: { ...x.edit, pk: d.columns.filter((c) => c.isPk).map((c) => c.name) } } : x)))
      .catch(() => {
        /* 메타 조회 실패 시 읽기 전용 */
      });
    run(t, tableSql);
  };
  // 방금 접속된 Data Source: 접속 실패로 비어 있던 테이블 탭을 다시 불러온다
  const wasConnected = useRef(new Set<string>());
  useEffect(() => {
    const now = new Set(conns.filter((c) => c.connected).map((c) => c.id));
    const back = [...now].filter((id) => !wasConnected.current.has(id));
    wasConnected.current = now;
    if (!back.length) return;
    for (const t of tabsRef.current) {
      if (t.kind === "table" && back.includes(t.connId) && t.error && !t.running && !t.stale) loadTable(t);
    }
  }, [conns]);
  /** 저장된 워크스페이스 하나 → 탭·분할 화면 (탭은 고를 때 불러온다) */
  const buildLayout = (ws: any): WsLayout => {
    if (!ws || !Array.isArray(ws.tabs)) return emptyLayout();
    const okPane = (p: any): boolean => p && (p.t === "leaf" ? Number.isInteger(p.g) && p.g >= 0 && p.g < MAX_PANES : p.t === "split" && okPane(p.a) && okPane(p.b));
    const pane0: Pane = okPane(ws.panes) ? ws.panes : leaf(0);
    const ls0 = leaves(pane0);
    const built: (Tab | null)[] = ws.tabs.map((x: any) => {
      const c = conns.find((cc) => cc.id === x.connId);
      if (!c || !["console", "table", "ddl", "redis"].includes(x.kind)) return null;
      const group = ls0.includes(x.group) ? x.group : ls0[0];
      let tableSql: string | undefined;
      if (x.kind === "table" && x.edit) {
        const { q } = dialectOf(c.kind);
        tableSql = `SELECT * FROM ${q(x.edit.schema)}.${q(x.edit.table)}${x.where ? ` WHERE ${x.where}` : ""}${x.orderBy ? ` ORDER BY ${x.orderBy}` : ""};`;
      }
      const t = buildTab(c, x.kind, { sql: x.kind === "table" ? tableSql : x.sql, database: x.database, tableSql, edit: x.edit ? { ...x.edit, pk: [] } : undefined, where: x.where, orderBy: x.orderBy }, group, x.title);
      return { ...t, pinned: x.pinned, redisKey: x.redisKey, dbaReport: x.dbaReport, erdSchemas: x.erdSchemas, stale: x.kind !== "ddl" };
    });
    const ts = built.filter((t): t is Tab => !!t);
    if (!ts.length) return emptyLayout();
    const pane = prunePanes(pane0, new Set(ts.map((t) => t.group)));
    const ls = leaves(pane);
    const act = [0, 1, 2, 3].map((g) => {
      if (!ls.includes(g)) return null;
      const want = built[ws.activeIds?.[g]];
      return want && want.group === g ? want.id : ts.find((t) => t.group === g)?.id ?? null;
    });
    return { tabs: ts, panes: pane, activeIds: act, focused: ls.includes(ws.focused) ? ws.focused : ls[0], mru: act.filter((x): x is string => !!x) };
  };
  /** 이 탭·분할 화면을 보인다. 칸마다 보이는 탭만 불러온다 (나머지는 고를 때) */
  const showLayout = (l: WsLayout) => {
    setTabs(l.tabs);
    setPanes(l.panes);
    setActiveIds(l.activeIds);
    setFocused(l.focused);
    mru.current = l.mru;
    for (const id of l.activeIds) {
      const t = l.tabs.find((x) => x.id === id);
      if (t?.stale) loadStale(t);
    }
  };
  const restoreWs = (data: any) => {
    // version 1 (워크스페이스 전) = 워크스페이스 하나
    const list: any[] | null = Array.isArray(data?.workspaces) ? data.workspaces : Array.isArray(data?.tabs) ? [{ id: "ws1", name: "Default", ...data }] : null;
    if (!list?.length) return;
    // 탐색기 목록이 없던 저장 파일 (0.2.0 이전·0.2.0): 첫 워크스페이스가 모든 Data Source 를 가진다
    const hasLists = list.some((w) => Array.isArray(w.connIds));
    const infos: WorkspaceInfo[] = list.map((w, i) => ({
      id: typeof w.id === "string" && w.id ? w.id : `ws${i + 1}`,
      name: typeof w.name === "string" && w.name.trim() ? w.name : `Workspace ${i + 1}`,
      connIds: hasLists ? (Array.isArray(w.connIds) ? w.connIds.filter((x: unknown) => typeof x === "string") : []) : i === 0 ? conns.map((c) => c.id) : [],
    }));
    const cur = infos.find((w) => w.id === data.current)?.id ?? infos[0].id;
    stash.current.clear();
    list.forEach((w, i) => {
      const l = buildLayout(w);
      if (infos[i].id === cur) showLayout(l);
      else stash.current.set(infos[i].id, l);
    });
    setWorkspaces(infos);
    setCurWs(cur);
  };
  /** 다른 워크스페이스로: 지금 것은 그대로 보관 (콘솔 세션·트랜잭션도). discard = 지금 것을 보관하지 않는다 (지울 때) */
  const switchWs = (id: string, discard = false) => {
    const from = curWsRef.current;
    if (id === from || !workspacesRef.current.some((w) => w.id === id)) return;
    const target = stash.current.get(id) ?? emptyLayout();
    stash.current.delete(id);
    if (!discard) stash.current.set(from, { tabs: tabsRef.current, panes: panesRef.current, activeIds: activeIdsRef.current, focused: focusedRef.current, mru: mru.current });
    tabsRef.current = target.tabs;
    curWsRef.current = id;
    showLayout(target);
    setCurWs(id);
  };
  const newWs = async () => {
    const name = (await askText("Workspace name", `Workspace ${workspacesRef.current.length + 1}`));
    if (!name?.trim()) return;
    const w = { id: crypto.randomUUID(), name: name.trim(), connIds: [] };
    setWorkspaces((l) => [...l, w]);
    workspacesRef.current = [...workspacesRef.current, w];
    switchWs(w.id);
  };
  const renameWs = async (id: string) => {
    const w = workspacesRef.current.find((x) => x.id === id);
    const name = w && (await askText("Rename workspace", w.name));
    if (name?.trim()) setWorkspaces((l) => l.map((x) => (x.id === id ? { ...x, name: name.trim() } : x)));
  };
  /** 워크스페이스와 그 탭을 닫는다 (커밋 안 한 변경은 롤백된다) */
  const deleteWs = async (id: string) => {
    const list = workspacesRef.current;
    const w = list.find((x) => x.id === id);
    if (!w || list.length < 2) return;
    const ts = id === curWsRef.current ? tabsRef.current : stash.current.get(id)?.tabs ?? [];
    const dirty = ts.filter((t) => t.inTx).length;
    if (!(await ask(`Delete the workspace "${w.name}"${ts.length ? ` and close its ${ts.length} tab(s)` : ""}?${dirty ? `\n\n${dirty} console(s) have uncommitted changes — they are rolled back.` : ""}`))) return;
    if (id === curWsRef.current) switchWs(list.find((x) => x.id !== id)!.id, true);
    stash.current.delete(id);
    for (const t of ts) {
      forgetConsoleModel(t.id);
      editorApis.current.delete(t.id);
      liveSql.current.delete(t.id);
      api.closeSession(t.id).catch(() => {});
    }
    setWorkspaces((l) => l.filter((x) => x.id !== id));
    workspacesRef.current = workspacesRef.current.filter((x) => x.id !== id);
  };
  /** 탭을 다른 워크스페이스로 (세션 그대로). 그 워크스페이스의 첫 칸 맨 뒤에 */
  const moveToWs = (t: Tab, id: string) => {
    const target = stash.current.get(id) ?? emptyLayout();
    const g = leaves(target.panes)[0];
    stash.current.set(id, { ...target, tabs: [...target.tabs, { ...t, group: g }], activeIds: target.activeIds.map((x, i) => (i === g ? t.id : x)), mru: [t.id, ...target.mru] });
    mru.current = mru.current.filter((x) => x !== t.id);
    setTabs((ts) => {
      const rest = ts.filter((x) => x.id !== t.id);
      settle(rest, panesRef.current);
      return rest;
    });
    setStatus(`Moved "${t.title}" to ${workspacesRef.current.find((w) => w.id === id)?.name}`);
  };
  useEffect(() => {
    if (restored.current || !conns.length) return;
    restored.current = true;
    api
      .loadWorkspace()
      .then(restoreWs)
      .catch(() => {})
      .finally(() => setWsReady(true));
  }, [conns]);
  // 어느 워크스페이스에도 없는 Data Source (방금 만든 것, TUI 에서 만든 것) → 지금 워크스페이스로
  const [wsReady, setWsReady] = useState(false);
  useEffect(() => {
    if (!wsReady) return;
    const known = new Set(workspacesRef.current.flatMap((w) => w.connIds));
    const orphans = conns.map((c) => c.id).filter((id) => !known.has(id));
    if (orphans.length) setWorkspaces((l) => l.map((w) => (w.id === curWsRef.current ? { ...w, connIds: [...w.connIds, ...orphans] } : w)));
  }, [conns, wsReady]);
  /** Data Source 를 워크스페이스 탐색기에 넣고 뺀다 (Data Source 자체는 그대로) */
  const setInWs = (wsId: string, connId: string, on: boolean) =>
    setWorkspaces((l) => l.map((w) => (w.id !== wsId ? w : { ...w, connIds: on ? [...w.connIds.filter((x) => x !== connId), connId] : w.connIds.filter((x) => x !== connId) })));
  /** 탐색기를 띄운 워크스페이스 (펼친 상태를 지키려고 한 번 띄우면 숨겨 둔다) */
  const [shownWs, setShownWs] = useState<string[]>([]);
  useEffect(() => setShownWs((l) => (l.includes(curWs) ? l : [...l, curWs])), [curWs]);
  // 바뀌면 저장 (콘솔에 친 SQL 은 상태가 아니라 주기적으로도 본다)
  useEffect(() => {
    const h = setTimeout(saveWs, 800);
    return () => clearTimeout(h);
  }, [tabs, panes, activeIds, focused, workspaces, curWs]);
  useEffect(() => {
    const h = setInterval(saveWs, 5000);
    return () => clearInterval(h);
  }, []);

  const openConsole = (c: ConnectionView, db?: string, sql?: string) => addTab(c, "console", { database: db, sql });

  const openTable = async (c: ConnectionView, schema: string, table: string, db?: string) => {
    const target = targetId(c.id, db && db !== (c.database || "postgres") ? db : undefined);
    const existing = tabsRef.current.find((t) => t.kind === "table" && t.target === target && t.edit?.schema === schema && t.edit?.table === table);
    if (existing) return activate(existing);
    return openTableTab(c, schema, table, db);
  };

  /** 테이블 데이터 탭을 새로 연다 (split 복제 시에는 필터·정렬이 독립적인 별도 탭) */
  const openTableTab = async (c: ConnectionView, schema: string, table: string, db?: string, group?: Group, where = "", orderBy = "") => {
    const target = targetId(c.id, db && db !== (c.database || "postgres") ? db : undefined);
    const { q } = dialectOf(c.kind);
    let sql = `SELECT * FROM ${q(schema)}.${q(table)}`;
    if (where) sql += ` WHERE ${where}`;
    if (orderBy) sql += ` ORDER BY ${orderBy}`;
    sql += ";";
    let edit: EditTarget = { schema, table, pk: [] };
    try {
      const d = await meta.table(target, schema, table);
      edit = { schema, table, pk: d.columns.filter((x) => x.isPk).map((x) => x.name) };
    } catch {
      /* 메타 조회 실패 시 읽기 전용 */
    }
    const tab = addTab(c, "table", { title: table, sql, database: db, tableSql: sql, edit, group, where, orderBy });
    run(tab, sql);
  };

  const openDdl = async (info: NodeInfo) => {
    if (!info.schema || !info.name) return;
    const target = targetId(info.conn.id, info.db && info.db !== (info.conn.database || "postgres") ? info.db : undefined);
    const title = `${info.name} [DDL]`;
    const existing = tabsRef.current.find((t) => t.kind === "ddl" && t.target === target && t.title === title);
    if (existing) return activate(existing);
    try {
      setStatus(`Loading DDL of ${info.schema}.${info.name}…`);
      const ddl = await meta.ddl(target, info.schema, info.name, info.kind);
      addTab(info.conn, "ddl", { title, sql: ddl, database: info.db });
      setStatus("Ready");
    } catch (e) {
      setStatus(`DDL failed: ${String(e)}`);
    }
  };

  /** SQL Generator: 선택한 테이블로 문장을 만들어 새 콘솔에 넣는다 */
  const generate = async (info: NodeInfo, what: "select" | "insert" | "update" | "delete" | "ddl") => {
    if (!info.schema || !info.name) return;
    const target = targetId(info.conn.id, info.db && info.db !== (info.conn.database || "postgres") ? info.db : undefined);
    const quoteIdent = dialectOf(info.conn.kind).q;
    const full = `${quoteIdent(info.schema)}.${quoteIdent(info.name)}`;
    try {
      let sql: string;
      if (what === "ddl") sql = await meta.ddl(target, info.schema, info.name, info.kind);
      else {
        const d = await meta.table(target, info.schema, info.name);
        const cols = d.columns;
        const pk = cols.filter((c) => c.isPk);
        const keys = pk.length ? pk : cols.slice(0, 1);
        const ph = (c: (typeof cols)[number]) => `NULL /* ${c.name} ${c.dataType} */`;
        const where = keys.map((c) => `${quoteIdent(c.name)} = ${ph(c)}`).join("\n  AND ");
        if (what === "select") sql = `SELECT ${cols.map((c) => `t.${quoteIdent(c.name)}`).join(",\n       ")}\nFROM ${full} t;`;
        else if (what === "insert")
          sql = `INSERT INTO ${full} (${cols.map((c) => quoteIdent(c.name)).join(", ")})\nVALUES (${cols.map(ph).join(",\n        ")});`;
        else if (what === "update")
          sql = `UPDATE ${full}\nSET ${cols.filter((c) => !c.isPk).map((c) => `${quoteIdent(c.name)} = ${ph(c)}`).join(",\n    ")}\nWHERE ${where};`;
        else sql = `DELETE\nFROM ${full}\nWHERE ${where};`;
      }
      openConsole(info.conn, info.db, sql);
    } catch (e) {
      setStatus(`Generate failed: ${String(e)}`);
    }
  };

  // ───────── 데이터 내보내기·가져오기 (형식 고르기 → 하단 진행 막대) ─────────
  const [exportReq, setExportReq] = useState<{ target: string; sql: string; name: string; table: [string, string] | null } | null>(null);
  const [importReq, setImportReq] = useState<{ target: string; schema: string; table?: string; refresh?: () => void } | null>(null);
  const [jobs, setJobs] = useState<TransferJob[]>([]);
  useEffect(() => {
    let off: (() => void) | undefined;
    listen<[string, number, number, number]>("transfer-progress", ({ payload: [id, rows, done, total] }) =>
      setJobs((js) => js.map((j) => (j.id === id ? { ...j, rows, done, total } : j))),
    )
      .then((u) => (off = u))
      .catch(() => {});
    return () => off?.();
  }, []);
  /** 작업을 진행 막대에 올리고 돌린다. 끝나면 내린다 (취소면 조용히) */
  const runJob = async <T,>(label: string, run: (job: string) => Promise<T>): Promise<T | undefined> => {
    const id = crypto.randomUUID();
    setJobs((js) => [...js, { id, label, rows: 0, done: 0, total: 0 }]);
    try {
      return await run(id);
    } catch (e) {
      setStatus(String(e) === "Cancelled" ? `${label}: cancelled` : `${label}: ${String(e)}`);
    } finally {
      setJobs((js) => js.filter((j) => j.id !== id));
    }
  };
  const startExport = async (format: DataFormat, clipboard: boolean) => {
    const r0 = exportReq;
    setExportReq(null);
    if (!r0) return;
    const label = `Export ${r0.name}`;
    const r = await runJob(label, (job) => api.exportData(r0.target, r0.sql, format, clipboard, r0.name, r0.table, job));
    if (!r) return;
    if (r.text !== null) {
      await navigator.clipboard.writeText(r.text);
      setStatus(`Copied ${r.rows} rows to the clipboard`);
    } else setStatus(`Exported ${r.rows} rows to ${r.path}`);
  };
  const startImport = async (table: string, create: boolean, columns: [number, string][], label: string) => {
    const r0 = importReq;
    setImportReq(null);
    if (!r0) return;
    const n = await runJob(`Import ${label}`, (job) => api.importData(r0.target, { path: "", schema: r0.schema, table, create, columns }, job));
    if (n === undefined) return;
    setStatus(`Imported ${n} rows into ${r0.schema}.${table}`);
    if (create) r0.refresh?.();
  };
  const exportTable = (info: NodeInfo) => {
    if (!info.schema || !info.name) return;
    const { q } = dialectOf(info.conn.kind);
    setExportReq({ target: nodeTarget(info), sql: `SELECT * FROM ${q(info.schema)}.${q(info.name)}`, name: info.name, table: [info.schema, info.name] });
  };

  const dangerous = async (info: NodeInfo, verb: "TRUNCATE" | "DROP") => {
    if (!info.schema || !info.name) return;
    const target = targetId(info.conn.id, info.db && info.db !== (info.conn.database || "postgres") ? info.db : undefined);
    const kw = verb === "DROP" ? (info.kind === "view" ? "VIEW" : info.kind === "matview" ? "MATERIALIZED VIEW" : info.kind === "sequence" ? "SEQUENCE" : "TABLE") : "TABLE";
    const { q } = dialectOf(info.conn.kind);
    const sql = `${verb} ${verb === "DROP" ? kw + " " : ""}${q(info.schema)}.${q(info.name)};`;
    if (!(await ask(`[${info.conn.name}] Execute?\n\n${sql}`))) return;
    try {
      await api.executeBatch(target, [sql]);
      setStatus(`Executed: ${sql}`);
      (verb === "DROP" ? info.refreshParent ?? info.refresh : info.refresh)?.();
    } catch (e) {
      setStatus(String(e));
    }
  };

  /** Redis 브라우저 탭 (Medis 식) */
  const openRedis = (c: ConnectionView, db?: string, key?: string) => {
    const database = redisDb(db ?? c.database);
    const target = targetId(c.id, database);
    const existing = tabsRef.current.find((t) => t.kind === "redis" && t.target === target);
    if (existing) {
      if (key) patch(existing.id, { redisKey: key });
      return activate(existing);
    }
    const tab = addTab(c, "redis", { title: `${c.name} [${database}]`, database, group: undefined });
    patch(tab.id, { target, redisKey: key });
  };

  /** DBA 리포트 탭 (같은 접속 대상이면 그 탭에서 리포트만 바꾼다) */
  const openDba = (c: ConnectionView, db?: string, report = "sessions", title = "Sessions") => {
    const database = db && db !== (c.database || "postgres") ? db : undefined;
    const target = targetId(c.id, database);
    const existing = tabsRef.current.find((t) => t.kind === "dba" && t.target === target);
    if (existing) {
      patch(existing.id, { dbaReport: report, title: `${title} — ${c.name}` });
      return activate(existing);
    }
    const tab = addTab(c, "dba", { title: `${title} — ${c.name}`, database: db });
    patch(tab.id, { dbaReport: report });
  };

  /** ERD 탭: 스키마들의 테이블·외래 키 그림 (없으면 이 DB 의 사용자 스키마 전부) */
  const openErd = async (c: ConnectionView, db: string | undefined, schemas?: string[]) => {
    const database = db && db !== (c.database || "postgres") ? db : undefined;
    const target = targetId(c.id, database);
    let list = schemas;
    if (!list) {
      const all = await meta.schemas(target).catch(() => []);
      list = all.filter((x) => !x.isSystem).map((x) => x.name);
    }
    const title = `Diagram: ${list.length === 1 ? list[0] : `${list.length} schemas`} — ${c.name}`;
    const existing = tabsRef.current.find((t) => t.kind === "erd" && t.target === target && (t.erdSchemas ?? []).join(",") === list!.join(","));
    if (existing) return activate(existing);
    const tab = addTab(c, "erd", { title, database: db });
    patch(tab.id, { erdSchemas: list });
  };

  /** Compare & Migrate 탭 (하나만) */
  const openMigrate = (c: ConnectionView, db?: string) => {
    const existing = tabsRef.current.find((t) => t.kind === "migrate");
    if (existing) return activate(existing);
    addTab(c, "migrate", { title: "Compare & Migrate", database: db });
  };

  /** AI Search (RAG) 탭 (하나만). 테이블에서 열면 그 테이블로 새 인덱스 */
  const [ragInit, setRagInit] = useState<{ n: number; connId: string; db?: string; schema?: string; table?: string } | undefined>();
  const openRag = (c: ConnectionView, db?: string, schema?: string, table?: string) => {
    // n 이 바뀌면 탭을 다시 그려 그 테이블로 새 인덱스 양식을 연다
    setRagInit((x) => ({ n: (x?.n ?? 0) + 1, connId: c.id, db, schema, table }));
    const existing = tabsRef.current.find((t) => t.kind === "rag");
    if (existing) return activate(existing);
    addTab(c, "rag", { title: "AI Search (RAG)", database: db });
  };

  /** 트리 노드 더블클릭 / F4 */
  const activateNode = (info: NodeInfo) => {
    if (info.conn.kind === "redis") {
      if (info.kind === "key") return openRedis(info.conn, info.db, info.name);
      if (info.kind === "conn" || info.kind === "db") return openRedis(info.conn, info.db);
    }
    if (isRelation(info.kind) && info.schema && info.name) openTable(info.conn, info.schema, info.name, info.db);
    else if (info.kind === "routine" || info.kind === "sequence") openDdl(info);
  };

  // ───────── Database Explorer 동작 (DataGrip 키맵) ─────────
  const K = { edit: sk("tree.editData"), props: sk("tree.properties"), refresh: sk("tree.refresh"), deact: sk("tree.disconnect"), jump: sk("tree.jumpConsole"), ddl: sk("tree.ddl"), gen: sk("tree.sqlGen"), ddlClip: sk("tree.ddlCopy"), ddlCons: sk("tree.ddlConsole"), ref: sk("tree.copyRef"), doc: sk("tree.quickDoc"), drop: sk("tree.remove"), dup: sk("tree.duplicate"), console: sk("console.new"), force: bindings("tree.refresh")[1] ? formatSpec(bindings("tree.refresh")[1]) : undefined, newK: sk("tree.newDs") };
  const [quickDoc, setQuickDoc] = useState<{ title: string; text: string } | null>(null);
  // ───────── 백업·복원 ─────────
  const [backupReq, setBackupReq] = useState<{ c: ConnectionView; db?: string } | null>(null);
  const dbTarget = (c: ConnectionView, db?: string) => targetId(c.id, db && db !== (c.database || "postgres") ? db : undefined);
  const startBackup = async (o: { format: "custom" | "plain"; schemaOnly: boolean; dataOnly: boolean }) => {
    const r0 = backupReq;
    setBackupReq(null);
    if (!r0) return;
    const name = `${r0.db ?? r0.c.database ?? r0.c.name}-${new Date().toISOString().slice(0, 10)}`;
    const r = await runJob(`Back up ${name}`, (job) => backup.run(dbTarget(r0.c, r0.db), { ...o, schemas: [] }, name, job));
    if (r) setStatus(`Backed up ${r[0] < 1048576 ? `${Math.max(1, Math.round(r[0] / 1024))} KB` : `${(r[0] / 1048576).toFixed(1)} MB`} to ${r[1]}`);
  };
  const startRestore = async (c: ConnectionView, db?: string) => {
    const where = `${c.name}${db ? ` / ${db}` : ""}`;
    if (!(await ask(`Restore a backup into ${where}?\n\nIt runs the backup's statements on this database (in one transaction for PostgreSQL). ${c.options?.confirmChanges ? "\n\n⚠ This is a Production data source." : ""}`))) return;
    const clean = (await ask("Drop the objects the backup contains before restoring them (--clean)?\n\nOK = drop and recreate · Cancel = keep and add"));
    const r = await runJob(`Restore into ${where}`, (job) => backup.restore(dbTarget(c, db), clean, job));
    if (r) setStatus(`Restored ${r} into ${where}`);
  };
  const nodeTarget = (info: NodeInfo) => targetId(info.conn.id, info.db && info.db !== (info.conn.database || "postgres") ? info.db : undefined);
  const fqn = (info: NodeInfo) => [info.db, info.schema, info.name].filter(Boolean).join(".") || info.conn.name;
  const ddlText = (info: NodeInfo) => meta.ddl(nodeTarget(info), info.schema!, info.name!, info.kind);

  const ddlToClipboard = async (info: NodeInfo) => {
    try {
      await navigator.clipboard.writeText(await ddlText(info));
      setStatus(`DDL of ${info.schema}.${info.name} copied to clipboard`);
    } catch (e) {
      setStatus(String(e));
    }
  };
  const ddlToConsole = async (info: NodeInfo) => {
    try {
      openConsole(info.conn, info.db, await ddlText(info));
    } catch (e) {
      setStatus(String(e));
    }
  };
  /** SQL Generator 도구 창: DDL + Copy / Save to File / Run in Console */
  const sqlGenerator = async (info: NodeInfo) => {
    if (!info.schema || !info.name) return;
    try {
      const ddl = await ddlText(info);
      addTab(info.conn, "ddl", { title: `SQL Generator: ${info.name}`, sql: ddl, database: info.db });
    } catch (e) {
      setStatus(String(e));
    }
  };
  const showQuickDoc = async (info: NodeInfo) => {
    if (info.kind === "conn") return setQuickDoc({ title: info.conn.name, text: `${info.conn.user}@${info.conn.host}:${info.conn.port}${info.conn.database ? "/" + info.conn.database : ""}${info.conn.ssh?.enabled ? `\nvia SSH ${info.conn.ssh.user}@${info.conn.ssh.host}` : ""}` });
    if (!info.schema || !info.name) return setQuickDoc({ title: fqn(info), text: info.kind });
    try {
      setQuickDoc({ title: fqn(info), text: await ddlText(info) });
    } catch (e) {
      setQuickDoc({ title: fqn(info), text: String(e) });
    }
  };
  /** Jump to Query Console…: 이 데이터 소스의 콘솔 목록 + New Query Console */
  const jumpToConsole = (info: NodeInfo, x?: number, y?: number) => {
    const consoles = tabsRef.current.filter((t) => t.kind === "console" && t.connId === info.conn.id);
    const el = document.querySelector(".ws-explorer.on .tree-node.selected")?.getBoundingClientRect();
    setTabMenu({
      x: x ?? (el ? el.left + 40 : 300),
      y: y ?? (el ? el.bottom : 200),
      items: [
        { label: "New Query Console", shortcut: K.console, action: () => openConsole(info.conn, info.db) },
        ...(consoles.length ? ([{ separator: true }] as MenuItem[]) : []),
        ...consoles.map((t) => ({ label: t.title, action: () => activate(t) })),
      ],
    });
  };
  const deactivate = async (c: ConnectionView) => {
    const pending = allTabs().filter((t) => t.connId === c.id && t.inTx);
    if (pending.length && !(await ask(`${pending.length} console(s) on ${c.name} have uncommitted changes. Disconnecting rolls them back. Continue?`))) return;
    await api.disconnect(c.id);
    for (const t of pending) updTab(t.id, (x) => ({ ...x, inTx: false }));
    invalidateCompletion(c.id);
    setStatus(`Deactivated ${c.name}`);
    refresh();
  };
  const duplicateConn = async (c: ConnectionView) => {
    const { hasPassword: _h, connected: _c, id: _id, ...config } = c;
    const saved = await api.saveConnection({ ...config, id: "", name: `${c.name} (copy)` });
    await refresh();
    setDialog({ edit: { ...c, ...saved, hasPassword: false, connected: false } });
  };

  /** DataGrip 트리 우클릭 메뉴 (문서 1.11 구조) */
  const buildMenu = (info: NodeInfo): MenuItem[] => {
    const c = info.conn;
    const isObj = !!info.name;
    const rel = isRelation(info.kind);
    const items: MenuItem[] = [
      { label: "New", submenu: [{ label: "Query Console", shortcut: K.console, action: () => openConsole(c, info.db) }, { separator: true }, { label: "Data Source…", action: () => setDialog({}) }] },
    ];
    if (rel) items.push({ label: "Edit Data", shortcut: K.edit, action: () => activateNode(info) });
    if (info.kind === "conn") items.push({ label: "Properties", shortcut: K.props, action: () => setDialog({ edit: c }) });
    items.push({ separator: true });
    items.push({
      label: "Copy/Paste",
      submenu: [
        { label: "Copy Reference", shortcut: K.ref, action: () => navigator.clipboard.writeText(fqn(info)) },
        ...(info.kind === "conn" ? ([{ label: "Duplicate", shortcut: K.dup, action: () => duplicateConn(c) }] as MenuItem[]) : []),
      ],
    });
    items.push({ label: "Refresh", shortcut: K.refresh, action: () => info.refresh?.() });
    if (info.kind === "conn" && c.connected) items.push({ label: "Deactivate", shortcut: K.deact, action: () => deactivate(c) });
    items.push({ label: "Quick Documentation", shortcut: K.doc, action: () => showQuickDoc(info) });
    items.push({ separator: true });
    items.push({
      label: "Navigation",
      submenu: [
        ...(isObj ? ([{ label: "Go to DDL", shortcut: K.ddl, action: () => openDdl(info) }] as MenuItem[]) : []),
        { label: "Jump to Query Console…", shortcut: K.jump, action: () => jumpToConsole(info) },
      ],
    });
    if (info.kind === "table" && c.kind !== "redis" && c.kind !== "mongodb" && !VECTOR_KINDS.includes(c.kind)) items.push({ label: "AI Search (RAG) — Index This Table…", action: () => openRag(c, info.db, info.schema, info.name) });
    if (isObj)
      items.push({
        label: "SQL Scripts",
        submenu: [
          { label: "SQL Generator…", shortcut: K.gen, action: () => sqlGenerator(info) },
          { label: "Generate DDL to Clipboard", shortcut: K.ddlClip, action: () => ddlToClipboard(info) },
          { label: "Generate DDL to Query Console", shortcut: K.ddlCons, action: () => ddlToConsole(info) },
          ...(rel
            ? ([
                { separator: true },
                { label: "SELECT Statement", action: () => generate(info, "select") },
                ...(info.kind === "table" || info.kind === "partitioned"
                  ? ([
                      { label: "INSERT Statement", action: () => generate(info, "insert") },
                      { label: "UPDATE Statement", action: () => generate(info, "update") },
                      { label: "DELETE Statement", action: () => generate(info, "delete") },
                    ] as MenuItem[])
                  : []),
              ] as MenuItem[])
            : []),
        ],
      });
    if (isObj && info.kind !== "routine")
      items.push({
        label: "Object Actions",
        submenu: [
          ...(info.kind === "table" || info.kind === "partitioned" ? ([{ label: "Truncate…", danger: true, action: () => dangerous(info, "TRUNCATE") }] as MenuItem[]) : []),
          { label: "Drop…", shortcut: K.drop, danger: true, action: () => dangerous(info, "DROP") },
        ],
      });
    if (rel)
      items.push({
        label: "Import/Export",
        submenu: [
          { label: "Export Data…", action: () => exportTable(info) },
          ...(info.kind === "table" || info.kind === "partitioned"
            ? ([{ label: "Import Data from File…", action: () => setImportReq({ target: nodeTarget(info), schema: info.schema!, table: info.name }) }] as MenuItem[])
            : []),
        ],
      });
    if (c.kind !== "redis" && (info.kind === "schema" || info.kind === "db" || (info.kind === "conn" && c.kind === "mysql")))
      items.push({ label: "Diagrams", submenu: [{ label: "Show Diagram (ERD)", action: () => openErd(c, info.db, info.kind === "schema" && info.schema ? [info.schema] : c.kind === "mysql" ? [info.db ?? c.database] : undefined) }] });
    if (info.kind === "schema" && info.schema && c.kind !== "redis")
      items.push({ label: "Import/Export", submenu: [{ label: "Import Data into New Table…", action: () => setImportReq({ target: nodeTarget(info), schema: info.schema!, refresh: info.refresh }) }] });
    items.push({
      label: "Tools",
      submenu: [
        { label: "Manage Shown Schemas…", action: () => document.querySelector<HTMLElement>(".ws-explorer.on .tree-node.conn.selected .scope-badge, .ws-explorer.on .tree-node.selected .scope-badge")?.click() },
        ...(c.kind !== "redis" && (info.kind === "conn" || info.kind === "db")
          ? ([
              { separator: true },
              { label: "Sessions & Locks", action: () => openDba(c, info.db, "sessions", "Sessions") },
              { label: "Slow Queries", action: () => openDba(c, info.db, "slow_queries", "Slow Queries (Top 100)") },
              { label: "Table Sizes & Indexes", action: () => openDba(c, info.db, "table_sizes", "Table Sizes") },
              { label: "Server Overview", action: () => openDba(c, info.db, "server", "Server Overview") },
              { label: "Users & Privileges", action: () => openDba(c, info.db, "roles", "Users & Roles") },
              { separator: true },
              { label: "Compare & Migrate (schemas, data, migration files)…", action: () => openMigrate(c, info.db) },
              { label: "AI Search (RAG)…", action: () => openRag(c, info.db) },
              { separator: true },
              { label: "Back Up Database…", action: () => setBackupReq({ c, db: info.db }) },
              { label: "Restore Database…", action: () => startRestore(c, info.db) },
            ] as MenuItem[])
          : []),
      ],
    });
    if (info.kind === "conn") {
      items.push({ label: "Diagnostics", submenu: [{ label: "Force Refresh", shortcut: K.force, action: () => (invalidateSearchIndex(c.id), invalidateCompletion(c.id), info.refresh?.()) }] });
      items.push({ separator: true });
      const others = workspaces.filter((w) => w.id !== curWs && !w.connIds.includes(c.id));
      if (others.length) items.push({ label: "Add to Workspace", submenu: others.map((w) => ({ label: w.name, action: () => setInWs(w.id, c.id, true) })) });
      items.push({ label: "Remove from This Workspace", action: () => setInWs(curWs, c.id, false) });
      items.push({ label: "Remove Data Source", shortcut: K.drop, danger: true, action: () => removeConn(c) });
    }
    return items;
  };

  // ───────── 콘솔 DB / 스키마 선택 (DataGrip 콘솔 오른쪽 위) ─────────
  const [dbCache, setDbCache] = useState<Record<string, string[]>>({});
  const [schemaCache, setSchemaCache] = useState<Record<string, string[]>>({});
  const loadDbList = (connId: string) => {
    if (dbCache[connId]) return;
    meta.databases(connId).then((d) => setDbCache((c) => ({ ...c, [connId]: d })), () => {});
  };
  const loadSchemaList = (target: string) => {
    if (schemaCache[target]) return;
    meta.schemas(target).then((d) => setSchemaCache((c) => ({ ...c, [target]: d.filter((x) => !x.isSystem).map((x) => x.name) })), () => {});
  };
  const switchDatabase = async (tab: Tab, db: string) => {
    const c = conns.find((x) => x.id === tab.connId);
    if (!c) return;
    if (tab.inTx && !(await ask("This console has uncommitted changes. Switching the database rolls them back. Continue?"))) return;
    const database = db !== (c.database || "postgres") ? db : undefined;
    api.closeSession(tab.id).catch(() => {}); // 다음 실행 때 새 DB로 세션을 연다
    patch(tab.id, { database, target: targetId(c.id, database), schema: undefined, inTx: false });
    setStatus(`Console switched to database ${db}`);
  };
  const switchSchema = async (tab: Tab, schema: string) => {
    try {
      const c = conns.find((x) => x.id === tab.connId);
      if (c?.kind === "mysql") {
        // MySQL 은 스키마 = 데이터베이스
        await api.runQuery(tab.target, `USE ${dialectOf("mysql").q(schema)}`, tab.id);
        patch(tab.id, { schema });
        setStatus(`USE ${schema}`);
        return;
      }
      await api.runQuery(tab.target, `SET search_path TO ${dialectOf("postgres").q(schema)}, public`, tab.id);
      patch(tab.id, { schema });
      setStatus(`search_path = ${schema}, public`);
    } catch (e) {
      setStatus(String(e));
    }
  };

  // ───────── 실행 ─────────
  const run = async (tab: Tab, sql: string) => {
    if (!sql.trim()) return;
    patch(tab.id, { running: true, error: null, lastSql: sql });
    setStatus("Executing…");
    try {
      const r = await api.runQuery(tab.target, sql, tab.id);
      patch(tab.id, { running: false, result: r });
      setStatus(r.hasResultSet ? `${r.rows.length}${r.truncated ? "+" : ""} rows retrieved in ${r.elapsedMs} ms` : `${r.rowsAffected} rows affected in ${r.elapsedMs} ms`);
      refresh();
    } catch (e) {
      patch(tab.id, { running: false, error: String(e), result: null });
      setStatus("Query failed");
    }
  };

  // ───────── 콘솔 실행 (문장 여러 개 → 결과 탭 여러 개, Output 로그) ─────────
  const nowTs = () => {
    const d = new Date();
    const p2 = (n: number) => String(n).padStart(2, "0");
    return `${d.getFullYear()}-${p2(d.getMonth() + 1)}-${p2(d.getDate())} ${p2(d.getHours())}:${p2(d.getMinutes())}:${p2(d.getSeconds())}`;
  };
  const updTab = (id: string, fn: (t: Tab) => Tab) => {
    if (!updStashed(id, fn)) setTabs((ts) => ts.map((t) => (t.id === id ? fn(t) : t)));
  };
  const log = (id: string, level: OutputLine["level"], text: string) => updTab(id, (t) => ({ ...t, output: [...t.output.slice(-999), { ts: nowTs(), text, level }] }));
  /** 결과 탭 제목: 쿼리 앞 주석 → 테이블 이름 → Result N (DataGrip) */
  const resultTitle = (sql: string, n: number) => {
    const c = /^\s*(?:--\s*(.+)|\/\*\s*([\s\S]*?)\*\/)/.exec(sql);
    if (c) return (c[1] ?? c[2]).trim().split("\n")[0].slice(0, 40);
    const f = /\bfrom\s+((?:"[^"]+"|\w+)(?:\.(?:"[^"]+"|\w+))?)/i.exec(sql);
    return f ? f[1].replace(/"/g, "") : `Result ${n}`;
  };
  /** 데이터를 바꾸지 않는 문장 (스크립트 중간 실패 안내에서 "이미 커밋된 변경" 을 셀 때) */
  const isReadStmt = (sql: string) => {
    const s2 = sql.replace(/--.*$/gm, "").replace(/\/\*[\s\S]*?\*\//g, "").trim().toLowerCase();
    return /^(select|with|show|explain|values|table|describe|desc)\b/.test(s2) && !/\b(insert|update|delete|merge)\b/.test(s2);
  };

  // 앞 주석을 건너뛴 첫 낱말이 구조를 바꾸는 문장인지 (Production 확인 창에서 "schema change" 로 보인다)
  const DDL_RE = /^(?:\s*(?:--[^\n]*\n|\/\*[\s\S]*?\*\/))*\s*(create|alter|drop|truncate|comment|grant|revoke|rename)\b/i;
  const executeConsole = async (tabId: string, stmts: string[], explain?: "plan" | "analyze") => {
    const tab = tabsRef.current.find((t) => t.id === tabId);
    if (!tab || !stmts.length) return;
    const c = conns.find((x) => x.id === tab.connId);
    // 위험 문장 확인 (WHERE 없는 UPDATE/DELETE, DROP, TRUNCATE — 판정은 core guard 와 같다).
    // Explain Analyze 는 실제로 실행하므로 확인한다 (그냥 Explain 만 건너뛴다)
    if (explain !== "plan") {
      for (const s of stmts) {
        const why = await api.unsafeReason(c?.kind ?? "postgres", s).catch(() => null);
        if (why && !(await ask(`${why}:\n\n${s.slice(0, 300)}\n\nRun it anyway?`))) return;
      }
    }
    // Production: 데이터를 바꾸는 문장은 바뀔 행 수를 세어 보여 주고 묻는다 (세는 것은 SELECT count(*))
    if (c?.options?.confirmChanges && !explain) {
      const writes = stmts.filter((s) => !isReadStmt(s));
      if (writes.length) {
        setStatus("Counting the rows this will change…");
        const lines: string[] = [];
        for (const w of writes) {
          const n = await previewImpact(tab.target, w).catch((e) => `can't count: ${String(e).split("\n")[0]}`);
          lines.push(`${w.replace(/\s+/g, " ").slice(0, 120)}\n   → ${typeof n === "number" ? `${n.toLocaleString("en-US")} row${n === 1 ? "" : "s"}` : n === null ? (DDL_RE.test(w) ? "schema change" : "can't count this statement") : n}`);
        }
        setStatus("Ready");
        if (!(await ask(`Production data source "${c.name}" — this changes data:\n\n${lines.join("\n\n")}\n\nRun it?`))) return;
      }
    }
    // 고정(pin)되지 않은 결과 탭은 새 결과로 바꾼다
    updTab(tabId, (t) => ({ ...t, running: true, results: t.results.filter((r) => r.pinned), cursorEntry: undefined }));
    setStatus("Executing…");
    let n = tab.results.filter((r) => r.pinned).length;
    let lastActive = "output";
    // Auto 모드에서 이미 확정된 변경 문장 수 (중간에 실패하면 알린다)
    let committed = 0;
    for (const sql0 of stmts) {
      const isMy = c?.kind === "mysql";
      const bare = sql0.replace(/;\s*$/, "");
      const sql = !explain ? sql0 : isMy ? `EXPLAIN ${explain === "analyze" ? "ANALYZE " : ""}${bare}` : `EXPLAIN (FORMAT JSON${explain === "analyze" ? ", ANALYZE, BUFFERS" : ""}) ${bare}`;
      log(tabId, "sql", `${c?.name ?? ""}> ${sql0}`);
      // Explain Analyse 는 문장을 실제로 실행한다 → 바꾸는 문장은 트랜잭션 안에서 돌리고 항상 되돌린다
      // (이미 트랜잭션 중이면 세이브포인트로 그 부분만)
      const dry = explain === "analyze" && !isReadStmt(sql0);
      const inTx0 = !!tabsRef.current.find((t) => t.id === tabId)?.inTx;
      try {
        let r: QueryResult;
        if (dry) {
          await api.runQuery(tab.target, inTx0 ? "SAVEPOINT qurry_explain" : "BEGIN", tabId);
          try {
            r = await api.runQuery(tab.target, sql, tabId);
          } finally {
            await api.runQuery(tab.target, inTx0 ? "ROLLBACK TO SAVEPOINT qurry_explain" : "ROLLBACK", tabId).catch((e) => log(tabId, "error", `Roll back after Explain Analyse failed: ${String(e)}`));
          }
          r = { ...r, inTransaction: inTx0 };
          log(tabId, "info", "Explain Analyse ran the statement in a transaction and rolled it back — nothing was changed");
        } else {
          r = await api.runQuery(tab.target, sql, tabId);
        }
        pushHistory({ connId: tab.connId, database: tab.database, sql: sql0 });
        updTab(tabId, (t) => ({ ...t, inTx: !!r.inTransaction }));
        if (!r.inTransaction && !isReadStmt(sql0) && !dry) committed++;
        if (explain && !isMy) {
          const plan = await analyzePlan(String(r.rows[0]?.[0] ?? "")).catch(() => null);
          const entry: ResultEntry = { id: crypto.randomUUID(), title: resultTitle(sql0, ++n), sql: sql0, kind: "plan", result: null, plan: plan ?? undefined, pinned: false };
          updTab(tabId, (t) => ({ ...t, results: [...t.results, entry] }));
          lastActive = entry.id;
          log(tabId, "info", `Plan retrieved in ${r.elapsedMs} ms`);
        } else if (r.hasResultSet) {
          const entry: ResultEntry = { id: crypto.randomUUID(), title: resultTitle(sql0, ++n), sql: sql0, kind: "result", result: r, pinned: false };
          updTab(tabId, (t) => ({ ...t, results: [...t.results, entry], cursorEntry: r.truncated ? entry.id : t.cursorEntry }));
          lastActive = entry.id;
          log(tabId, "info", `${r.rows.length}${r.truncated ? "+" : ""} row${r.rows.length === 1 ? "" : "s"} retrieved starting from 1 in ${r.elapsedMs} ms`);
        } else {
          log(tabId, "info", `${r.rowsAffected} row${r.rowsAffected === 1 ? "" : "s"} affected in ${r.elapsedMs} ms`);
        }
      } catch (e) {
        log(tabId, "error", String(e));
        if (committed > 0) log(tabId, "error", `Note: ${committed} earlier statement${committed > 1 ? "s" : ""} of this run ${committed > 1 ? "were" : "was"} already committed (Auto mode) and ${committed > 1 ? "were" : "was"} not rolled back`);
        // 오류가 나도 트랜잭션이 열려 있을 수 있다 (Manual 첫 문장 실패 등) → Commit/Roll back 버튼을 맞춘다
        api.txState(tabId).then((inTx) => updTab(tabId, (t) => ({ ...t, inTx })), () => {});
        lastActive = "output";
        updTab(tabId, (t) => ({ ...t, running: false, activeResult: "output" }));
        setStatus("Query failed");
        refresh();
        return;
      }
    }
    updTab(tabId, (t) => ({ ...t, running: false, activeResult: lastActive, lastSql: stmts[stmts.length - 1] }));
    setStatus(`${stmts.length} statement${stmts.length > 1 ? "s" : ""} executed`);
    refresh();
  };

  const fetchMoreEntry = async (tabId: string, entryId: string) => {
    const tab = tabsRef.current.find((t) => t.id === tabId);
    if (!tab || tab.cursorEntry !== entryId) {
      // 서버 커서는 마지막 결과에만 열려 있다
      updTab(tabId, (t) => ({ ...t, results: t.results.map((r) => (r.id === entryId && r.result ? { ...r, result: { ...r.result, truncated: false } } : r)) }));
      return;
    }
    const shown = tab.results.find((x) => x.id === entryId)?.result?.rows.length ?? 0;
    if (shown >= MAX_ROWS) return setStatus(capMsg(shown));
    updTab(tabId, (t) => ({ ...t, loadingMore: true }));
    try {
      const r = await api.fetchMore(tabId);
      updTab(tabId, (t) => ({
        ...t,
        loadingMore: false,
        cursorEntry: r.truncated ? entryId : undefined,
        results: t.results.map((x) => (x.id === entryId && x.result ? { ...x, result: { ...x.result, rows: [...x.result.rows, ...r.rows], truncated: r.truncated } } : x)),
      }));
    } catch (e) {
      updTab(tabId, (t) => ({ ...t, loadingMore: false, cursorEntry: undefined }));
      setStatus(String(e));
    }
  };

  const setTxMode = async (tab: Tab, manual: boolean) => {
    try {
      // 커밋 안 한 변경이 있으면 먼저 묻는다 (예전엔 말없이 커밋했다)
      if (!manual && tab.inTx) {
        if (!(await ask("This console has uncommitted changes.\n\nOK = commit them and switch to Auto\nCancel = stay in Manual"))) return;
        await api.commitTx(tab.id);
        log(tab.id, "info", "Transaction committed");
      }
      await api.setTxMode(tab.target, tab.id, manual);
      updTab(tab.id, (t) => ({ ...t, txManual: manual, inTx: manual ? t.inTx : false }));
      log(tab.id, "info", `Transaction mode: ${manual ? "Manual" : "Auto"}`);
    } catch (e) {
      setStatus(String(e));
    }
  };
  const txEnd = async (tab: Tab, what: "commit" | "rollback") => {
    try {
      if (what === "commit") await api.commitTx(tab.id);
      else await api.rollbackTx(tab.id);
      updTab(tab.id, (t) => ({ ...t, inTx: false }));
      log(tab.id, "info", what === "commit" ? "Transaction committed" : "Transaction rolled back");
    } catch (e) {
      log(tab.id, "error", String(e));
    }
  };

  const applyTableFilter = (tab: Tab, where: string, orderBy: string) => {
    if (!tab.edit) return;
    const { q } = dialectOf(conns.find((x) => x.id === tab.connId)?.kind);
    let sql = `SELECT * FROM ${q(tab.edit.schema)}.${q(tab.edit.table)}`;
    if (where) sql += ` WHERE ${where}`;
    if (orderBy) sql += ` ORDER BY ${orderBy}`;
    sql += ";";
    patch(tab.id, { where, orderBy, tableSql: sql });
    // PK 를 못 받은 탭(접속 실패로 열린 탭)은 같이 다시 받는다
    if (tab.edit.pk.length) run({ ...tab, tableSql: sql }, sql);
    else loadTable({ ...tab, tableSql: sql });
  };

  const fetchMore = async (tab: Tab) => {
    const shown = tab.result?.rows.length ?? 0;
    if (shown >= MAX_ROWS) return setStatus(capMsg(shown));
    patch(tab.id, { loadingMore: true });
    try {
      const r = await api.fetchMore(tab.id);
      setTabs((ts) =>
        ts.map((t) => (t.id === tab.id && t.result ? { ...t, loadingMore: false, result: { ...t.result, rows: [...t.result.rows, ...r.rows], truncated: r.truncated } } : t)),
      );
      setStatus(`${(tab.result?.rows.length ?? 0) + r.rows.length}${r.truncated ? "+" : ""} rows retrieved`);
    } catch (e) {
      setTabs((ts) => ts.map((t) => (t.id === tab.id && t.result ? { ...t, loadingMore: false, result: { ...t.result, truncated: false } } : t)));
      setStatus(String(e));
    }
  };

  const cancel = async (tab: Tab) => {
    try {
      await api.cancelQuery(tab.id);
      setStatus("Cancel requested");
    } catch (e) {
      setStatus(String(e));
    }
  };

  const submitEdits = async (tab: Tab, statements: string[]) => {
    // 각 UPDATE/DELETE 가 정확히 1행에 적용돼야 저장 (아니면 전부 롤백)
    const n = await api.executeBatch(tab.target, statements, true);
    setStatus(`Submitted ${statements.length} statements, ${n} rows affected`);
    if (tab.lastSql) run(tab, tab.lastSql);
  };

  // ───────── 탭 닫기 / 분할 ─────────
  const closeTabs = async (ids: string[]) => {
    if (!ids.length) return;
    // 닫으면 롤백된다 → 커밋 안 한 변경이 있으면 묻는다
    const pending = tabsRef.current.filter((t) => ids.includes(t.id) && t.inTx);
    if (pending.length && !(await ask(`${pending.length === 1 ? `"${pending[0].title}" has` : `${pending.length} consoles have`} uncommitted changes. Roll them back and close?`))) return;
    for (const t of tabsRef.current.filter((x) => ids.includes(x.id))) closedStack.current = [snapshot(t), ...closedStack.current].slice(0, 30);
    mru.current = mru.current.filter((x) => !ids.includes(x));
    for (const id of ids) {
      forgetConsoleModel(id);
      editorApis.current.delete(id);
      liveSql.current.delete(id);
      api.closeSession(id).catch(() => {});
    }
    setTabs((ts) => {
      const rest = ts.filter((t) => !ids.includes(t.id));
      settle(rest, panesRef.current);
      return rest;
    });
  };

  /** 탭이 바뀐 뒤: 빈 칸을 없애고 (옆 칸이 채운다) 칸마다 보이는 탭과 포커스를 맞춘다. show = 그 칸에 그 탭을 보인다 */
  const settle = (next: Tab[], pane: Pane, show?: { g: Group; id: string }) => {
    const p = prunePanes(pane, new Set(next.map((t) => t.group)));
    const ls = leaves(p);
    setPanes(p);
    setActiveIds((a) =>
      a.map((cur, g) => {
        if (!ls.includes(g)) return null;
        if (show?.g === g) return show.id;
        if (cur && next.some((t) => t.id === cur && t.group === g)) return cur;
        return mru.current.find((id) => next.some((t) => t.id === id && t.group === g)) ?? last(next.filter((t) => t.group === g))?.id ?? null;
      }),
    );
    setFocused((f) => (show && ls.includes(show.g) ? show.g : ls.includes(f) ? f : ls[0]));
  };

  /** 탭을 다른 칸으로 */
  const moveTo = (id: string, g: Group) =>
    setTabs((ts) => {
      const next = ts.map((t) => (t.id === id ? { ...t, group: g } : t));
      settle(next, panesRef.current, { g, id });
      return next;
    });

  /** 탭이 있는 칸을 나누고 탭을 새 칸으로 (Split and Move Right / Down) */
  const splitAndMove = (t: Tab, dir: "right" | "down") => {
    const r = splitPane(panesRef.current, t.group, dir);
    if (!r) return setStatus(`Up to ${MAX_PANES} panes`);
    setTabs((ts) => {
      const next = ts.map((x) => (x.id === t.id ? { ...x, group: r.g } : x));
      settle(next, r.pane, { g: r.g, id: t.id });
      return next;
    });
  };

  // ───────── 탭 드래그 앤 드롭 (DataGrip: 순서 변경 / 다른 그룹으로 이동 / 가장자리로 끌어 분할) ─────────
  const [dragId, setDragId] = useState<string | null>(null);
  const [dropHint, setDropHint] = useState<{ g: Group; zone: "tabs" | "center" | "right" | "down"; beforeId: string | null } | null>(null);

  const dropTab = (id: string, g: Group, beforeId: string | null, dir?: "right" | "down") => {
    // 가장자리에 놓으면 그 칸을 나눠 새 칸으로
    let pane = panesRef.current;
    if (dir) {
      const r = splitPane(pane, g, dir);
      if (!r) return setStatus(`Up to ${MAX_PANES} panes`);
      pane = r.pane;
      g = r.g;
    }
    setTabs((ts) => {
      const t = ts.find((x) => x.id === id);
      if (!t || beforeId === id) return ts;
      const rest = ts.filter((x) => x.id !== id);
      let idx = beforeId ? rest.findIndex((x) => x.id === beforeId) : -1;
      if (idx < 0) {
        const lastIdx = rest.map((x) => x.group).lastIndexOf(g);
        idx = lastIdx < 0 ? rest.length : lastIdx + 1;
      }
      const next = [...rest.slice(0, idx), { ...t, group: g }, ...rest.slice(idx)];
      settle(next, pane, { g, id });
      return next;
    });
  };

  const endDrag = () => {
    setDragId(null);
    setDropHint(null);
  };

  const unsplit = () => {
    setTabs((ts) => ts.map((t) => ({ ...t, group: 0 })));
    setPanes(leaf(0));
    setFocused(0);
    setActiveIds((a) => [a[focused] ?? a.find(Boolean) ?? null, null, null, null]);
  };

  /** Reopen Closed Tab / Recent Files 에서 다시 열기 */
  const reopen = (snap: TabSnapshot) => {
    const c = conns.find((x) => x.id === snap.connId);
    if (!c) return;
    const open = tabsRef.current.find((t) => snapKey(t) === snap.key);
    if (open) return activate(open);
    if (snap.kind === "table" && snap.edit) {
      openTableTab(c, snap.edit.schema, snap.edit.table, snap.database, undefined, snap.where, snap.orderBy);
    } else addTab(c, snap.kind, { title: snap.title, sql: snap.sql, database: snap.database });
  };
  const reopenClosed = () => {
    const snap = closedStack.current.shift();
    if (snap) reopen(snap);
  };

  /** Split Right / Down: 같은 대상을 독립된 탭으로 복제해서 그 탭의 칸을 나눈 오른쪽(아래) 칸에 연다 (최대 4칸) */
  const splitClone = (t: Tab, dir: "right" | "down") => {
    const c = conns.find((x) => x.id === t.connId);
    if (!c) return;
    const r = splitPane(panesRef.current, t.group, dir);
    if (!r) return setStatus(`Up to ${MAX_PANES} panes`);
    setPanes(r.pane);
    if (t.kind === "table" && t.edit) openTableTab(c, t.edit.schema, t.edit.table, t.database, r.g, t.where, t.orderBy);
    else addTab(c, t.kind, { title: t.kind === "console" ? `${t.title} (copy)` : t.title, sql: sqlOf(t), database: t.database, group: r.g });
  };

  const togglePin = (t: Tab) => {
    setTabs((ts) => {
      const flipped = ts.map((x) => (x.id === t.id ? { ...x, pinned: !x.pinned } : x));
      // 고정 탭은 그룹 안에서 왼쪽으로 (Keep pinned tabs on the left)
      return [...flipped.filter((x) => x.pinned), ...flipped.filter((x) => !x.pinned)];
    });
  };

  const openTabMenu = (e: React.MouseEvent, t: Tab) => {
    e.preventDefault();
    e.stopPropagation();
    activate(t);
    const sameGroup = tabs.filter((x) => x.group === t.group);
    const idx = sameGroup.findIndex((x) => x.id === t.id);
    const ls = leaves(panes);
    const other: Group = ls[(ls.indexOf(t.group) + 1) % ls.length];
    const canSplit = ls.length < MAX_PANES;
    const c = conns.find((x) => x.id === t.connId);
    const items: MenuItem[] = [
      { label: "Close", shortcut: sk("tab.close"), action: () => closeTabs([t.id]) },
      { label: "Close Other Tabs", action: () => closeTabs(sameGroup.filter((x) => x.id !== t.id && !x.pinned).map((x) => x.id)) },
      { label: "Close All but Pinned", action: () => closeTabs(sameGroup.filter((x) => !x.pinned).map((x) => x.id)) },
      { label: "Close Tabs to the Left", action: () => closeTabs(sameGroup.slice(0, idx).filter((x) => !x.pinned).map((x) => x.id)), disabled: idx === 0 },
      { label: "Close Tabs to the Right", action: () => closeTabs(sameGroup.slice(idx + 1).filter((x) => !x.pinned).map((x) => x.id)), disabled: idx === sameGroup.length - 1 },
      { label: "Close All Tabs", action: () => closeTabs(sameGroup.map((x) => x.id)) },
      { label: "Reopen Closed Tab", action: reopenClosed, disabled: closedStack.current.length === 0 },
      { separator: true },
      { label: t.pinned ? "Unpin Tab" : "Pin Tab", action: () => togglePin(t) },
      {
        label: "Copy Path/Reference…",
        submenu: [
          { label: "Copy Reference", action: () => navigator.clipboard.writeText(t.edit ? `${t.edit.schema}.${t.edit.table}` : t.title) },
          ...(c ? ([{ label: "Copy Data Source Name", action: () => navigator.clipboard.writeText(c.name) }] as MenuItem[]) : []),
        ],
      },
      { separator: true },
      { label: "Split Right", action: () => splitClone(t, "right"), disabled: !canSplit },
      { label: "Split Down", action: () => splitClone(t, "down"), disabled: !canSplit },
      { label: "Split and Move Right", action: () => splitAndMove(t, "right"), disabled: !canSplit || sameGroup.length < 2 },
      { label: "Split and Move Down", action: () => splitAndMove(t, "down"), disabled: !canSplit || sameGroup.length < 2 },
      ...(split
        ? ([
            { label: "Move To Next Group", action: () => moveTo(t.id, other) },
            { label: "Open In Next Group", action: () => { const cc = c; if (!cc) return; if (t.kind === "table" && t.edit) openTableTab(cc, t.edit.schema, t.edit.table, t.database, other, t.where, t.orderBy); else addTab(cc, t.kind, { title: t.title, sql: sqlOf(t), database: t.database, group: other }); } },
            { label: "Unsplit All", action: unsplit },
          ] as MenuItem[])
        : []),
      { separator: true },
      {
        label: "Move to Workspace",
        submenu: [
          ...workspaces.filter((w) => w.id !== curWs).map((w) => ({ label: w.name, action: () => moveToWs(t, w.id) })),
          ...(workspaces.length > 1 ? ([{ separator: true }] as MenuItem[]) : []),
          {
            label: "New Workspace…",
            action: async () => {
              const name = (await askText("Workspace name", `Workspace ${workspacesRef.current.length + 1}`));
              if (!name?.trim()) return;
              const w = { id: crypto.randomUUID(), name: name.trim(), connIds: [] };
              setWorkspaces((l) => [...l, w]);
              workspacesRef.current = [...workspacesRef.current, w];
              moveToWs(t, w.id);
            },
          },
        ],
      },
    ];
    if (t.kind === "console" && c)
      items.push(
        { separator: true },
        { label: "Duplicate", action: () => addTab(c, "console", { title: `${t.title} (copy)`, sql: sqlOf(t), database: t.database, group: t.group }) },
        { label: "Rename File…", shortcut: "⇧F6", action: () => renameConsole(t) },
      );
    setTabMenu({ x: e.clientX, y: e.clientY, items });
  };

  /** Refresh (⌥⌘y, Find Action): Data Source 목록 + 검색·자동완성 캐시 */
  const refreshAll = () => {
    invalidateSearchIndex();
    for (const c of conns) invalidateCompletion(c.id);
    refresh();
  };

  const renameConsole = async (t: Tab) => {
    const name = (await askText("Rename console", t.title));
    if (name && name.trim()) patch(t.id, { title: name.trim() });
  };

  // ───────── Switcher / Recent Files / Maximize ─────────
  const [switcher, setSwitcher] = useState<{ items: SwitchItem[]; start: number } | null>(null);
  const [recentOpen, setRecentOpen] = useState(false);
  const [maximized, setMaximized] = useState(false);
  const kindIcon = (k: Tab["kind"]) => (k === "table" ? "▦" : k === "ddl" ? "ƒ" : k === "redis" ? "◆" : "⌘");
  const hintOf = (connId: string, db?: string) => {
    const c = conns.find((x) => x.id === connId);
    return c ? c.name + (db ? ` · ${db}` : "") : "";
  };
  const openSwitcher = (back: boolean) => {
    const ts = tabsRef.current;
    const ordered = [...mru.current.map((id) => ts.find((t) => t.id === id)).filter((t): t is Tab => !!t), ...ts.filter((t) => !mru.current.includes(t.id))];
    if (!ordered.length) return;
    const items = ordered.map((t) => ({ key: t.id, icon: kindIcon(t.kind), title: t.title, hint: hintOf(t.connId, t.database), color: conns.find((c) => c.id === t.connId)?.color }));
    setSwitcher({ items, start: back ? items.length - 1 : 1 });
  };
  const recentItems = (): SwitchItem[] => {
    const openKeys = new Map(tabsRef.current.map((t) => [snapKey(t), t] as const));
    return recent.current
      .filter((r) => conns.some((c) => c.id === r.connId))
      .map((r) => ({ key: r.key, icon: kindIcon(r.kind), title: r.title, hint: hintOf(r.connId, r.database), open: openKeys.has(r.key), color: conns.find((c) => c.id === r.connId)?.color }));
  };

  // ───────── Search Everywhere (⇧⇧) / Go to Database Object (⌘o) / Find Action (⇧⌘A) ─────────
  const [se, setSe] = useState<SeTab | null>(null);

  // ── Tool windows: Database Explorer (⌘1), Find (⌘3) ──
  const [explorerHidden, setExplorerHidden] = useState(false);
  const toggleExplorer = () => {
    const tree = document.querySelector<HTMLElement>(".ws-explorer.on [data-tree]");
    const focusedInTree = !!document.activeElement?.closest?.("[data-tree]");
    if (explorerHidden || maximized) {
      setExplorerHidden(false);
      setMaximized(false);
      setTimeout(() => document.querySelector<HTMLElement>(".ws-explorer.on [data-tree]")?.focus(), 50);
    } else if (focusedInTree) setExplorerHidden(true);
    else tree?.focus();
  };
  const [findOpen, setFindOpen] = useState(false);
  const [findRuns, setFindRuns] = useState<FindRun[]>([]);
  const [findActive, setFindActive] = useState<string | null>(null);
  const [ftsScope, setFtsScope] = useState<FtsScope | null>(null);
  const openFts = async () => {
    const sel = treeSel;
    const c = sel?.conn ?? conns.find((x) => x.id === active?.connId);
    if (!c) return setStatus("Select a data source, database, schema or table to search in");
    const db = sel?.db ?? active?.database ?? (c.database || "postgres");
    if (sel && isRelation(sel.kind) && sel.schema && sel.name)
      return setFtsScope({ connId: c.id, connName: c.name, db, schemas: [], tables: [[sel.schema, sel.name]] });
    if (sel?.kind === "schema" && sel.schema) return setFtsScope({ connId: c.id, connName: c.name, db, schemas: [sel.schema], tables: [] });
    try {
      const schemas = (await meta.schemas(targetId(c.id, db))).filter((s) => !s.isSystem).map((s) => s.name);
      setFtsScope({ connId: c.id, connName: c.name, db, schemas, tables: [] });
    } catch (e) {
      setStatus(String(e));
    }
  };
  const runFts = async (scope: FtsScope, req: FtsRequest) => {
    setFtsScope(null);
    const id = crypto.randomUUID();
    const title = `"${req.text}"`;
    setFindRuns((rs) => [...rs.filter((r) => r.pinned), { id, title, scope, running: true, hits: [], text: req.text, matchCase: req.matchCase }]);
    setFindActive(id);
    setFindOpen(true);
    try {
      const hits = await meta.fullTextSearch(targetId(scope.connId, scope.db), req);
      setFindRuns((rs) => rs.map((r) => (r.id === id ? { ...r, running: false, hits } : r)));
      setStatus(`Full-text search: ${hits.reduce((n, h) => n + h.result.rows.length, 0)} matches in ${hits.length} tables`);
    } catch (e) {
      setFindRuns((rs) => rs.map((r) => (r.id === id ? { ...r, running: false, error: String(e) } : r)));
    }
  };

  // ── Editor font size ──
  const [, setFontTick] = useState(editorFontSize());
  const bumpFont = (d: number) => setFontTick(setEditorFontSize(d === 0 ? DEFAULT_FONT : editorFontSize() + d));
  const objNode = (o: DbObject): NodeInfo | null => {
    const conn = conns.find((c) => c.id === o.connId);
    if (!conn) return null;
    const kind = (o.kind === "function" || o.kind === "procedure" ? "routine" : o.kind) as NodeInfo["kind"];
    return { kind, conn, db: o.db, schema: o.schema, name: o.kind === "schema" ? undefined : o.name };
  };
  const seFiles = () => {
    const openKeys = new Map(tabsRef.current.map((t) => [snapKey(t), t] as const));
    const seen = new Set<string>();
    const items = [];
    for (const id of mru.current) {
      const t = tabsRef.current.find((x) => x.id === id);
      if (t) {
        seen.add(snapKey(t));
        items.push({ key: snapKey(t), icon: kindIcon(t.kind), title: t.title, hint: hintOf(t.connId, t.database), open: true });
      }
    }
    for (const r of recent.current)
      if (!seen.has(r.key) && conns.some((c) => c.id === r.connId))
        items.push({ key: r.key, icon: kindIcon(r.kind), title: r.title, hint: hintOf(r.connId, r.database), open: openKeys.has(r.key) });
    return items;
  };
  const seActions = (): ActionItem[] => {
    const t = active;
    const c = treeSel?.conn ?? conns.find((x) => x.id === t?.connId);
    return [
      { name: "New Query Console", shortcut: sk("console.new"), group: "File", run: () => c && openConsole(c, treeSel?.db ?? t?.database), disabled: !c },
      { name: "New Data Source…", group: "File", run: () => setDialog({}) },
      { name: "New Workspace…", group: "Window", run: () => newWs() },
      ...workspaces.map((w, i) => ({ name: `Workspace: ${w.name}`, shortcut: i < 9 ? sk(`ws.${i + 1}`) : undefined, group: "Window", run: () => switchWs(w.id), disabled: w.id === curWs })),
      { name: "Data Source Properties", shortcut: sk("tree.properties"), group: "File", run: () => c && setDialog({ edit: c }), disabled: !c },
      { name: "Export Data Sources…", group: "File", run: () => exportConns() },
      { name: "Import Data Sources…", group: "File", run: () => importConns() },
      { name: "Settings…", shortcut: sk("settings"), group: "File", run: () => setSettingsOpen("appearance") },
      { name: "Keymap Reference", shortcut: sk("keymap"), group: "Help", run: () => setKeymapOpen(true) },
      { name: "Edit Keymap…", group: "Help", run: () => setSettingsOpen("keymap") },
      { name: "Theme: Dark", group: "Appearance", run: () => setThemePref("dark") },
      { name: "Theme: Light", group: "Appearance", run: () => setThemePref("light") },
      { name: "Theme: High contrast", group: "Appearance", run: () => setThemePref("hc") },
      { name: "Theme: Sync with OS", group: "Appearance", run: () => setThemePref("system") },
      { name: "Quick Switch Theme…", shortcut: sk("theme"), group: "Appearance", run: () => setSchemeOpen(true) },
      { name: "Execute", shortcut: sk("editor.run"), group: "Run", run: () => t && editorApis.current.get(t.id)?.run(), disabled: t?.kind !== "console" },
      { name: "Explain Plan", group: "Run", run: () => t && editorApis.current.get(t.id)?.run("plan"), disabled: t?.kind !== "console" },
      { name: "Explain Analyse", group: "Run", run: () => t && editorApis.current.get(t.id)?.run("analyze"), disabled: t?.kind !== "console" },
      { name: "Cancel Running Statements", shortcut: sk("editor.cancel"), group: "Run", run: () => t && cancel(t), disabled: !t?.running },
      { name: "Commit", group: "Transaction", run: () => t && txEnd(t, "commit"), disabled: !t?.inTx },
      { name: "Roll Back", group: "Transaction", run: () => t && txEnd(t, "rollback"), disabled: !t?.inTx },
      { name: "Transaction Mode: Manual", group: "Transaction", run: () => t && setTxMode(t, true), disabled: t?.kind !== "console" },
      { name: "Transaction Mode: Auto", group: "Transaction", run: () => t && setTxMode(t, false), disabled: t?.kind !== "console" },
      { name: "Browse Query History…", shortcut: sk("editor.history"), group: "Run", run: () => t && (setHistoryTab("history"), setHistoryFor(t.id)), disabled: t?.kind !== "console" },
      { name: "Snippets…", group: "Run", run: () => t && (setHistoryTab("snippets"), setHistoryFor(t.id)), disabled: t?.kind !== "console" },
      {
        name: "Save Query as Snippet…",
        group: "Run",
        run: async () => {
          const c = t && conns.find((x) => x.id === t.connId);
          const sql = t && editorApis.current.get(t.id)?.text();
          if (c && sql && (await saveSnippet(sql, c.id, c.name))) setStatus("Snippet saved (⌘k › Snippets…)");
        },
        disabled: t?.kind !== "console",
      },
      { name: "Go to Database Object…", shortcut: sk("goto.object"), group: "Navigate", run: () => setTimeout(() => setSe("database")) },
      { name: "Go to Symbol…", shortcut: sk("goto.symbol"), group: "Navigate", run: () => setTimeout(() => setSe("symbols")) },
      { name: "Go to File…", shortcut: sk("goto.file"), group: "Navigate", run: () => setTimeout(() => setSe("files")) },
      { name: "Full-text Search…", shortcut: sk("fts"), group: "Navigate", run: () => openFts() },
      { name: "Recent Files", shortcut: sk("recent"), group: "Navigate", run: () => setRecentOpen(true) },
      { name: "Switcher", shortcut: "⌃⇥", group: "Navigate", run: () => openSwitcher(false) },
      { name: "Reopen Closed Tab", shortcut: sk("tab.reopen"), group: "Window", run: reopenClosed },
      { name: "Close Tab", shortcut: sk("tab.close"), group: "Window", run: () => t && closeTabs([t.id]), disabled: !t },
      { name: "Close All Tabs", group: "Window", run: () => closeTabs(tabsRef.current.map((x) => x.id)) },
      { name: "Split Right", group: "Window", run: () => t && splitClone(t, "right"), disabled: !t },
      { name: "Split Down", group: "Window", run: () => t && splitClone(t, "down"), disabled: !t },
      { name: "Unsplit All", group: "Window", run: unsplit, disabled: !split },
      { name: "Maximize Editor", shortcut: sk("maximize"), group: "Window", run: () => setMaximized((m) => !m) },
      { name: "Database Explorer", shortcut: sk("explorer"), group: "Tool Windows", run: () => toggleExplorer() },
      { name: "Find (Full-text Search results)", shortcut: sk("find.window"), group: "Tool Windows", run: () => setFindOpen((o) => !o) },
      { name: "Refresh", shortcut: sk("refresh"), group: "Database", run: () => refreshAll() },
      { name: "Increase Editor Font Size", shortcut: sk("font.bigger"), group: "Editor", run: () => bumpFont(1) },
      { name: "Decrease Editor Font Size", shortcut: sk("font.smaller"), group: "Editor", run: () => bumpFont(-1) },
      { name: "Reset Editor Font Size", group: "Editor", run: () => bumpFont(0) },
    ];
  };

  // 키 핸들러는 한 번만 등록되므로 최신 함수는 ref 로 넘긴다
  const latest = useRef({ toggleExplorer, openFts, bumpFont, setSe, setFindOpen, findOpen, switchWs });
  latest.current = { toggleExplorer, openFts, bumpFont, setSe, setFindOpen, findOpen, switchWs };
  const lastShift = useRef(0);

  // ───────── 단축키 (DataGrip 키맵) ─────────
  const keyState = useRef({ treeSel, active, focused, tabs, split });
  keyState.current = { treeSel, active, focused, tabs, split };
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      // Switcher / Recent Files 팝업이 떠 있으면 팝업이 키를 처리한다
      if (document.querySelector(".switcher, .search-everywhere, .modal.fts, .modal.keymap, .keymap-recording")) return;
      const { treeSel, active, tabs } = keyState.current;
      const L = latest.current;
      // ⇧⇧: Search Everywhere
      if (e.key === "Shift") {
        if (!e.repeat) {
          const now = Date.now();
          if (now - lastShift.current < 400) {
            lastShift.current = 0;
            L.setSe("all");
          } else lastShift.current = now;
        }
        return;
      }
      lastShift.current = 0;
      // 명령 단축키는 lib/shortcuts.ts (Settings › Keymap 에서 바꾼다)
      const spec = eventSpec(e);
      const is = (id: string) => hit(spec, id);
      if (is("goto.object")) return e.preventDefault(), L.setSe("database");
      if (is("goto.symbol")) return e.preventDefault(), L.setSe("symbols");
      if (is("goto.file")) return e.preventDefault(), L.setSe("files");
      if (is("find.action")) return e.preventDefault(), L.setSe("actions");
      if (is("fts")) return e.preventDefault(), void L.openFts();
      if (is("explorer")) return e.preventDefault(), L.toggleExplorer();
      // ⌥1~9: 워크스페이스
      for (let n = 1; n <= 9; n++) if (is(`ws.${n}`)) return e.preventDefault(), void (workspacesRef.current[n - 1] && L.switchWs(workspacesRef.current[n - 1].id));
      if (is("find.window")) return e.preventDefault(), L.setFindOpen((o) => !o);
      // ⇧⎋ Hide Active Tool Window
      if (e.shiftKey && e.key === "Escape") {
        const ae = document.activeElement;
        if (ae?.closest?.("[data-toolwindow=find]")) return e.preventDefault(), L.setFindOpen(false);
        if (ae?.closest?.("[data-tree]")) return e.preventDefault(), L.toggleExplorer();
      }
      if (is("font.bigger")) return e.preventDefault(), L.bumpFont(1);
      if (is("font.smaller")) return e.preventDefault(), L.bumpFont(-1);
      const inTree = (document.activeElement as HTMLElement | null)?.closest?.("[data-tree]");
      const inInput = ["INPUT", "TEXTAREA", "SELECT"].includes((document.activeElement as HTMLElement | null)?.tagName ?? "");
      if (is("console.new")) {
        e.preventDefault();
        const conn = treeSel?.conn ?? conns.find((c) => c.id === active?.connId);
        if (conn) openConsole(conn, treeSel?.db ?? active?.database);
        return;
      }
      if (inTree && treeSel && !inInput) {
        const act = (fn: () => void) => {
          e.preventDefault();
          e.stopPropagation();
          fn();
        };
        // Edit Data (데이터 소스에서는 기본 콘솔)
        if (is("tree.editData"))
          return act(() => ((treeSel.kind === "conn" || treeSel.kind === "db") && treeSel.conn.kind !== "redis" ? jumpToConsole(treeSel) : activateNode(treeSel)));
        if (is("tree.properties") && treeSel.kind === "conn") return act(() => setDialog({ edit: treeSel.conn }));
        if (is("tree.refresh")) return act(() => treeSel.refresh?.());
        if (is("tree.disconnect") && treeSel.conn.connected) return act(() => deactivate(treeSel.conn));
        if (is("tree.jumpConsole")) return act(() => jumpToConsole(treeSel));
        if (is("tree.sqlGen") && treeSel.name) return act(() => sqlGenerator(treeSel));
        if (is("tree.ddlCopy") && treeSel.name) return act(() => ddlToClipboard(treeSel));
        if (is("tree.ddlConsole") && treeSel.name) return act(() => ddlToConsole(treeSel));
        if (is("tree.quickDoc")) return act(() => showQuickDoc(treeSel));
        if (is("tree.newDs")) return act(() => setDialog({}));
        if (is("tree.duplicate") && treeSel.kind === "conn") return act(() => duplicateConn(treeSel.conn));
        if (is("tree.remove"))
          return act(() => (treeSel.kind === "conn" ? removeConn(treeSel.conn) : treeSel.name ? dangerous(treeSel, "DROP") : undefined));
      }
      if (is("tree.ddl") && inTree && treeSel && treeSel.name) {
        e.preventDefault();
        openDdl(treeSel);
        return;
      }
      if (is("tree.copyRef") && inTree && treeSel) {
        e.preventDefault();
        navigator.clipboard.writeText(treeSel.name ? `${treeSel.schema}.${treeSel.name}` : treeSel.schema ?? treeSel.db ?? treeSel.conn.name);
        return;
      }
      if (is("refresh")) {
        e.preventDefault();
        if (inTree && treeSel?.refresh) treeSel.refresh();
        else if (active?.kind === "table") applyTableFilter(active, active.where, active.orderBy);
        else refreshAll();
        return;
      }
      if (is("editor.history") && active?.kind === "console") {
        e.preventDefault();
        setHistoryTab("history");
        setHistoryFor(active.id);
        return;
      }
      if (is("editor.cancel") && !inTree && active?.running) {
        e.preventDefault();
        cancel(active);
        return;
      }
      // ⇧F6 = 콘솔 이름 바꾸기 (탭 메뉴 Rename File… 과 같다)
      if (e.key === "F6" && e.shiftKey && !e.metaKey && !e.ctrlKey && !e.altKey && active?.kind === "console") {
        e.preventDefault();
        renameConsole(active);
        return;
      }
      if (is("tab.close") && active) {
        e.preventDefault();
        closeTabs([active.id]);
        return;
      }
      // ⌃⇥ / ⌃⇧⇥: Switcher (최근 사용 순)
      if (e.ctrlKey && !e.metaKey && e.key === "Tab") {
        e.preventDefault();
        e.stopPropagation();
        openSwitcher(e.shiftKey);
        return;
      }
      if (is("keymap")) {
        e.preventDefault();
        setKeymapOpen(true);
        return;
      }
      if (is("settings")) {
        e.preventDefault();
        setSettingsOpen("appearance");
        return;
      }
      if (is("theme")) {
        e.preventDefault();
        setSchemeOpen(true);
        return;
      }
      if (is("recent")) {
        e.preventDefault();
        e.stopPropagation();
        setRecentOpen(true);
        return;
      }
      if ((is("tab.next") || is("tab.prev")) && active) {
        e.preventDefault();
        const g = tabs.filter((t) => t.group === active.group);
        const i = g.findIndex((t) => t.id === active.id);
        const n = g[(i + (is("tab.prev") ? -1 : 1) + g.length) % g.length];
        if (n) activate(n);
        return;
      }
      if (is("tab.reopen")) {
        e.preventDefault();
        reopenClosed();
        return;
      }
      if (is("maximize")) {
        e.preventDefault();
        setMaximized((m) => !m);
        return;
      }
      // 분할된 에디터 그룹 사이 이동 (Goto Next Splitter)
      if (is("split.other") && keyState.current.split) {
        e.preventDefault();
        // 다음 칸으로 (⇧ 이면 이전 칸)
        const ls = leaves(panesRef.current);
        const g: Group = ls[(ls.indexOf(keyState.current.focused) + (e.shiftKey ? -1 : 1) + ls.length) % ls.length];
        const id = activeIdsRef.current[g];
        const t = tabs.find((x) => x.id === id);
        if (t) activate(t);
        return;
      }
      // F4 / Enter 를 입력칸에서 누른 건 무시
      if (inInput) return;
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [conns]);

  // ───────── 기타 ─────────
  const setColor = async (c: ConnectionView, color: string | null) => {
    const { hasPassword: _h, connected: _c, ...config } = c;
    await api.saveConnection({ ...config, color });
    refresh();
  };

  const setScope = async (c: ConnectionView, scope: NonNullable<ConnectionView["scope"]>) => {
    const { hasPassword: _h, connected: _c, ...config } = c;
    await api.saveConnection({ ...config, scope });
    refresh();
  };

  const exportConns = async () => {
    try {
      const r = await api.exportConnections();
      if (r) setStatus(`Exported ${r[0]} data sources (passwords excluded) to ${r[1]}`);
    } catch (e) {
      setStatus(String(e));
    }
  };

  const importConns = async () => {
    try {
      // 남이 준 파일은 Read-only 를 끄거나 접속 시 실행할 SQL 을 넣을 수 있다 → 바뀌는 내용을 보여 주고 확인
      const preview = await api.pickImportFile();
      if (!preview) return;
      if (!preview.length) return setStatus("Nothing to import");
      if (!(await ask(`Import these data sources?\n\n${preview.join("\n")}`))) return;
      const r = await api.importPicked();
      setStatus(`Imported: ${r.added} added, ${r.updated} updated (re-enter passwords)`);
      refresh();
    } catch (e) {
      setStatus(String(e));
    }
  };

  const exportCsv = (tab: Tab | null) => {
    if (!tab?.lastSql) return;
    setExportReq({ target: tab.target, sql: tab.lastSql, name: tab.kind === "table" ? tab.title : "result", table: tab.edit ? [tab.edit.schema, tab.edit.table] : null });
  };

  const removeConn = async (c: ConnectionView) => {
    if (!(await ask(`Delete data source '${c.name}'?`))) return;
    await api.deleteConnection(c.id);
    closeTabs(tabsRef.current.filter((t) => t.connId === c.id).map((t) => t.id));
    refresh();
  };

  // ───────── 에디터 그룹 렌더링 ─────────
  /** 분할 트리: 가지는 나란히(좌우/상하), 잎은 칸. 맨 바깥 비율만 저장한다 */
  const renderPanes = (p: Pane, path: string): React.ReactNode =>
    p.t === "leaf" ? (
      renderGroup(p.g)
    ) : (
      <PanelGroup key={path} direction={p.dir === "right" ? "horizontal" : "vertical"} autoSaveId={path === "" ? `dbtool-split-${p.dir}` : undefined}>
        <Panel minSize={15}>{renderPanes(p.a, path + "a")}</Panel>
        <PanelResizeHandle className={p.dir === "right" ? "resize-h" : "resize-v"} />
        <Panel minSize={15}>{renderPanes(p.b, path + "b")}</Panel>
      </PanelGroup>
    );
  const renderGroup = (g: Group) => {
    const gTabs = tabs.filter((t) => t.group === g);
    const ga = groupActive(g);
    const gConn = conns.find((c) => c.id === ga?.connId);
    return (
      <div className={"panel editor-group" + (split && focused === g ? " focused" : "")} onMouseDownCapture={() => setFocused(g)}>
        <div
          className={"tabs" + (dropHint?.zone === "tabs" && dropHint.g === g && dropHint.beforeId === null ? " drop-end" : "")}
          onDragOver={(e) => {
            if (!dragId) return;
            e.preventDefault();
            setDropHint({ g, zone: "tabs", beforeId: null });
          }}
          onDrop={(e) => {
            e.preventDefault();
            if (dragId) dropTab(dragId, g, null);
            endDrag();
          }}
        >
          {gTabs.map((t) => {
            const color = conns.find((c) => c.id === t.connId)?.color;
            return (
              <div
                key={t.id}
                className={
                  "tab" +
                  (t.id === ga?.id ? " active" : "") +
                  (t.id === dragId ? " dragging" : "") +
                  (dropHint?.zone === "tabs" && dropHint.g === g && dropHint.beforeId === t.id ? " drop-before" : "")
                }
                style={color ? { background: connBg(color, theme) } : undefined}
                draggable
                onDragStart={(e) => {
                  e.dataTransfer.setData("text/plain", t.id);
                  e.dataTransfer.effectAllowed = "move";
                  setDragId(t.id);
                }}
                onDragEnd={endDrag}
                onDragOver={(e) => {
                  if (!dragId) return;
                  e.preventDefault();
                  e.stopPropagation();
                  const r = e.currentTarget.getBoundingClientRect();
                  const i = gTabs.findIndex((x) => x.id === t.id);
                  const beforeId = e.clientX < r.left + r.width / 2 ? t.id : gTabs[i + 1]?.id ?? null;
                  setDropHint({ g, zone: "tabs", beforeId });
                }}
                onDrop={(e) => {
                  e.preventDefault();
                  e.stopPropagation();
                  if (dragId && dropHint) dropTab(dragId, g, dropHint.beforeId);
                  endDrag();
                }}
                onMouseDown={(e) => {
                  if (e.button === 1) closeTabs([t.id]);
                  else activate(t);
                }}
                onContextMenu={(e) => openTabMenu(e, t)}
                onDoubleClick={() => setMaximized((m) => !m)}
                title={t.kind === "table" ? `${t.edit?.schema}.${t.edit?.table}` : t.title}
              >
                <span className={"tab-icon " + t.kind}>{t.kind === "table" ? "▦" : t.kind === "ddl" ? "ƒ" : t.kind === "redis" ? "◆" : t.kind === "dba" ? "◉" : t.kind === "erd" ? "⧉" : t.kind === "migrate" ? "⇄" : t.kind === "rag" ? "✦" : "⌘"}</span>
                <span>{t.title}</span>
                {t.pinned ? (
                  <span
                    className="tab-pin"
                    title="Unpin Tab"
                    onMouseDown={(e) => e.stopPropagation()}
                    onClick={(e) => {
                      e.stopPropagation();
                      togglePin(t);
                    }}
                  >
                    📌
                  </span>
                ) : (
                  <span
                    className="tab-close"
                    title={`Close. Alt-Click to Close Others`}
                    onMouseDown={(e) => e.stopPropagation()}
                    onClick={(e) => {
                      e.stopPropagation();
                      // ⌥+클릭: 이 탭만 남기고 닫기
                      if (e.altKey) closeTabs(gTabs.filter((x) => x.id !== t.id && !x.pinned).map((x) => x.id));
                      else closeTabs([t.id]);
                    }}
                  >
                    ×
                  </span>
                )}
              </div>
            );
          })}
        </div>
        <div className="work-area" style={gConn?.color ? { boxShadow: `inset 0 2px 0 ${gConn.color}` } : undefined}>
          {/* 탭을 끌고 있을 때: 가운데 = 이 그룹으로, 오른쪽/아래 가장자리 = 분할 */}
          {dragId && (
            <div
              className="drop-overlay"
              onDragOver={(e) => {
                e.preventDefault();
                const r = e.currentTarget.getBoundingClientRect();
                const fx = (e.clientX - r.left) / r.width;
                const fy = (e.clientY - r.top) / r.height;
                // 칸의 오른쪽 / 아래 가장자리 = 그 칸을 나눈다 (최대 4칸)
                const canSplit = leaves(panes).length < MAX_PANES;
                const zone = canSplit && fx > 0.7 ? "right" : canSplit && fy > 0.7 ? "down" : "center";
                setDropHint({ g, zone, beforeId: null });
              }}
              onDragLeave={() => setDropHint((h) => (h && h.g === g && h.zone !== "tabs" ? null : h))}
              onDrop={(e) => {
                e.preventDefault();
                const h = dropHint;
                if (h && h.g === g) {
                  if (h.zone === "right" || h.zone === "down") {
                    // 제 칸에 하나뿐인 탭은 나눠도 그대로라 무시
                    if (!(gTabs.length === 1 && gTabs[0].id === dragId)) dropTab(dragId, g, null, h.zone);
                  } else dropTab(dragId, g, null);
                }
                endDrag();
              }}
            >
              {dropHint?.g === g && dropHint.zone !== "tabs" && <div className={"drop-zone " + dropHint.zone} />}
            </div>
          )}
          {/* SQL 콘솔 (DataGrip: 콘솔 툴바 + 에디터 + 결과 탭/Output) */}
          {!gTabs.length && (
            <div className="empty center">
              Right-click a data source → <b>New Query Console</b> ({isMac ? "⇧⌘L" : "Ctrl+Shift+L"}).
              <br />
              <span className="muted">Double-click a table ({isMac ? "⌘↓" : "F4"}) to edit data, {isMac ? "⌘b" : "Ctrl+b"} for DDL, {isMac ? "⌘↩" : "Ctrl+Enter"} to execute.</span>
              <br />
              <span className="muted">All shortcuts: <b>Keymap Reference</b> ({sk("keymap")}) · Search Everywhere ({isMac ? "⇧⇧" : "Shift Shift"})</span>
            </div>
          )}
          {gTabs
            .filter((t) => t.kind === "console")
            .map((t) => {
              const c = conns.find((x) => x.id === t.connId);
              return (
                <div key={t.id} className="work-layer" style={{ display: t.id === ga?.id ? "block" : "none" }}>
                  <ConsolePane
                    tabId={t.id}
                    target={t.target}
                    initialSql={sqlOf(t)}
                    connName={c?.name ?? ""}
                    database={t.database || c?.database || (c?.kind === "mysql" ? "" : "postgres")}
                    schema={t.schema ?? (c?.kind === "mysql" ? t.database || c?.database || "" : "public")}
                    dialect={dialectOf(c?.kind)}
                    dbList={dbCache[t.connId] ?? []}
                    schemaList={schemaCache[t.target] ?? []}
                    running={t.running}
                    loadingMore={t.loadingMore}
                    results={t.results}
                    activeResult={t.activeResult}
                    output={t.output}
                    txManual={t.txManual}
                    inTx={t.inTx}
                    onChange={(sql) => liveSql.current.set(t.id, sql)}
                    onExecute={(stmts, explain) => executeConsole(t.id, stmts, explain)}
                    onCancel={() => cancel(t)}
                    onSetActiveResult={(id) => patch(t.id, { activeResult: id })}
                    onPin={(id, pinned) => setTabs((ts) => ts.map((x) => (x.id === t.id ? { ...x, results: x.results.map((r) => (r.id === id ? { ...r, pinned } : r)) } : x)))}
                    onCloseResult={(id) =>
                      setTabs((ts) => ts.map((x) => (x.id === t.id ? { ...x, results: x.results.filter((r) => r.id !== id), activeResult: x.activeResult === id ? "output" : x.activeResult } : x)))
                    }
                    onFetchMore={(entryId) => fetchMoreEntry(t.id, entryId)}
                    onExport={(sql) => exportCsv({ ...t, lastSql: sql })}
                    onHistory={() => (setHistoryTab("history"), setHistoryFor(t.id))}
                    onTxMode={(manual) => setTxMode(t, manual)}
                    onCommit={() => txEnd(t, "commit")}
                    onRollback={() => txEnd(t, "rollback")}
                    onLoadDbs={() => loadDbList(t.connId)}
                    onLoadSchemas={() => loadSchemaList(t.target)}
                    onSwitchDb={(db) => switchDatabase(t, db)}
                    onSwitchSchema={(sc) => switchSchema(t, sc)}
                    schemaSwitchDisabled={conns.find((x) => x.id === t.connId)?.options?.switchSchema === "disable"}
                    registerApi={(id, a) => editorApis.current.set(id, a)}
                    onClearOutput={() => patch(t.id, { output: [] })}
                  />
                </div>
              );
            })}
          {/* AI Search (RAG) */}
          {gTabs
            .filter((t) => t.kind === "rag")
            .map((t) => (
              <div key={t.id} className="work-layer" style={{ display: t.id === ga?.id ? "block" : "none" }}>
                <RagView key={ragInit?.n} conns={conns} initial={ragInit} runJob={runJob} onStatus={setStatus} />
              </div>
            ))}
          {/* 비교·마이그레이션 */}
          {gTabs
            .filter((t) => t.kind === "migrate")
            .map((t) => (
              <div key={t.id} className="work-layer" style={{ display: t.id === ga?.id ? "block" : "none" }}>
                <MigrateView
                  conns={conns}
                  initialConnId={t.connId}
                  initialDb={t.database}
                  onOpenConsole={(connId, db, sql) => {
                    const c = conns.find((x) => x.id === connId);
                    if (c) openConsole(c, db, sql);
                  }}
                  onStatus={setStatus}
                />
              </div>
            ))}
          {/* ERD */}
          {gTabs
            .filter((t) => t.kind === "erd")
            .map((t) => {
              const c = conns.find((x) => x.id === t.connId);
              return (
                <div key={t.id} className="work-layer" style={{ display: t.id === ga?.id ? "block" : "none" }}>
                  <ErdView target={t.target} schemas={t.erdSchemas ?? []} visible={t.id === ga?.id} onOpenTable={(sc, tb) => c && openTable(c, sc, tb, t.database)} onStatus={setStatus} />
                </div>
              );
            })}
          {/* DBA 리포트 */}
          {gTabs
            .filter((t) => t.kind === "dba")
            .map((t) => {
              const c = conns.find((x) => x.id === t.connId);
              return (
                <div key={t.id} className="work-layer" style={{ display: t.id === ga?.id ? "block" : "none" }}>
                  <DbaReport
                    target={t.target}
                    connName={c?.name ?? ""}
                    report={t.dbaReport ?? "sessions"}
                    readOnly={!!c?.readOnly}
                    dialect={dialectOf(c?.kind ?? "postgres")}
                    visible={t.id === ga?.id}
                    onReport={(id, title) => c && patch(t.id, { dbaReport: id, title: `${title} — ${c.name}` })}
                    onOpenConsole={(sql) => c && openConsole(c, t.database, sql)}
                    onStatus={setStatus}
                  />
                </div>
              );
            })}
          {/* Redis 브라우저 */}
          {gTabs
            .filter((t) => t.kind === "redis")
            .map((t) => {
              const c = conns.find((x) => x.id === t.connId);
              return (
                <div key={t.id} className="work-layer" style={{ display: t.id === ga?.id ? "block" : "none" }}>
                  <RedisBrowser
                    target={t.target}
                    connName={c?.name ?? ""}
                    database={t.database ?? "db0"}
                    dbList={dbCache[t.connId] ?? []}
                    readOnly={!!c?.readOnly}
                    initialKey={t.redisKey}
                    onLoadDbs={() => loadDbList(t.connId)}
                    onSwitchDb={(db) => c && patch(t.id, { database: db, target: targetId(c.id, db), title: `${c.name} [${db}]`, redisKey: undefined })}
                    onStatus={setStatus}
                  />
                </div>
              );
            })}
          {/* 테이블 데이터 에디터 */}
          {gTabs
            .filter((t) => t.kind === "table")
            .map((t) => (
              <div key={t.id} className="work-layer" style={{ display: t.id === ga?.id ? "block" : "none" }}>
                <TableEditor
                  title={t.title}
                  where={t.where}
                  orderBy={t.orderBy}
                  running={t.running}
                  error={t.error}
                  result={t.result}
                  query={t.lastSql}
                  edit={t.edit && t.lastSql === t.tableSql ? t.edit : undefined}
                  readOnly={!!conns.find((x) => x.id === t.connId)?.readOnly}
                  dialect={dialectOf(conns.find((x) => x.id === t.connId)?.kind)}
                  loadingMore={t.loadingMore}
                  onApply={(w, o) => applyTableFilter(t, w, o)}
                  onCancel={() => cancel(t)}
                  onFetchMore={() => fetchMore(t)}
                  onSubmit={(st) => submitEdits(t, st)}
                  onExportCsv={() => exportCsv(t)}
                  onDdl={() => {
                    const c = conns.find((x) => x.id === t.connId);
                    if (c && t.edit) openDdl({ kind: "table", conn: c, db: t.database, schema: t.edit.schema, name: t.edit.table });
                  }}
                  onCopyQueryToConsole={(sql) => { const c = conns.find((x) => x.id === t.connId); if (c) openConsole(c, t.database, sql); }}
                  onFindSimilar={
                    conns.find((x) => x.id === t.connId)?.kind === "postgres"
                      ? async (column, row) => {
                          const c = conns.find((x) => x.id === t.connId);
                          if (!c || !t.edit) return;
                          try {
                            const sql = await meta.similarRowsSql(t.target, t.edit.schema, t.edit.table, column, t.edit.pk.map((k) => [k, row[k] === null ? null : String(row[k])]));
                            openConsole(c, t.database, sql);
                            setStatus("Similar-rows query opened in a new console");
                          } catch (e) {
                            setStatus(String(e));
                          }
                        }
                      : undefined
                  }
                />
              </div>
            ))}
          {/* DDL (읽기 전용) */}
          {gTabs
            .filter((t) => t.kind === "ddl")
            .map((t) => (
              <div key={t.id} className="work-layer ddl-layer" style={{ display: t.id === ga?.id ? "flex" : "none" }}>
                <div className="dg-toolbar">
                  <span className="tb-info" style={{ marginLeft: 0 }}>{t.title.startsWith("SQL Generator") ? "SQL Generator" : "DDL"} · read-only</span>
                  <span className="grow" />
                  <button className="tb text" title="Copy to Clipboard" onClick={() => { navigator.clipboard.writeText(t.sql); setStatus("Copied to clipboard"); }}>Copy</button>
                  <button
                    className="tb text"
                    title="Save to File"
                    onClick={async () => {
                      try {
                        const path = await api.saveTextFile(`${t.title.replace(/^SQL Generator: | \[DDL\]$/g, "")}.sql`, t.sql);
                        if (path) setStatus(`Saved ${path}`);
                      } catch (e) {
                        setStatus(String(e));
                      }
                    }}
                  >
                    Save to File
                  </button>
                  <button className="tb text" title="Run in a console" onClick={() => { const c = conns.find((x) => x.id === t.connId); if (c) openConsole(c, t.database, t.sql); }}>Run in Console</button>
                </div>
                <div className="ddl-editor">
                <ConsoleEditor
                  tabId={t.id}
                  connId={t.target}
                  initialSql={t.sql}
                  language={dialectOf(conns.find((x) => x.id === t.connId)?.kind).monaco}
                  readOnly
                  onChange={() => {}}
                  onRun={() => {}}
                  registerApi={() => {}}
                />
                </div>
              </div>
            ))}
        </div>
      </div>
    );
  };

  // Search Everywhere 의 파일·액션 목록은 열 때 한 번 만든다 (열려 있는 동안 App 이 다시 그려질 때마다 새로 만들면 안쪽 memo 가 매번 깨진다)
  // eslint-disable-next-line react-hooks/exhaustive-deps
  const seLists = useMemo(() => (se ? { files: seFiles(), actions: seActions() } : null), [se]);

  return (
    <div className="app">
      <div className="toolbar" style={activeConn?.color ? { boxShadow: `inset 4px 0 0 ${activeConn.color}` } : undefined}>
        <span className="brand">Qurry</span>
        <span className="sep" />
        <button
          className="run"
          disabled={!active || active.kind !== "console" || active.running}
          onClick={() => active && editorApis.current.get(active.id)?.run()}
          title={`Execute (${isMac ? "⌘↩" : "Ctrl+Enter"})`}
        >
          ▶
        </button>
        <button className="stop" disabled={!active?.running} onClick={() => active && cancel(active)} title="Cancel Running Statements">
          ■
        </button>
        <button className="icon tb-settings" title={`Settings (${sk("settings")})`} onClick={() => setSettingsOpen("appearance")}>⚙</button>
        <button className="icon tb-settings" title={`Keymap Reference (${isMac ? "⇧⌘/" : "Ctrl+Shift+/"})`} onClick={() => setKeymapOpen(true)}>⌨</button>
        <span className="tb-conn">
          {activeConn ? `${activeConn.name} · ${activeConn.user}@${activeConn.host} · ${active?.database || activeConn.database || "postgres"}` : "No console"}
        </span>
      </div>

      <div className="ws-row">
      {!maximized && (
        <WorkspaceBar
          list={workspaces}
          current={curWs}
          counts={Object.fromEntries(workspaces.map((w) => [w.id, w.id === curWs ? tabs.length : stash.current.get(w.id)?.tabs.length ?? 0]))}
          shortcut={(n) => (isMac ? `⌥${n}` : `Alt+${n}`)}
          onSwitch={switchWs}
          onNew={newWs}
          onRename={renameWs}
          onDelete={deleteWs}
        />
      )}
      <PanelGroup direction="vertical" className="main" autoSaveId="dbtool-v">
      <Panel id="top" order={1} minSize={20}>
      <PanelGroup direction="horizontal" autoSaveId="dbtool-h">
        {!maximized && !explorerHidden && (
        <Panel id="explorer" order={1} defaultSize={20} minSize={12}>
          {workspaces
            .filter((w) => shownWs.includes(w.id))
            .map((w) => {
              const mine = w.connIds.map((id) => conns.find((c) => c.id === id)).filter((c): c is ConnectionView => !!c);
              return (
                <div key={w.id} className={"ws-explorer" + (w.id === curWs ? " on" : "")} style={{ display: w.id === curWs ? "block" : "none" }}>
                  <Explorer
                    connections={mine}
                    others={conns.filter((c) => !w.connIds.includes(c.id))}
                    onAddExisting={(id) => setInWs(w.id, id, true)}
                    onAdd={() => setDialog({})}
                    onRefresh={refresh}
                    onExport={exportConns}
                    onImport={importConns}
                    onSelect={setTreeSel}
                    onActivate={activateNode}
                    buildMenu={buildMenu}
                    onSetColor={setColor}
                    onScopeChange={setScope}
                  />
                </div>
              );
            })}
        </Panel>
        )}
        {!maximized && !explorerHidden && <PanelResizeHandle className="resize-h" />}
        <Panel id="editors" order={2}>
          {renderPanes(panes, "")}
        </Panel>
      </PanelGroup>
      </Panel>
      {findOpen && !maximized && (
        <>
          <PanelResizeHandle className="resize-v" />
          <Panel id="find" order={2} defaultSize={30} minSize={10}>
            <FindPanel
              runs={findRuns}
              active={findActive}
              onActive={setFindActive}
              onClose={() => setFindOpen(false)}
              onCloseRun={(id) => setFindRuns((rs) => rs.filter((r) => r.id !== id))}
              onPin={(id) => setFindRuns((rs) => rs.map((r) => (r.id === id ? { ...r, pinned: !r.pinned } : r)))}
              onOpen={(run, hit) => {
                const c = conns.find((x) => x.id === run.scope.connId);
                if (c) openTableTab(c, hit.schema, hit.table, run.scope.db, undefined, hit.whereClause);
              }}
            />
          </Panel>
        </>
      )}
      </PanelGroup>
      </div>

      {switcher && (
        <Switcher
          items={switcher.items}
          start={switcher.start}
          onClose={() => setSwitcher(null)}
          onPick={(id) => {
            setSwitcher(null);
            const t = tabsRef.current.find((x) => x.id === id);
            if (t) activate(t);
          }}
        />
      )}
      {schemeOpen && (
        <RecentFiles
          title="Theme"
          start={["dark", "light", "hc", "system"].indexOf(getThemePref())}
          items={[
            { key: "dark", icon: getThemePref() === "dark" ? "●" : "", title: "Dark", hint: "" },
            { key: "light", icon: getThemePref() === "light" ? "●" : "", title: "Light", hint: "" },
            { key: "hc", icon: getThemePref() === "hc" ? "●" : "", title: "High contrast", hint: "" },
            { key: "system", icon: getThemePref() === "system" ? "●" : "", title: "Sync with OS", hint: `currently ${THEME_LABEL[theme]}` },
          ]}
          onClose={() => setSchemeOpen(false)}
          onPick={(k) => {
            setSchemeOpen(false);
            setThemePref(k as ThemePref);
          }}
        />
      )}
      {settingsOpen && (
        <SettingsDialog
          initialPage={settingsOpen}
          onClose={() => setSettingsOpen(false)}
          onUpdate={(u) => {
            setUpdate(u);
            setSettingsOpen(false);
            setUpdateOpen(true);
          }}
        />
      )}
      {backupReq && <BackupDialog what={backupReq.db ?? backupReq.c.name} pg={backupReq.c.kind === "postgres"} onRun={startBackup} onClose={() => setBackupReq(null)} />}
      {exportReq && <ExportDialog what={exportReq.name} onExport={startExport} onClose={() => setExportReq(null)} />}
      {importReq && <ImportDialog target={importReq.target} schema={importReq.schema} table={importReq.table} onImport={startImport} onClose={() => setImportReq(null)} />}
      <TextDialog />
      {updateOpen && update?.latest && <UpdateDialog info={update} onClose={() => setUpdateOpen(false)} beforeInstall={readyToQuit} />}
      {keymapOpen && <KeymapDialog onClose={() => setKeymapOpen(false)} onEdit={() => (setKeymapOpen(false), setSettingsOpen("keymap"))} />}
      {se && (
        <SearchEverywhere
          initialTab={se}
          conns={conns}
          contextConnId={treeSel?.conn.id ?? active?.connId}
          files={seLists?.files ?? []}
          actions={seLists?.actions ?? []}
          onClose={() => setSe(null)}
          onSelectInTree={(o) => {
            setExplorerHidden(false);
            setMaximized(false);
            revealInTree({ connId: o.connId, db: o.db, schema: o.schema, name: o.kind === "schema" ? undefined : o.name, kind: o.kind });
          }}
          onOpenData={(o) => {
            const n = objNode(o);
            if (n) activateNode(n);
          }}
          onDdl={(o) => {
            const n = objNode(o);
            if (n) openDdl(n);
          }}
          onQuickDoc={(o) => {
            const n = objNode(o);
            if (n) showQuickDoc(n);
          }}
          onOpenFile={(key) => {
            const snap = recent.current.find((r) => r.key === key);
            const open = tabsRef.current.find((t) => snapKey(t) === key);
            if (open) activate(open);
            else if (snap) reopen(snap);
          }}
        />
      )}
      {ftsScope && <FullTextSearchDialog scope={ftsScope} onClose={() => setFtsScope(null)} onSearch={(req) => runFts(ftsScope, req)} />}
      {recentOpen && (
        <RecentFiles
          items={recentItems()}
          onClose={() => setRecentOpen(false)}
          onPick={(key) => {
            setRecentOpen(false);
            const snap = recent.current.find((r) => r.key === key);
            if (snap) reopen(snap);
          }}
        />
      )}
      <StatusBar status={status} readOnly={!!activeConn?.readOnly} update={update?.latest?.version} onUpdate={() => setUpdateOpen(true)} jobs={jobs} />

      {tabMenu && <ContextMenu menu={tabMenu} onClose={() => setTabMenu(null)} />}

      {historyFor && (() => {
        const t = tabs.find((x) => x.id === historyFor);
        const c = conns.find((x) => x.id === t?.connId);
        return t && c ? (
          <HistoryDialog
            connId={c.id}
            title={`${c.name} Query History`}
            initialTab={historyTab}
            onPick={(sql) => {
              editorApis.current.get(t.id)?.insert(sql);
              setHistoryFor(null);
            }}
            onClose={() => setHistoryFor(null)}
          />
        ) : null;
      })()}

      {quickDoc && (
        <div className="modal-backdrop light" onMouseDown={() => setQuickDoc(null)} onKeyDown={(e) => e.key === "Escape" && setQuickDoc(null)}>
          <div className="quick-doc" onMouseDown={(e) => e.stopPropagation()} tabIndex={-1} ref={(el) => el?.focus()} onKeyDown={(e) => e.key === "Escape" && setQuickDoc(null)}>
            <div className="qd-title">{quickDoc.title}</div>
            <pre className="qd-body">{quickDoc.text}</pre>
          </div>
        </div>
      )}

      {dialog && (
        <ConnectionDialog
          edit={dialog.edit}
          onClose={() => setDialog(null)}
          onSaved={async (saved) => {
            setDialog(null);
            await refresh();
            try {
              setStatus(`Connecting to ${saved.name}…`);
              const v = await api.connect(saved.id);
              setStatus(`Connected: ${v.split(" on ")[0]}`);
            } catch (e) {
              setStatus(`Connection failed: ${String(e)}`);
            }
            refresh();
          }}
        />
      )}
    </div>
  );
}

/** Settings › Appearance */
function SettingsDialog({ onClose, onUpdate, initialPage = "appearance" }: { onClose: () => void; onUpdate: (u: UpdateInfo) => void; initialPage?: "appearance" | "keymap" }) {
  const [page, setPage] = useState(initialPage);
  const [checkOnStart, setCheckOnStart] = useState(updateCheckEnabled());
  const [updMsg, setUpdMsg] = useState<string | null>(null);
  const checkNow = async () => {
    setUpdMsg("Checking…");
    try {
      const u = await api.checkUpdate(true);
      if (u.latest) onUpdate(u);
      else setUpdMsg(`You have the latest version (${u.current}).`);
    } catch (e) {
      setUpdMsg(String(e));
    }
  };
  const [pref, setPref] = useState<ThemePref>(getThemePref());
  const theme = useTheme();
  const pick = (p: ThemePref) => {
    setPref(p);
    setThemePref(p);
  };
  const sync = pref === "system";
  return (
    <div className="modal-backdrop" onMouseDown={onClose}>
      <div className={"modal settings" + (page === "keymap" ? " wide" : "")} onMouseDown={(e) => e.stopPropagation()}>
        <div className="modal-title">Settings</div>
        <div className="settings-body">
          <div className="settings-nav">
            <div className={page === "appearance" ? "on" : ""} onClick={() => setPage("appearance")}>Appearance</div>
            <div className={page === "keymap" ? "on" : ""} onClick={() => setPage("keymap")}>Keymap</div>
          </div>
          {page === "keymap" && <KeymapEditor />}
          {page === "appearance" && <div className="form settings-form">
            <label>Theme</label>
            <div className="row">
              <select value={sync ? theme : pref} disabled={sync} onChange={(e) => pick(e.target.value as ThemePref)}>
                <option value="dark">Dark</option>
                <option value="light">Light</option>
                <option value="hc">High contrast</option>
              </select>
              <label className="check">
                <input type="checkbox" checked={sync} onChange={(e) => pick(e.target.checked ? "system" : theme)} />
                Sync with OS
              </label>
            </div>
            <label>Updates</label>
            <div className="row">
              <label className="check">
                <input
                  type="checkbox"
                  checked={checkOnStart}
                  onChange={(e) => {
                    setCheckOnStart(e.target.checked);
                    setUpdateCheckEnabled(e.target.checked);
                  }}
                />
                Check for updates automatically
              </label>
              <button onClick={checkNow}>Check Now</button>
            </div>
            {updMsg && (
              <>
                <span />
                <span className="hint" style={{ margin: 0 }}>{updMsg}</span>
              </>
            )}
          </div>}
        </div>
        {page === "appearance" && <div className="hint">Quick switch: {sk("theme")}</div>}
        <div className="modal-actions">
          <span className="grow" />
          <button className="primary" onClick={onClose}>OK</button>
        </div>
      </div>
    </div>
  );
}

function StatusBar({ status, readOnly, update, onUpdate, jobs }: { status: string; readOnly: boolean; update?: string; onUpdate: () => void; jobs: TransferJob[] }) {
  const w = useStatusWidgets();
  return (
    <div className="statusbar">
      <span className="sb-text">{status}</span>
      <span className="grow" />
      <TransferBar jobs={jobs} />
      {update && (
        <button className="sb-widget sb-update" title="A new version is available" onClick={onUpdate}>
          ⬆ v{update} available
        </button>
      )}
      {w.aggregate && <span className="sb-widget" title="Aggregate of selected cells">{w.aggregate}</span>}
      {readOnly && <span className="sb-widget" title="Data source is read-only">🔒 Read-only</span>}
      {w.caret && <span className="sb-widget" title="Line:Column">{w.caret}</span>}
      <span className="sb-widget">UTF-8</span>
    </div>
  );
}
