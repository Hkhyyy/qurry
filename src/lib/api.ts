import { invoke } from "@tauri-apps/api/core";
import type { DataFormat } from "./transfer";

export type DbKind = "postgres" | "mysql" | "redis" | "sqlite" | "duckdb" | "mssql" | "clickhouse" | "mongodb" | "oracle" | "qdrant" | "chroma" | "milvus";
/** DBs connected via a separately downloaded driver plugin */
export const PLUGIN_KINDS: DbKind[] = ["sqlite", "duckdb", "mssql", "clickhouse", "mongodb", "oracle", "qdrant", "chroma", "milvus"];
/** Vector DB: the console speaks REST (METHOD /path + JSON body) */
export const VECTOR_KINDS: DbKind[] = ["qdrant", "chroma", "milvus"];
/** The DB is a single file (file path instead of host·port) */
export const isFileKind = (k: DbKind) => k === "sqlite" || k === "duckdb";
/** Start of the backend error when the driver is missing */
export const DRIVER_MISSING = "Driver not installed:";
/** A delete_connection error that still removed the data source (only its saved password stayed: AppState::delete_connection) */
export const removedDespite = (e: unknown) => String(e).includes("The data source was removed, but its saved password couldn't be");

export interface ConnectionConfig {
  id: string;
  name: string;
  kind: DbKind;
  host: string;
  port: number;
  user: string;
  database: string;
  color?: string | null;
  ssh?: SshConfig | null;
  /** DBs·schemas shown in the tree. No databases = all; no schemas[db] = all schemas of that DB */
  scope?: { databases?: string[] | null; schemas?: Record<string, string[]> };
  /** Options → Read-only */
  readOnly?: boolean;
  /** Options → remaining Connection settings */
  options?: ConnOptions;
  /** SSL on the SSH/SSL tab */
  ssl?: SslConfig | null;
  /** Redis connection mode */
  redis?: RedisMode | null;
}

export interface SslConfig {
  enabled: boolean;
  /** require | verify-ca | verify-full */
  mode: string;
  caFile: string;
  certFile: string;
  keyFile: string;
}
export interface RedisMode {
  mode: "standalone" | "sentinel" | "cluster";
  sentinelMaster: string;
  /** host:port, comma-separated */
  nodes: string;
}

export interface ConnOptions {
  txManual?: boolean;
  isolation?: string | null;
  switchSchema?: "auto" | "manual" | "disable" | null;
  timeZone?: string | null;
  keepAliveSecs?: number | null;
  autoDisconnectSecs?: number | null;
  singleDb?: boolean;
  /** Production: modifying statements show the affected row count and ask for confirmation */
  confirmChanges?: boolean;
  /** Remote MySQL with SSL unset may connect in plain text if it can't do TLS */
  allowPlaintext?: boolean;
  /** Whether it is visible to the MCP server (missing on older sources: Production hidden, others read·write) */
  mcp?: "off" | "read" | "write" | null;
  startupScript?: string;
  /** Oracle Instant Client folder */
  oracleClient?: string | null;
}

export interface SshConfig {
  enabled: boolean;
  host: string;
  port: number;
  user: string;
  auth: "password" | "key";
  keyPath: string;
}

export interface ConnectionView extends ConnectionConfig {
  hasPassword: boolean;
  connected: boolean;
}

export interface ColumnInfo {
  name: string;
  typeName: string;
}

export type Cell = string | number | boolean | null;

export interface QueryResult {
  columns: ColumnInfo[];
  rows: Cell[][];
  rowsAffected: number;
  truncated: boolean;
  elapsedMs: number;
  hasResultSet: boolean;
  inTransaction?: boolean;
}

/** password: undefined → keep existing, "" → delete */
/** Latest GitHub release (new version check) */
export interface ReleaseInfo {
  version: string;
  url: string;
  notes: string;
  assets: { name: string; url: string }[];
}
export interface UpdateInfo {
  current: string;
  latest: ReleaseInfo | null;
  /** Changes in versions newer than the current one (newest first) */
  changes?: { version: string; changes: string[] }[];
}
/** Whether this is a release URL of this repo (current name · old name). The update window opens only these (even if the cache in the config folder changes) */
export const isReleaseUrl = (u: string) => /^https:\/\/github\.com\/Hkhyyy\/(qurry|dbtool)\/releases(\/|$)/.test(u);

