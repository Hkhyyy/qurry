import { invoke } from "@tauri-apps/api/core";
import type { DataFormat } from "./transfer";

export type DbKind = "postgres" | "mysql" | "redis" | "sqlite" | "duckdb" | "mssql" | "clickhouse" | "mongodb" | "oracle" | "qdrant" | "chroma" | "milvus";
/** 따로 받는 드라이버 플러그인으로 붙는 DB */
export const PLUGIN_KINDS: DbKind[] = ["sqlite", "duckdb", "mssql", "clickhouse", "mongodb", "oracle", "qdrant", "chroma", "milvus"];
/** 벡터 DB: 콘솔이 REST (METHOD /path + JSON 본문) */
export const VECTOR_KINDS: DbKind[] = ["qdrant", "chroma", "milvus"];
/** 파일 하나가 DB (호스트·포트 대신 파일 경로) */
export const isFileKind = (k: DbKind) => k === "sqlite" || k === "duckdb";
/** 드라이버가 없을 때 백엔드 오류의 앞부분 */
export const DRIVER_MISSING = "Driver not installed:";

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
  /** 트리에 보일 DB·스키마. databases 가 없으면 전부, schemas[db] 가 없으면 그 DB의 스키마 전부 */
  scope?: { databases?: string[] | null; schemas?: Record<string, string[]> };
  /** Options → Read-only */
  readOnly?: boolean;
  /** Options → 나머지 Connection 설정 */
  options?: ConnOptions;
  /** SSH/SSL 탭의 SSL */
  ssl?: SslConfig | null;
  /** Redis 연결 방식 */
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
  /** host:port, 쉼표 구분 */
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
  /** Production: 바꾸는 문장은 바뀔 행 수를 보여 주고 확인 */
  confirmChanges?: boolean;
  /** MCP 서버에 보이는지 (없으면 예전 소스: Production 은 숨김, 나머지는 읽기·쓰기) */
  mcp?: "off" | "read" | "write" | null;
  startupScript?: string;
  /** Oracle Instant Client 폴더 */
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

/** password: undefined → 기존 유지, "" → 삭제 */
/** GitHub 의 최신 릴리스 (새 버전 확인) */
export interface ReleaseInfo {
  version: string;
  url: string;
  notes: string;
  assets: { name: string; url: string }[];
}
export interface UpdateInfo {
  current: string;
  latest: ReleaseInfo | null;
  /** 지금 버전보다 새 버전들의 바뀐 것 (최신부터) */
  changes?: { version: string; changes: string[] }[];
}
/** 이 저장소(지금 이름 · 예전 이름)의 릴리스 주소인지. 업데이트 창은 이것만 연다 (설정 폴더의 캐시가 바뀌어도) */
export const isReleaseUrl = (u: string) => /^https:\/\/github\.com\/Hkhyyy\/(qurry|dbtool)\/releases(\/|$)/.test(u);

/** 새 버전 자동 확인 (시작할 때 + 1시간마다. Settings 에서 끈다) */
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
    /* 무시 */
  }
};

