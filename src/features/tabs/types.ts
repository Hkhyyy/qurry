import type { QueryResult } from "../../lib/api";
import type { OutputLine, ResultEntry } from "../../components/ConsolePane";
import type { EditTarget } from "../../components/ResultGrid";
import { leaf, type Pane } from "../../lib/panes";
import type { DdlView } from "../../lib/structure";

/** Pane id (0~3). The pane layout is a Pane tree */
type Group = number;

/** One workspace's tabs and split view (kept while it is not the current one) */
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
  /** console: SQL console / table: table data editor / ddl: read-only DDL */
  kind: "console" | "table" | "ddl" | "redis" | "dba" | "erd" | "migrate" | "rag";
  group: Group;
  connId: string;
  /** Connected DB (if none, the Data Source's default DB) */
  database?: string;
  /** Backend connection target id (connId or connId::database) */
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
  /** Console's current schema (search_path) */
  schema?: string;
  tableSql?: string;
  edit?: EditTarget;
  /** Console: result tabs, Output log, transaction */
  results: ResultEntry[];
  activeResult: string;
  output: OutputLine[];
  txManual: boolean;
  inTx: boolean;
  /** Result tab with an open server cursor (target for loading more) */
  cursorEntry?: string;
  /** Pin Tab */
  pinned?: boolean;
  /** Redis browser: key to select first */
  redisKey?: string;
  /** dba: report being viewed (sessions, slow_queries, …) */
  dbaReport?: string;
  /** erd: schemas to draw */
  erdSchemas?: string[];
  /** ddl of a table (edit set): SQL text or Structure; set when built from the last choice (lib/structure ddlView) */
  ddlView?: DdlView;
  /** Tab restored on relaunch: not loaded yet (loads when first shown) */
  stale?: boolean;
}

/** Closed tabs / recent files (Reopen Closed Tab, ⌘e Recent Files) */
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
const TAB_LIMIT = 10; // DataGrip default Tab limit
const snapKey = (t: { kind: string; connId: string; database?: string; title: string; edit?: EditTarget }) =>
  `${t.kind}|${t.connId}|${t.database ?? ""}|${t.edit ? t.edit.schema + "." + t.edit.table : t.title}`;

export type { Group, WsLayout, Tab, TabSnapshot };
export { emptyLayout, TAB_LIMIT, snapKey };