/** Automatic new-version check (at startup + every hour; turn off in Settings) */
const UPDATE_PREF = "dbtool.updateCheck";
export const updateCheckEnabled = () => {
  try {
    return localStorage.getItem(UPDATE_PREF) !== "off";
  } catch {
    return true;
  }
};
export const setUpdateCheckEnabled = (on: boolean) => {
  try {
    localStorage.setItem(UPDATE_PREF, on ? "on" : "off");
  } catch {
    /* ignore */
  }
};

export const api = {
  checkUpdate: (force = false) => invoke<UpdateInfo>("check_update", { force }),
  /** Download the new version, swap it in, and restart the app (progress via the "update-progress" event) */
  installUpdate: () => invoke<void>("install_update"),
  /** Open tabs·splits (restored on restart) */
  loadWorkspace: () => invoke<unknown>("load_workspace"),
  saveWorkspace: (ws: unknown) => invoke<void>("save_workspace", { ws }),
  /** The workspace list (name · Data Source) is shared with the TUI */
  sharedWorkspaces: () => invoke<WsInfo[] | null>("shared_workspaces"),
  adoptWorkspaces: (local: WsInfo[], synced: boolean) => invoke<{ list: WsInfo[]; renamed: [string, string][] }>("adopt_workspaces", { local, synced }),
  syncWorkspaces: (prev: WsInfo[], next: WsInfo[]) => invoke<WsInfo[]>("sync_workspaces", { prev, next }),
  listConnections: () => invoke<ConnectionView[]>("list_connections"),
  saveConnection: (config: ConnectionConfig, password?: string, sshSecret?: string) =>
    invoke<ConnectionConfig>("save_connection", { config, password: password ?? null, sshSecret: sshSecret ?? null }),
  deleteConnection: (id: string) => invoke<void>("delete_connection", { id }),
  testConnection: (config: ConnectionConfig, password?: string, sshSecret?: string) =>
    invoke<string>("test_connection", { config, password: password ?? null, sshSecret: sshSecret ?? null }),
  testSsh: (config: ConnectionConfig, sshSecret?: string) => invoke<string>("test_ssh", { config, sshSecret: sshSecret ?? null }),
  connect: (id: string) => invoke<string>("connect", { id }),
  disconnect: (id: string) => invoke<void>("disconnect", { id }),
  runQuery: (id: string, sql: string, tabId?: string, limit = 500) => invoke<QueryResult>("run_query", { id, sql, limit, tabId: tabId ?? null }),
  fetchMore: (tabId: string, limit = 500) => invoke<QueryResult>("fetch_more", { tabId, limit }),
  cancelQuery: (tabId: string) => invoke<boolean>("cancel_query", { tabId }),
  setTxMode: (id: string, tabId: string, manual: boolean) => invoke<void>("set_tx_mode", { id, tabId, manual }),
  commitTx: (tabId: string) => invoke<void>("commit_tx", { tabId }),
  rollbackTx: (tabId: string) => invoke<void>("rollback_tx", { tabId }),
  closeSession: (tabId: string) => invoke<void>("close_session", { tabId }),
  /** exactOne = saving grid edits (roll back everything unless each UPDATE/DELETE hits exactly 1 row) */
  executeBatch: (id: string, statements: string[], exactOne = false) => invoke<number>("execute_batch", { id, statements, exactOne }),
  // For file writes the backend opens the save dialog itself (the webview doesn't pick the path). null if canceled
  exportConnections: () => invoke<[number, string] | null>("export_connections"),
  /** Pick a file to import → preview the changes. importPicked on confirm */
  pickImportFile: () => invoke<string[] | null>("pick_import_file"),
  importPicked: () => invoke<{ added: number; updated: number }>("import_picked"),
  saveTextFile: (defaultName: string, text: string) => invoke<string | null>("save_text_file", { defaultName, text }),
  /** Data export: file (save dialog) or clipboard text. null if the save dialog is closed. Progress via the "transfer-progress" event */
  exportData: (id: string, sql: string, format: DataFormat, clipboard: boolean, defaultName: string, table: [string, string] | null, job: string) =>
    invoke<{ rows: number; path: string | null; text: string | null } | null>("export_data", { id, sql, format, clipboard, defaultName, table, job }),
  cancelJob: (job: string) => invoke<void>("cancel_job", { job }),
  /** Pick a file to import → (path, preview). The backend remembers the picked file */
  pickDataFile: () => invoke<[string, DataPreview] | null>("pick_data_file"),
  importData: (id: string, spec: ImportSpec, job: string) => invoke<number>("import_data", { id, spec, job }),
  txState: (tabId: string) => invoke<boolean>("tx_state", { tabId }),
  trustSshHost: (host: string, port: number) => invoke<string>("trust_ssh_host", { host, port }),
  /** Reason if the statement is dangerous and needs confirmation before running (UPDATE/DELETE without WHERE, DROP, TRUNCATE, Redis FLUSH*·DEL) */
  /** Whether the driver plugin is installed (null for built-in DBs) · download and install */
  driverStatus: (kind: DbKind) => invoke<boolean | null>("driver_status", { kind }),
  installDriver: (kind: DbKind) => invoke<string>("install_driver", { kind }),
  unsafeReason: (kind: DbKind, sql: string) => invoke<string | null>("unsafe_reason", { kind, sql }),
};