export const api = {
  checkUpdate: (force = false) => invoke<UpdateInfo>("check_update", { force }),
  /** 새 버전을 받아 바꿔 넣고 앱을 다시 켠다 (진행은 "update-progress" 이벤트) */
  installUpdate: () => invoke<void>("install_update"),
  /** 열어 둔 탭·분할 (다시 켜면 그대로) */
  loadWorkspace: () => invoke<unknown>("load_workspace"),
  saveWorkspace: (ws: unknown) => invoke<void>("save_workspace", { ws }),
  /** 워크스페이스 목록 (이름 · Data Source) 은 TUI 와 같이 쓴다 */
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
  /** exactOne = 표 편집 저장 (UPDATE/DELETE 가 정확히 1행이 아니면 전부 롤백) */
  executeBatch: (id: string, statements: string[], exactOne = false) => invoke<number>("execute_batch", { id, statements, exactOne }),
  // 파일 쓰기는 백엔드가 저장 대화상자를 직접 연다 (웹뷰가 경로를 정하지 않는다). 취소하면 null
  exportConnections: () => invoke<[number, string] | null>("export_connections"),
  /** 가져올 파일 고르기 → 바뀌는 내용 미리보기. 확인하면 importPicked */
  pickImportFile: () => invoke<string[] | null>("pick_import_file"),
  importPicked: () => invoke<{ added: number; updated: number }>("import_picked"),
  saveTextFile: (defaultName: string, text: string) => invoke<string | null>("save_text_file", { defaultName, text }),
  /** 데이터 내보내기: 파일(저장 창) 또는 클립보드 글. 저장 창을 닫으면 null. 진행은 "transfer-progress" 이벤트 */
  exportData: (id: string, sql: string, format: DataFormat, clipboard: boolean, defaultName: string, table: [string, string] | null, job: string) =>
    invoke<{ rows: number; path: string | null; text: string | null } | null>("export_data", { id, sql, format, clipboard, defaultName, table, job }),
  cancelJob: (job: string) => invoke<void>("cancel_job", { job }),
  /** 가져올 파일 고르기 → (경로, 미리보기). 고른 파일은 백엔드가 기억한다 */
  pickDataFile: () => invoke<[string, DataPreview] | null>("pick_data_file"),
  importData: (id: string, spec: ImportSpec, job: string) => invoke<number>("import_data", { id, spec, job }),
  txState: (tabId: string) => invoke<boolean>("tx_state", { tabId }),
  trustSshHost: (host: string, port: number) => invoke<string>("trust_ssh_host", { host, port }),
  /** 실행 전 확인할 위험 문장이면 이유 (WHERE 없는 UPDATE/DELETE, DROP, TRUNCATE, Redis FLUSH*·DEL) */
  /** 드라이버 플러그인이 깔려 있는지 (기본 DB 는 null) · 받아 깔기 */
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
/** 워크스페이스 목록 한 줄 (GUI · TUI 공유) */
export interface WsInfo { id: string; name: string; connIds: string[] }
export interface CompletionTable { schema: string; name: string; columns: [string, string][] }

export type DbaAction = { kind: "kill"; label: string; terminate: boolean } | { kind: "sql"; label: string; template: string };
export interface DbaReportInfo { id: string; title: string; group: string; help: string; actions: DbaAction[] }
/** 실행 계획 분석 (core): 몫·경고. PlanNode 는 components/ConsolePane */
export const analyzePlan = (json: string) => invoke<import("../components/ConsolePane").PlanNode>("analyze_plan", { json });
export interface HistoryItem { id: string; connId: string; database?: string; sql: string; ts: number }
export interface Snippet { id: string; name: string; sql: string; connId?: string | null }
/** 테스트 데이터 만들기: 컬럼마다 채우는 법 · 예시 행 · 경고 */
export interface FakePreview { plan: { name: string; dataType: string; how: string }[]; result: QueryResult; warnings: string[] }
export const fake = {
  preview: (id: string, schema: string, table: string, rows = 8) => invoke<FakePreview>("fake_preview", { id, schema, table, rows }),
  fill: (id: string, schema: string, table: string, rows: number) => invoke<number>("fake_fill", { id, schema, table, rows }),
};
/** 운영 데이터 일부 복사: 시작 행 + FK 로 이어진 행 → 다른 Data Source (개인정보는 가린다) */
export interface SubsetSpec { schema: string; table: string; filter: string; depth: number; mask: boolean }
export interface SubsetPlan { tables: { schema: string; table: string; rows: number; masked: { column: string; mask: string }[]; reason: string }[]; total: number }
export const subset = {
  preview: (source: string, spec: SubsetSpec) => invoke<SubsetPlan>("subset_preview", { source, spec }),
  copy: (source: string, spec: SubsetSpec, target: string) => invoke<SubsetPlan>("subset_copy", { source, spec, target }),
};
/** 객체 만들기 · 고치기 (New ▸ · Modify Table… · Rename…) */
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
/** 운영 작업 기록: Production Data Source 에서 실행한 변경 (TUI 와 같이 쓴다) */
export interface AuditEntry { id: string; ts: number; connId: string; connName: string; database?: string; sql: string; rows?: number; error?: string; inTx: boolean; undo?: string; note?: string }
export const auditLog = (connId: string | null, filter: string, limit = 500) => invoke<AuditEntry[]>("audit_log", { connId, filter, limit });
/** 쿼리 히스토리·스니펫 (설정 폴더, TUI 와 같이 쓴다) */
export const history = {
  add: (connId: string, database: string | undefined, sql: string) => invoke<void>("history_add", { connId, database: database ?? null, sql }),
  list: (connId: string | null, filter: string, limit = 500) => invoke<HistoryItem[]>("history_list", { connId, filter, limit }),
  remove: (id: string) => invoke<void>("history_delete", { id }),
  import: (items: HistoryItem[]) => invoke<number>("history_import", { items }),
  snippets: (connId: string | null) => invoke<Snippet[]>("snippets", { connId }),
  saveSnippet: (snippet: Snippet) => invoke<Snippet>("snippet_save", { snippet }),
  deleteSnippet: (id: string) => invoke<void>("snippet_delete", { id }),
};
/** Production 확인: 이 문장이 바꿀 행 수 (셀 수 없으면 null) */
export const previewImpact = (id: string, sql: string) => invoke<number | null>("preview_impact", { id, sql });
export interface ErdColumn { name: string; dataType: string; pk: boolean; nullable: boolean }
export interface ErdTable { schema: string; name: string; columns: ErdColumn[] }
export interface ErdRelation { name: string; fromSchema: string; fromTable: string; fromColumns: string[]; toSchema: string; toTable: string; toColumns: string[] }
/** ERD (테이블·외래 키) + Mermaid 글 */
export const erd = (id: string, schemas: string[]) => invoke<[{ tables: ErdTable[]; relations: ErdRelation[] }, string]>("erd", { id, schemas });
/** 글을 파일로 저장 (저장 창). 취소하면 null */
export const saveFile = (defaultName: string, text: string, filter: string, ext: string) => invoke<string | null>("save_file", { defaultName, text, filter, ext });
export interface BackupOptions { format: "custom" | "plain"; schemaOnly: boolean; dataOnly: boolean; schemas: string[] }
/** 백업·복원 (pg_dump / mysqldump). 진행은 "transfer-progress" */
export const backup = {
  run: (id: string, opts: BackupOptions, defaultName: string, job: string) => invoke<[number, string] | null>("backup_db", { id, opts, defaultName, job }),
  restore: (id: string, clean: boolean, job: string) => invoke<string | null>("restore_db", { id, opts: { clean }, job }),
};
export interface SchemaChange { kind: string; object: string; sql: string; destructive: boolean }
export interface DataDiff { same: number; onlySource: number; onlyTarget: number; different: number; sql: string; truncated: boolean }
export interface Migration { version: string; description: string; file: string; checksum: string; status: "pending" | "applied" | "changed" | "missing"; appliedAt: string | null }
/** 스키마·데이터 비교, 마이그레이션 파일 (폴더는 pickFolder 로 고른 것만) */
export const migrate = {
  compareSchemas: (sourceId: string, sourceSchema: string, targetId: string, targetSchema: string) => invoke<SchemaChange[]>("compare_schemas", { sourceId, sourceSchema, targetId, targetSchema }),
  compareData: (sourceId: string, sourceSchema: string, sourceTable: string, targetId: string, targetSchema: string, targetTable: string) =>
    invoke<DataDiff>("compare_data", { sourceId, sourceSchema, sourceTable, targetId, targetSchema, targetTable }),
  pickFolder: () => invoke<string | null>("pick_folder"),
  status: (id: string, dir: string) => invoke<Migration[]>("migration_status", { id, dir }),
  apply: (id: string, dir: string) => invoke<string[]>("apply_migrations", { id, dir }),
  save: (dir: string, description: string, sql: string) => invoke<string>("save_migration", { dir, description, sql }),
};
/** RAG 인덱스: 원본 테이블의 글 열 → 임베딩 → 저장소(pgvector 테이블 · 벡터 DB 컬렉션) */
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
export const RAG_DEFAULT_MODEL = { openai: "text-embedding-3-small", ollama: "nomic-embed-text" } as const;
export const rag = {
  list: () => invoke<RagIndex[]>("rag_indexes"),
  save: (index: RagIndex) => invoke<RagIndex>("rag_save", { index }),
  remove: (id: string) => invoke<void>("rag_delete", { id }),
  hasKey: () => invoke<boolean>("rag_has_key"),
  setKey: (key: string | null) => invoke<void>("rag_set_key", { key }),
  /** 진행은 "transfer-progress" */
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
/** columns: [파일 열 번호, 테이블 컬럼]. create = 새 테이블 (컬럼은 모두 text) */
export interface ImportSpec { path: string; schema: string; table: string; create: boolean; columns: [number, string][] }

export const meta = {
  databases: (id: string) => invoke<string[]>("list_databases", { id }),
  schemas: (id: string) => invoke<SchemaInfo[]>("list_schemas", { id }),
  objects: (id: string, schema: string) => invoke<SchemaObjects>("list_objects", { id, schema }),
  table: (id: string, schema: string, table: string) => invoke<TableDetail>("table_detail", { id, schema, table }),
  /** pgvector: 이 행과 가까운 행을 찾는 SQL (pk = [열, 값]) */
  similarRowsSql: (id: string, schema: string, table: string, column: string, pk: [string, string | null][]) => invoke<string>("similar_rows_sql", { id, schema, table, column, pk }),
  completion: (id: string) => invoke<CompletionTable[]>("completion_meta", { id }),
  /** 자동완성 값 추천: 컬럼 값 · json 키 (앞쪽 일부 행, 많이 나온 순) */
  hints: (id: string, req: { schema: string; table: string; column: string; path: string[]; keys: boolean; pairs?: boolean }) => invoke<{ value: string; count: number }[]>("column_hints", { id, req }),
  ddl: (id: string, schema: string, name: string, kind: string) => invoke<string>("object_ddl", { id, schema, name, kind }),
  objectIndex: (id: string) => invoke<IndexEntry[]>("object_index", { id }),
  /** 접속하지 않은 Data Source 의 지난 객체·컬럼 목록 (접속해서 읽을 때마다 저장된다) */
  cachedIndex: () => invoke<CachedIndex[]>("cached_index"),
  warmSearchCache: (id: string) => invoke<void>("warm_search_cache", { id }),
  fullTextSearch: (id: string, req: FtsRequest) => invoke<FtsHit[]>("full_text_search", { id, req }),
};
/** Redis 기본 DB 이름: "" / "3" / "db3" → "db3" */
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

/** 따옴표 없이 쓰면 문법이 깨지는 예약어 (core sql.rs RESERVED 와 같은 목록) */
const RESERVED = new Set(
  ("all analyse analyze and any array as asc asymmetric between both by case cast change check collate column condition constraint create cross " +
    "current_date current_role current_time current_timestamp current_user database default deferrable delete desc distinct do drop else end except " +
    "exists false fetch for foreign from full grant group having in index initially inner insert interval intersect into is join key keys lateral " +
    "leading left like limit localtime localtimestamp match natural not null offset on only or order outer placing primary range rank read references " +
    "returning right row rows select session_user set some symmetric table then to trailing true union unique update usage user using values variadic " +
    "when where window with write").split(" "),
);
export const quoteIdent = (s: string) => (/^[a-z_][a-z0-9_]*$/.test(s) && !RESERVED.has(s) ? s : `"${s.replace(/"/g, '""')}"`);

/** DataGrip처럼 접속을 구분하는 색 (어두운 배경용) */
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

/** 데이터 편집 SQL에 쓰는 리터럴. PG가 컬럼 타입으로 캐스팅한다. */
export const sqlLiteral = (v: Cell) => (v === null ? "NULL" : `'${String(v).replace(/'/g, "''")}'`);

/** DB 종류별 SQL 방언: 식별자 따옴표, 문자열 리터럴, 텍스트 캐스트 */
export interface Dialect {
  q: (s: string) => string;
  lit: (v: Cell) => string;
  text: (expr: string) => string;
  monaco: string;
  /** 콘솔 문장 나누기 (없으면 세미콜론 기준) */
  split?: (text: string) => { start: number; end: number }[];
}
const PG: Dialect = { q: quoteIdent, lit: sqlLiteral, text: (e) => `${e}::text`, monaco: "pgsql" };
const MY: Dialect = {
  q: (s) => (/^[A-Za-z_][A-Za-z0-9_]*$/.test(s) && !RESERVED.has(s.toLowerCase()) ? s : "`" + s.replace(/`/g, "``") + "`"),
  // MySQL 문자열에서는 백슬래시도 이스케이프 문자
  lit: (v) => (v === null ? "NULL" : `'${String(v).replace(/\\/g, "\\\\").replace(/'/g, "''")}'`),
  text: (e) => `CAST(${e} AS CHAR)`,
  monaco: "mysql",
};
/** Redis: 한 줄 = 명령 하나 (# 주석) */
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
/** 드라이버 DB: 표준 SQL 따옴표, 글 변환은 CAST (PG 의 ::text 는 쓰지 않는다) */
const GEN: Dialect = { ...PG, text: (e) => `CAST(${e} AS VARCHAR(4000))`, monaco: "sql" };
const MONGO: Dialect = { q: (s) => s, lit: sqlLiteral, text: (e) => e, monaco: "javascript", split: RD.split };
/** REST 요청 하나 = 메서드 줄부터 다음 메서드 줄 전까지 (본문 JSON 포함). core 의 sql::split_rest 와 같은 규칙 */
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

/** 백엔드 접속 대상: Data Source id + DB 이름 */
export const targetId = (connId: string, database?: string | null) => (database ? `${connId}::${database}` : connId);

/** 스키마 선택(DataGrip "N of M" 팝업) 규칙. 특수값: "*" = 전부, "@default" = 기본 DB / 기본 스키마(public) */
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