export interface SchemaInfo { name: string; isSystem: boolean }
export interface RelationInfo { name: string; kind: "table" | "partitioned" | "view" | "matview" | "foreign" | "sequence" | "key"; comment: string | null; rowEstimate: number | null }
export interface RoutineInfo { name: string; args: string; returns: string | null; kind: string }
export interface SchemaObjects { relations: RelationInfo[]; routines: RoutineInfo[] }
export interface ColumnMeta { name: string; dataType: string; nullable: boolean; defaultValue: string | null; comment: string | null; isPk: boolean }
export interface IndexMeta { name: string; definition: string; isUnique: boolean; isPrimary: boolean }
export interface ConstraintMeta { name: string; kind: "p" | "f" | "u" | "c"; definition: string }
export interface TableDetail { columns: ColumnMeta[]; indexes: IndexMeta[]; constraints: ConstraintMeta[] }
/** One row of the workspace list (shared by GUI · TUI) */
export interface WsInfo { id: string; name: string; connIds: string[] }
export interface CompletionTable { schema: string; name: string; columns: [string, string][] }

export type DbaAction = { kind: "kill"; label: string; terminate: boolean } | { kind: "sql"; label: string; template: string };
export interface DbaReportInfo { id: string; title: string; group: string; help: string; actions: DbaAction[] }
/** Execution plan analysis (core): shares·warnings. PlanNode is in components/ConsolePane */
export const analyzePlan = (json: string) => invoke<import("../components/ConsolePane").PlanNode>("analyze_plan", { json });
export interface HistoryItem { id: string; connId: string; database?: string; sql: string; ts: number }
export interface Snippet { id: string; name: string; sql: string; connId?: string | null }
/** Test data generation: fill method per column · sample rows · warnings */
export interface FakePreview { plan: { name: string; dataType: string; how: string }[]; result: QueryResult; warnings: string[] }
export const fake = {
  preview: (id: string, schema: string, table: string, rows = 8) => invoke<FakePreview>("fake_preview", { id, schema, table, rows }),
  fill: (id: string, schema: string, table: string, rows: number) => invoke<number>("fake_fill", { id, schema, table, rows }),
};
/** Copy a slice of production data: start rows + rows linked by FK → another Data Source (personal data is masked) */
export interface SubsetSpec { schema: string; table: string; filter: string; depth: number; mask: boolean }
export interface SubsetPlan { tables: { schema: string; table: string; rows: number; masked: { column: string; mask: string }[]; reason: string }[]; total: number }
export const subset = {
  preview: (source: string, spec: SubsetSpec) => invoke<SubsetPlan>("subset_preview", { source, spec }),
  copy: (source: string, spec: SubsetSpec, target: string) => invoke<SubsetPlan>("subset_copy", { source, spec, target }),
};
/** Create · modify objects (New ▸ · Modify Table… · Rename…) */
export interface ColumnDef { name: string; orig?: string | null; dataType: string; nullable: boolean; default?: string | null; autoIncrement: boolean; comment?: string | null }
export interface IndexDef { name: string; orig?: string | null; columns: string[]; unique: boolean; constraint?: boolean; definition?: string | null }
export interface ForeignKeyDef { name: string; orig?: string | null; columns: string[]; refSchema: string; refTable: string; refColumns: string[]; onDelete: string; onUpdate: string }
export interface TableDef { schema: string; name: string; columns: ColumnDef[]; primaryKey: string[]; primaryKeyName?: string | null; indexes: IndexDef[]; foreignKeys: ForeignKeyDef[]; comment?: string | null }
export type NewObject =
  | { kind: "database"; name: string; charset: string }
  | { kind: "schema"; name: string }
  | { kind: "view"; schema: string; name: string; query: string; materialized: boolean }
  | { kind: "sequence"; schema: string; name: string; start: number | null; increment: number | null };
export const ddl = {
  tableDef: (id: string, schema: string, table: string) => invoke<TableDef>("table_def", { id, schema, table }),
  table: (id: string, before: TableDef | null, after: TableDef) => invoke<string[]>("table_ddl", { id, before, after }),
  newObject: (id: string, object: NewObject) => invoke<string[]>("new_object_ddl", { id, object }),
  rename: (id: string, what: string, schema: string, old: string, newName: string) => invoke<string>("rename_ddl", { id, what, schema, old, new: newName }),
  run: (id: string, statements: string[]) => invoke<void>("run_ddl", { id, statements }),
};
/** Production log: changes run on Production Data Sources (shared with the TUI) */
export interface AuditEntry { id: string; ts: number; connId: string; connName: string; database?: string; sql: string; rows?: number; error?: string; inTx: boolean; undo?: string; note?: string }
export const auditLog = (connId: string | null, filter: string, limit = 500) => invoke<AuditEntry[]>("audit_log", { connId, filter, limit });
/** Query history·snippets (config folder, shared with the TUI) */
export const history = {
  add: (connId: string, database: string | undefined, sql: string) => invoke<void>("history_add", { connId, database: database ?? null, sql }),
  list: (connId: string | null, filter: string, limit = 500) => invoke<HistoryItem[]>("history_list", { connId, filter, limit }),
  remove: (id: string) => invoke<void>("history_delete", { id }),
  import: (items: HistoryItem[]) => invoke<number>("history_import", { items }),
  snippets: (connId: string | null) => invoke<Snippet[]>("snippets", { connId }),
  saveSnippet: (snippet: Snippet) => invoke<Snippet>("snippet_save", { snippet }),
  deleteSnippet: (id: string) => invoke<void>("snippet_delete", { id }),
};
/** Production confirmation: rows this statement will change (null if they can't be counted) */
export const previewImpact = (id: string, sql: string) => invoke<number | null>("preview_impact", { id, sql });
export interface ErdColumn { name: string; dataType: string; pk: boolean; nullable: boolean }
export interface ErdTable { schema: string; name: string; columns: ErdColumn[] }
export interface ErdRelation { name: string; fromSchema: string; fromTable: string; fromColumns: string[]; toSchema: string; toTable: string; toColumns: string[] }
/** ERD (tables·foreign keys) + Mermaid text */
export const erd = (id: string, schemas: string[]) => invoke<[{ tables: ErdTable[]; relations: ErdRelation[] }, string]>("erd", { id, schemas });
/** Save text to a file (save dialog). null if canceled */
export const saveFile = (defaultName: string, text: string, filter: string, ext: string) => invoke<string | null>("save_file", { defaultName, text, filter, ext });
export interface BackupOptions { format: "custom" | "plain"; schemaOnly: boolean; dataOnly: boolean; schemas: string[] }
/** Backup·restore (pg_dump / mysqldump). Progress via "transfer-progress" */
export const backup = {
  run: (id: string, opts: BackupOptions, defaultName: string, job: string) => invoke<[number, string] | null>("backup_db", { id, opts, defaultName, job }),
  restore: (id: string, clean: boolean, job: string) => invoke<string | null>("restore_db", { id, opts: { clean }, job }),
};
export interface SchemaChange { kind: string; object: string; sql: string; destructive: boolean }
export interface DataDiff { same: number; onlySource: number; onlyTarget: number; different: number; sql: string; truncated: boolean }
export interface Migration { version: string; description: string; file: string; checksum: string; status: "pending" | "applied" | "changed" | "missing"; appliedAt: string | null }
/** Schema·data compare, migration files (folders only as picked via pickFolder) */
export const migrate = {
  compareSchemas: (sourceId: string, sourceSchema: string, targetId: string, targetSchema: string) => invoke<SchemaChange[]>("compare_schemas", { sourceId, sourceSchema, targetId, targetSchema }),
  compareData: (sourceId: string, sourceSchema: string, sourceTable: string, targetId: string, targetSchema: string, targetTable: string) =>
    invoke<DataDiff>("compare_data", { sourceId, sourceSchema, sourceTable, targetId, targetSchema, targetTable }),
  pickFolder: () => invoke<string | null>("pick_folder"),
  status: (id: string, dir: string) => invoke<Migration[]>("migration_status", { id, dir }),
  apply: (id: string, dir: string) => invoke<string[]>("apply_migrations", { id, dir }),
  save: (dir: string, description: string, sql: string) => invoke<string>("save_migration", { dir, description, sql }),
};
/** RAG index: text column of a source table → embeddings → store (pgvector table · vector DB collection) */
export interface RagIndex {
  id: string;
  name: string;
  source: string;
  schema: string;
  table: string;
  idColumn: string;
  textColumns: string[];
  provider: "openai" | "ollama";
  model: string;
  baseUrl: string;
  dest: string;
  collection: string;
  dim?: number | null;
  rows?: number;
  builtAt?: string | null;
}
/** Where the password file's master key is kept */
export const secretKey = {
  location: () => invoke<"file" | "keychain">("secret_key_location"),
  move: (keychain: boolean) => invoke<"file" | "keychain">("set_secret_key_location", { keychain }),
};
export const RAG_DEFAULT_MODEL = { openai: "text-embedding-3-small", ollama: "nomic-embed-text" } as const;
export const rag = {
  list: () => invoke<RagIndex[]>("rag_indexes"),
  save: (index: RagIndex) => invoke<RagIndex>("rag_save", { index }),
  remove: (id: string) => invoke<void>("rag_delete", { id }),
  hasKey: () => invoke<boolean>("rag_has_key"),
  setKey: (key: string | null) => invoke<void>("rag_set_key", { key }),
  /** Progress via "transfer-progress" */
  build: (id: string, job: string) => invoke<number>("rag_build", { id, job }),
  search: (id: string, query: string, limit = 20) => invoke<QueryResult>("rag_search", { id, query, limit }),
};
export const dba = {
  reports: (id: string) => invoke<DbaReportInfo[]>("dba_reports", { id }),
  run: (id: string, report: string) => invoke<QueryResult>("dba_report", { id, report }),
  kill: (id: string, pid: number, terminate: boolean) => invoke<boolean>("kill_session", { id, pid, terminate }),
  actionSql: (id: string, template: string, row: Record<string, string>) => invoke<string>("dba_action_sql", { id, template, row }),
};

export interface DataPreview { format: DataFormat; columns: string[]; rows: (string | null)[][]; bytes: number }
/** columns: [file column index, table column]. create = new table (all columns text) */
export interface ImportSpec { path: string; schema: string; table: string; create: boolean; columns: [number, string][] }

export const meta = {
  databases: (id: string) => invoke<string[]>("list_databases", { id }),
  schemas: (id: string) => invoke<SchemaInfo[]>("list_schemas", { id }),
  objects: (id: string, schema: string) => invoke<SchemaObjects>("list_objects", { id, schema }),
  table: (id: string, schema: string, table: string) => invoke<TableDetail>("table_detail", { id, schema, table }),
  /** pgvector: SQL finding rows near this row (pk = [column, value]) */
  similarRowsSql: (id: string, schema: string, table: string, column: string, pk: [string, string | null][]) => invoke<string>("similar_rows_sql", { id, schema, table, column, pk }),
  completion: (id: string) => invoke<CompletionTable[]>("completion_meta", { id }),
  /** Autocomplete value suggestions: column values · json keys (first rows only, most frequent first) */
  hints: (id: string, req: { schema: string; table: string; column: string; path: string[]; keys: boolean; pairs?: boolean }) => invoke<{ value: string; count: number }[]>("column_hints", { id, req }),
  ddl: (id: string, schema: string, name: string, kind: string) => invoke<string>("object_ddl", { id, schema, name, kind }),
  objectIndex: (id: string) => invoke<IndexEntry[]>("object_index", { id }),
  /** Last known object·column list of a disconnected Data Source (saved each time it is read while connected) */
  cachedIndex: () => invoke<CachedIndex[]>("cached_index"),
  warmSearchCache: (id: string) => invoke<void>("warm_search_cache", { id }),
  fullTextSearch: (id: string, req: FtsRequest) => invoke<FtsHit[]>("full_text_search", { id, req }),
};
/** Redis default DB name: "" / "3" / "db3" → "db3" */
export const redisDb = (database?: string | null) => `db${String(database ?? "").replace(/^db/, "").trim() || "0"}`;
export interface RedisKeyRow { key: string; type: string; ttl: number }
export interface RedisKeyDetail { type: string; ttl: number; length: number; memory: number | null; encoding: string | null; value: unknown; truncated: boolean }
export type RedisReply = null | number | string | boolean | { status: string } | RedisReply[];
export const redis = {
  scan: (id: string, cursor: string, pattern: string, count: number, typeFilter?: string) =>
    invoke<{ cursor: string; keys: RedisKeyRow[] }>("redis_scan", { id, cursor, pattern, count, typeFilter: typeFilter || null }),
  key: (id: string, key: string, limit?: number) => invoke<RedisKeyDetail>("redis_key", { id, key, limit: limit ?? null }),
  exec: (id: string, args: string[]) => invoke<RedisReply>("redis_exec", { id, args }),
  execLine: (id: string, line: string) => invoke<RedisReply>("redis_exec_line", { id, line }),
};
export interface IndexEntry { schema: string; name: string; kind: string }
export interface CachedIndex { target: string; objects: IndexEntry[]; columns: CompletionTable[] }
export interface FtsRequest {
  text: string;
  mode: "contains" | "starts" | "ends" | "matches" | "like";
  matchCase: boolean;
  columns: "text" | "numeric" | "all";
  schemas: string[];
  tables: [string, string][];
  limit: number;
}
export interface FtsHit { schema: string; table: string; whereClause: string; result: QueryResult }

/** Reserved words that break syntax unless quoted (same list as RESERVED in core sql.rs) */
const RESERVED = new Set(
  ("all analyse analyze and any array as asc asymmetric between both by case cast change check collate column condition constraint create cross " +
    "current_date current_role current_time current_timestamp current_user database default deferrable delete desc distinct do drop else end except " +
    "exists false fetch for foreign from full grant group having in index initially inner insert interval intersect into is join key keys lateral " +
    "leading left like limit localtime localtimestamp match natural not null offset on only or order outer placing primary range rank read references " +
    "returning right row rows select session_user set some symmetric table then to trailing true union unique update usage user using values variadic " +
    "when where window with write").split(" "),
);
export const quoteIdent = (s: string) => (/^[a-z_][a-z0-9_]*$/.test(s) && !RESERVED.has(s) ? s : `"${s.replace(/"/g, '""')}"`);

/** Colors to tell connections apart, like DataGrip (for dark backgrounds) */
export const COLOR_PRESETS: { name: string; value: string | null }[] = [
  { name: "None", value: null },
  { name: "Red", value: "#6e2a2a" },
  { name: "Orange", value: "#6e4a22" },
  { name: "Yellow", value: "#5f5b24" },
  { name: "Green", value: "#2f5634" },
  { name: "Teal", value: "#22524f" },
  { name: "Blue", value: "#28406e" },
  { name: "Violet", value: "#4f2f68" },
  { name: "Gray", value: "#45474d" },
];

export const DEFAULT_PORT: Record<DbKind, number> = { postgres: 5432, mysql: 3306, redis: 6379, sqlite: 0, duckdb: 0, mssql: 1433, clickhouse: 8123, mongodb: 27017, oracle: 1521, qdrant: 6333, chroma: 8000, milvus: 19530 };
export const KIND_LABEL: Record<DbKind, string> = {
  postgres: "PostgreSQL",
  mysql: "MySQL / MariaDB",
  redis: "Redis",
  sqlite: "SQLite",
  duckdb: "DuckDB",
  mssql: "SQL Server",
  clickhouse: "ClickHouse",
  mongodb: "MongoDB",
  oracle: "Oracle",
  qdrant: "Qdrant",
  chroma: "Chroma",
  milvus: "Milvus",
};

/** Literal used in data-editing SQL. PG casts it to the column type. */
export const sqlLiteral = (v: Cell) => (v === null ? "NULL" : `'${String(v).replace(/'/g, "''")}'`);

/** SQL dialect per DB kind: identifier quoting, string literals, text cast */
export interface Dialect {
  q: (s: string) => string;
  lit: (v: Cell) => string;
  text: (expr: string) => string;
  monaco: string;
  /** Console statement splitting (semicolon-based if absent) */
  split?: (text: string) => { start: number; end: number }[];
}
const PG: Dialect = { q: quoteIdent, lit: sqlLiteral, text: (e) => `${e}::text`, monaco: "pgsql" };
const MY: Dialect = {
  q: (s) => (/^[A-Za-z_][A-Za-z0-9_]*$/.test(s) && !RESERVED.has(s.toLowerCase()) ? s : "`" + s.replace(/`/g, "``") + "`"),
  // In MySQL strings backslash is also an escape char
  lit: (v) => (v === null ? "NULL" : `'${String(v).replace(/\\/g, "\\\\").replace(/'/g, "''")}'`),
  text: (e) => `CAST(${e} AS CHAR)`,
  monaco: "mysql",
};
/** Redis: one line = one command (# comments) */
const RD: Dialect = {
  q: (s) => s,
  lit: (v) => (v === null ? '""' : JSON.stringify(String(v))),
  text: (e) => e,
  monaco: "redis",
  split: (text) => {
    const out: { start: number; end: number }[] = [];
    let pos = 0;
    for (const line of text.split("\n")) {
      const t = line.trim();
      if (t && !t.startsWith("#") && !t.startsWith("//")) out.push({ start: pos, end: pos + line.length });
      pos += line.length + 1;
    }
    return out;
  },
};
/** Driver DBs: standard SQL quoting, text conversion via CAST (no PG ::text) */
const GEN: Dialect = { ...PG, text: (e) => `CAST(${e} AS VARCHAR(4000))`, monaco: "sql" };
const MONGO: Dialect = { q: (s) => s, lit: sqlLiteral, text: (e) => e, monaco: "javascript", split: RD.split };
/** One REST request = from a method line up to the next method line (including JSON body). Same rule as core's sql::split_rest */
const REST: Dialect = {
  q: (s) => s,
  lit: sqlLiteral,
  text: (e) => e,
  monaco: "javascript",
  split: (text) => {
    const out: { start: number; end: number }[] = [];
    let pos = 0;
    let cur: { start: number; end: number } | null = null;
    for (const line of text.split("\n")) {
      const t = line.trim();
      if (/^(GET|POST|PUT|PATCH|DELETE|SELECT)\s/i.test(t + " ")) {
        if (cur) out.push(cur);
        cur = { start: pos, end: pos + line.length };
      } else if (t && !t.startsWith("#") && !t.startsWith("//")) {
        if (cur) cur.end = pos + line.length;
        else cur = { start: pos, end: pos + line.length };
      }
      pos += line.length + 1;
    }
    if (cur) out.push(cur);
    return out;
  },
};
export const dialectOf = (kind?: DbKind) => (kind === "mysql" ? MY : kind === "redis" ? RD : kind === "mongodb" ? MONGO : kind && VECTOR_KINDS.includes(kind) ? REST : kind && PLUGIN_KINDS.includes(kind) ? GEN : PG);

/** Backend connection target: Data Source id + DB name */
export const targetId = (connId: string, database?: string | null) => (database ? `${connId}::${database}` : connId);

/** Schema selection rules (DataGrip "N of M" popup). Special values: "*" = all, "@default" = default DB / default schema (public) */
export type Scope = NonNullable<ConnectionConfig["scope"]>;
export const SCOPE_ALL = "*";
export const SCOPE_DEFAULT = "@default";

export function dbVisible(scope: Scope | undefined, db: string, defaultDb: string) {
  const d = scope?.databases;
  if (!d || d.length === 0) return true;
  return d.includes(SCOPE_ALL) || d.includes(db) || (d.includes(SCOPE_DEFAULT) && db === defaultDb);
}

export function schemaVisible(scope: Scope | undefined, db: string, schema: string, isSystem: boolean, showSystem: boolean) {
  const s = scope?.schemas?.[db];
  if (!s || s.length === 0 || s.includes(SCOPE_ALL)) return showSystem || !isSystem;
  return s.includes(schema) || (s.includes(SCOPE_DEFAULT) && schema === "public");
}
