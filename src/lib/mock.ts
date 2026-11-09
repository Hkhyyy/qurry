/** For development: opening in a browser with ?mock uses fake data instead of the Tauri backend (for checking screens). */
// A real font file for the added custom fonts (FontFace has to load something): Monaco's icon font (by path: the package exports only .js)
import fontBytesUrl from "/node_modules/monaco-editor/esm/vs/base/browser/ui/codicons/codicon/codicon.ttf?url";
import { shared } from "./shared";
import THEMES from "../../crates/shared/src/themes.json";
const conns = [
  { id: "dev", name: "[DEV] shop_app", kind: "postgres", host: "dev-db.example.com", port: 5432, user: "app", database: "shop_app", color: "#5f5b24", hasPassword: true, connected: true },
  { id: "prod", name: "[PROD] shop_app", kind: "postgres", host: "prod-db", port: 5432, user: "app", database: "shop_app", color: "#6e2a2a", hasPassword: true, connected: false, options: { confirmChanges: true },
    ssh: { enabled: true, host: "bastion.example.com", port: 22, user: "ec2-user", auth: "key", keyPath: "~/.ssh/id_ed25519" } },
  { id: "my", name: "[MySQL] shop", kind: "mysql", host: "127.0.0.1", port: 13306, user: "root", database: "shop", color: "#22524f", hasPassword: true, connected: true, options: { confirmChanges: true } },
  { id: "rd", name: "[Redis] cache", kind: "redis", host: "127.0.0.1", port: 16379, user: "", database: "0", color: "#6e2a2a", hasPassword: false, connected: true },
  { id: "local", name: "[LOCAL] TestServer", kind: "postgres", host: "localhost", port: 5432, user: "dev", database: "", color: null, hasPassword: false, connected: false },
  // A remote server without TLS: connects only after allow_plaintext. window.__mockLanProd (set before load) makes it Production
  { id: "lan", name: "LAN dev", kind: "postgres", host: "10.0.0.12", port: 5432, user: "dev", database: "lan_app", color: null, hasPassword: true, connected: false, options: { confirmChanges: !!(globalThis as any).__mockLanProd, allowPlaintext: false } },
];
// The core's NEEDS_TLS for PostgreSQL (qurry_shared::errors::needs_tls_pg); the GUI recognises it through wasm tlsOptIn
const NEEDS_TLS =
  'This server didn\'t set up an encrypted (TLS) connection, so Qurry didn\'t send the password. If it\'s an old server and the network is trusted, turn on "Allow unencrypted connection" for this data source (GUI: Options tab · TUI: data source form, "Allow unencrypted").\n(error communicating with database: TLS required)';
// The core's error for a missing Session Manager plugin (ssm::explain): one of the longest, and what to do comes last
const SSM_NO_PLUGIN =
  "The AWS Session Manager plugin is not installed. Install it (macOS: brew install --cask session-manager-plugin; others: https://docs.aws.amazon.com/systems-manager/latest/userguide/session-manager-working-with-install-plugin.html), then try again.";
const lan: any = conns.find((c) => c.id === "lan");
// What the TLS e2e test counts
const tls = ((globalThis as any).__mockTls = { allow: 0, runs: 0, saves: 0 });
/** Throws NEEDS_TLS for "LAN dev" targets (id or id::db) until plain text is allowed */
const lanCheck = (id: unknown) => {
  if (String(id).split("::")[0] === "lan" && !lan.options.allowPlaintext) throw NEEDS_TLS;
};
/** Table Sizes of a report target: the default database's tables are big, the others' small */
const tableSizes = (id: string) => {
  const [base, db] = id.split("::");
  const big = !db || db === conns.find((c) => c.id === base)?.database;
  return {
    columns: ["schema", "table", "total", "data", "indexes"].map((name) => ({ name, typeName: "TEXT" })),
    rows: big ? [["public", "orders", "182 MB", "120 MB", "62 MB"], ["public", "order_items", "41 MB", "30 MB", "11 MB"]] : [["public", "orders", "2 MB", "1 MB", "1 MB"]],
    rowsAffected: 0, truncated: false, elapsedMs: 5, hasResultSet: true,
  };
};
const result = {
  columns: [
    { name: "id", typeName: "INT8" },
    { name: "user_name", typeName: "VARCHAR" },
    { name: "amount", typeName: "NUMERIC" },
    { name: "created_at", typeName: "TIMESTAMPTZ" },
    { name: "memo", typeName: "TEXT" },
  ],
  rows: Array.from({ length: 500 }, (_, i) => [i + 1, `user_${i + 1}`, (i * 1234.5).toFixed(2), `2026-10-15 12:00:00${["", ".5", ".125"][i % 3]} +09:00`, i % 3 ? null : "note"]),
  rowsAffected: 0,
  truncated: true,
  elapsedMs: 12,
  hasResultSet: true,
};
/** `select … from wide_demo`: many columns and rows (scroll both ways) */
const wide = {
  ...result,
  columns: Array.from({ length: 30 }, (_, c) => ({ name: `column_${c + 1}`, typeName: c % 2 ? "VARCHAR" : "INT8" })),
  rows: Array.from({ length: 500 }, (_, r) => Array.from({ length: 30 }, (_, c) => (c % 2 ? `value ${r + 1}.${c + 1}` : r * 30 + c))),
  truncated: false,
};
/** `select … from cjk_demo`: a join's two `id` columns and Hangul text (wide characters) */
const cjkDemo = {
  ...result,
  columns: [{ name: "id", typeName: "INT8" }, { name: "memo", typeName: "TEXT" }, { name: "id", typeName: "INT8" }],
  rows: Array.from({ length: 20 }, (_, i) => [i + 1, i === 3 ? "배송 전에 꼭 전화 주세요 고객 요청 사항" : "고객 요청", 100 + i]),
  truncated: false,
};
/** `select … from json_demo`: a jsonb column (colored cells); row 3 holds a ~100 KB value */
const jsonDemo = {
  ...result,
  columns: [{ name: "id", typeName: "INT8" }, { name: "payload", typeName: "JSONB" }],
  rows: Array.from({ length: 50 }, (_, i) => [
    i + 1,
    i === 2
      ? JSON.stringify({ items: Array.from({ length: 4000 }, (_, k) => ({ sku: `A-${k}`, qty: k })) })
      : JSON.stringify({ type: "payment.paid", amount: 1000 * i, live: i % 2 === 0, memo: "고객 요청 🙏", meta: { pg: "toss", coupon: null } }),
  ]),
  truncated: false,
};
/** `select … from payments`: a jsonb column as PostgreSQL sends it (one line, spaces after : and ,), a NULL and a text column holding JSON */
const payments = {
  ...result,
  columns: [{ name: "id", typeName: "INT8" }, { name: "status", typeName: "VARCHAR" }, { name: "data", typeName: "JSONB" }, { name: "note", typeName: "TEXT" }],
  rows: Array.from({ length: 40 }, (_, i) => [i + 1, i % 2 ? "paid" : "ready", i === 2 ? null : `{"kind": "card", "meta": {"src": "web", "ip": "10.0.0.${i + 1}"}, "items": [${i}, ${i + 1}]}`, i % 4 ? "plain note" : "[1, 2]"]),
  truncated: false,
};
/** `select … from crm.voc vo left join users pu …` (#102): edited per table, each by its own key. voc 2 has no user (the outer side is
 *  NULL there), `shout` is computed */
const vocJoin = {
  ...result,
  columns: ["idx", "user_idx", "status", "idx", "role", "status", "shout"].map((name, i) => ({ name, typeName: i === 6 || i === 2 || i === 4 || i === 5 ? "VARCHAR" : "INT8" })),
  rows: [[1, 10, "open", 10, "member", "active", "MEMBER"], [2, 99, "new", null, null, null, null], [3, 11, "closed", 11, "admin", "active", "ADMIN"]],
  truncated: false,
  editJoin: {
    tables: [{ schema: "crm", table: "voc", pk: ["idx"], pkAt: [0] }, { schema: "public", table: "users", pk: ["idx"], pkAt: [3] }],
    columns: [{ table: 0, column: "idx" }, { table: 0, column: "user_idx" }, { table: 0, column: "status" }, { table: 1, column: "idx" }, { table: 1, column: "role" }, { table: 1, column: "status" }, { readOnly: "Read-only: shout is computed, not a column of the table." }],
  },
};
/** A console result's edit target or why it has none, as the backend says (#86): `select * from users` is one table with its key; a
 *  join of voc and users edits per table (vocJoin), other joins lack a key */
const consoleEdit = (sql: string) =>
  /\bjoin\b/i.test(sql)
    ? { readOnlyReason: "Read-only: the result doesn't include the primary key of public.orders (id)." }
    : /^\s*select\s+\*\s+from\s+(public\.)?users\s*;?\s*$/i.test(sql)
      ? { edit: { schema: "public", table: "users", pk: ["id"], columns: result.columns.map((c) => c.name) } }
      : {};
/** `select * from users limit n`: the first n rows, editable as `select * from users` is (a short result: Storybook's theme preview) */
const usersLimit = (sql: string) => {
  const m = /^\s*select\s+\*\s+from\s+(public\.)?users\s+limit\s+(\d+)\s*;?\s*$/i.exec(sql);
  return m ? { ...result, rows: result.rows.slice(0, Number(m[2])), truncated: false, edit: { schema: "public", table: "users", pk: ["id"], columns: result.columns.map((c) => c.name) } } : null;
};
let flakyLoads = 0;
let updateChannel = "stable";
/** Font files added in Settings: each "Add Font File…" adds "Fira Code" (then "Fira Code 2"…, as the backend names a taken name) */
let customFonts: { name: string; file: string }[] = [];
/** settings.json "theme" / "themeSync" / "customThemes" (kept across a reload, as the file is; read on each call, so a test can change
 *  it as the TUI would). Custom themes are checked as the core checks them (the shared WASM's theme file rules) */
const themeSettings = () => JSON.parse(localStorage.getItem("mock-themeSettings") ?? "null") ?? { theme: null, sync: { dark: "dark", light: "light" }, custom: [] };
const saveThemeSettings = (s: unknown) => localStorage.setItem("mock-themeSettings", JSON.stringify(s));
const checkTheme = (t: any) => {
  shared().parseThemeFile(shared().themeFileText({ ...t, id: "" }));
};
/** settings.json "keywordCase" (kept across a reload, as the file is) */
let keywordCase = localStorage.getItem("mock-keywordCase") ?? "off";
/** settings.json "gridAlign" (kept across a reload, as the file is) */
let gridAlign = JSON.parse(localStorage.getItem("mock-gridAlign") ?? "{}");
/** settings.json "columnWidths": table (source · database · schema · table) → its set widths (kept across a reload, as the file is;
 *  read on each call, as the core reads the file, so a test can change it as the TUI would) */
const columnWidths = (): Record<string, Record<string, number>> => JSON.parse(localStorage.getItem("mock-columnWidths") ?? "{}");
const widthsKey = (t: any) => JSON.stringify([t.source, t.database, t.schema, t.table]);
const saveWidths = (all: Record<string, Record<string, number>>) => localStorage.setItem("mock-columnWidths", JSON.stringify(all));
/** Whether check_update ran (the real app then has a saved check that a channel switch answers from) */
let updateChecked = false;
/** A delayed load-more in flight (__mockFetchMoreDelay): cancel_query makes it fail as the server does */
let cancelMore: (() => void) | null = null;
/** Test Connections waiting for cancel_test, by test id */
const testsRunning: Record<string, () => void> = {};
// Import Data Sources (core import.rs): the locations found and a preview in the core's words
const IMPORT_LOCATIONS = [
  { label: "DataGrip 2025.2 · global", path: "/Users/me/Library/Application Support/JetBrains/DataGrip2025.2/options", shown: "~/Library/Application Support/JetBrains/DataGrip2025.2/options", count: 7 },
  { label: "IntelliJ IDEA · project shop-api", path: "/Users/me/work/shop-api/.idea", shown: "~/work/shop-api/.idea", count: 3 },
  { label: "DBeaver · workspace6 General", path: "/Users/me/Library/DBeaverData/workspace6/General/.dbeaver", shown: "~/Library/DBeaverData/workspace6/General/.dbeaver", count: 6 },
];
const IMPORT_NOTE = "New data sources start with MCP Off. Passwords aren't imported: each asks on its first connect.";
export function importPlan(from: string, path: string) {
  const shown = path.replace(/^\/Users\/me/, "~");
  const rows = [
    { text: '+ add "orders-prod" (app@orders-db.internal:5432/orders)', note: "PostgreSQL · SSH ec2-user@bastion.example.com (key ~/.ssh/id_ed25519) · SSL verify-full (CA ~/certs/orders-ca.pem) · Read-only", value: "password on connect", tone: null, checkable: true },
    { text: '+ add "analytics" (analyst@10.0.3.21:3306/stats)', note: "MySQL · SSL require", value: "SSL without certificate checks", tone: "warn", checkable: true },
    { text: '+ add "events" (default@ch.internal:8123/default)', note: "ClickHouse · SSH deploy@jump-eu.example.com:2222 (ssh-agent)", value: "password on connect", tone: null, checkable: true },
    { text: '+ add "local-pg" (postgres@localhost:5432/postgres)', note: "PostgreSQL", value: "no password", tone: null, checkable: true },
    { text: '+ add "cache" (localhost:6379/0)', note: "Redis", value: "no password", tone: null, checkable: true },
    { text: '− skip "warehouse" (Snowflake)', note: "Snowflake isn't supported", value: "skipped", tone: null, checkable: false },
    { text: '− skip "billing" (PostgreSQL)', note: "already in Qurry (same server, user and database)", value: "skipped", tone: null, checkable: false },
  ];
  // window.__mockImportAllSkipped: every source is skipped (a notice instead of the confirm)
  if ((window as any).__mockImportAllSkipped) {
    const skipped = rows.filter((r) => !r.checkable);
    return { title: "Import Data Sources", message: `Nothing to import from ${from}: its ${skipped.length} data sources are skipped.\n${shown}`, rows: skipped, importable: 0 };
  }
  return { title: "Import Data Sources", message: `Import these data sources from ${from}?\n${shown}\n\n5 to add, 2 skipped. ${IMPORT_NOTE}`, rows, importable: 5 };
}

/** [Redis] cache's keys: db0 (the default) and db3 */
const redisKeys = (id: string) =>
  String(id).endsWith("::db3")
    ? [
        { key: "jobs:1", type: "hash", ttl: -1 },
        { key: "jobs:2", type: "hash", ttl: 600_000 },
      ]
    : [
        { key: "session:abc", type: "string", ttl: 3_540_000 },
        { key: "user:1", type: "hash", ttl: -1 },
        { key: "user:1:name", type: "string", ttl: -1 },
        { key: "queue:mail", type: "list", ttl: -1 },
        { key: "tags", type: "set", ttl: -1 },
        { key: "rank", type: "zset", ttl: 101_000_000 },
        { key: "events", type: "stream", ttl: -1 },
      ];
/** SCAN MATCH with * and ? only */
const redisMatch = (pattern: string | undefined, key: string) =>
  new RegExp(`^${(pattern?.trim() || "*").replace(/[.+^${}()|\\]/g, "\\$&").replace(/\*/g, ".*").replace(/\?/g, ".")}$`).test(key);
const historyAdded: { id: string; connId: string; database?: string; sql: string; ts: number }[] = [];
const handlers: Record<string, (a: any) => unknown> = {
  // A copy, as from the backend: state that holds the last list must see a change (Allow unencrypted, connect) as a new list
  // window.__mockRedis (set before load): { readOnly, production, managed } for [Redis] cache (managed: CONFIG refused, as on ElastiCache)
  list_connections: () =>
    structuredClone(conns).map((c: any) => {
      const r = (globalThis as any).__mockRedis;
      return c.id === "rd" && r ? { ...c, readOnly: !!r.readOnly, options: { ...c.options, confirmChanges: !!r.production } } : c;
    }),
  // window.__mockDbListFail: the account can't connect to a source's default database, so listing through the source itself fails.
  // window.__mockDbListDelay (ms): the list arrives that late (a select asked for it only on its first click would open without it)
  list_databases: (a: any) => {
    const g = globalThis as any;
    lanCheck(a.id);
    if (g.__mockDbListFail && !String(a.id).includes("::")) throw `permission denied for database "${conns.find((c) => c.id === a.id)?.database}"`;
    // window.__mockDbRefuse: a database the account can't connect to (listing through it and reports on it fail)
    if (g.__mockDbRefuse && String(a.id).endsWith(`::${g.__mockDbRefuse}`)) throw `permission denied for database "${g.__mockDbRefuse}"`;
    const v = a.id === "lan" ? ["lan_app"] : a.id === "rd" ? ["db0", "db3"] : a.id === "my" ? ["information_schema", "mysql", "shop", "sys"] : ["postgres", "shop_app", "shop_app_test"];
    return g.__mockDbListDelay ? new Promise((ok) => setTimeout(() => ok(v), g.__mockDbListDelay)) : v;
  },
  redis_scan: (a: any) => ({
    cursor: "0",
    keys: redisKeys(a.id).filter((k) => (!a.typeFilter || k.type === a.typeFilter) && redisMatch(a.pattern, k.key)),
  }),
  redis_overview: () => ({ configAllowed: !(globalThis as any).__mockRedis?.managed, databases: [{ name: "db0", keys: 7 }, { name: "db3", keys: 2 }] }),
  redis_key_names: (a: any) => redisKeys(a.id).map((k) => k.key).filter((k) => k.startsWith(a.prefix)).sort().slice(0, a.max),
  redis_key: (a: any) =>
    a.key === "user:1"
      ? { type: "hash", ttl: -1, length: 3, memory: 96, encoding: "listpack", truncated: false, value: [["age", "30"], ["email", "a@b.c"], ["profile", '{"city":"Seoul","tags":["vip","beta"]}']] }
      : a.key === "rank"
      ? { type: "zset", ttl: 86400000, length: 2, memory: 72, encoding: "listpack", truncated: false, value: [["alice", "10"], ["bob", "20"]] }
      : { type: "string", ttl: 3540000, length: 42, memory: 80, encoding: "embstr", truncated: false, value: '{"uid":1,"roles":["admin"],"exp":1760000000}' },
  redis_exec: () => ({ status: "OK" }),
  // As the backend: a read-only source refuses writes; replies by command (window.__mockRedisLines lists the lines that ran)
  redis_exec_line: (a: any) => {
    const g = globalThis as any;
    const cmd = String(a.line).trim().split(/\s+/)[0].toUpperCase();
    if (g.__mockRedis?.readOnly && !["GET", "HGETALL", "LRANGE", "SMEMBERS", "ZRANGE", "TTL", "TYPE", "DBSIZE", "PING", "SCAN"].includes(cmd)) throw `Data source "[Redis] cache" is read-only: ${cmd} changes data`;
    (g.__mockRedisLines ??= []).push(a.line);
    const reply: Record<string, unknown> = {
      HGETALL: ["age", "30", "email", "a@b.c"],
      LRANGE: ["mail-1", "mail-2", "mail-3"],
      SMEMBERS: ["beta", "vip"],
      ZRANGE: /withscores/i.test(a.line) ? ["alice", "10", "bob", "20"] : ["alice", "bob"],
      INCR: 7,
      GET: null,
      DBSIZE: 7,
      FLUSHDB: { status: "OK" },
    };
    if (cmd === "NOSUCH") throw "Redis: ERR unknown command 'NOSUCH', with args beginning with: ";
    return cmd in reply ? reply[cmd] : { status: "OK" };
  },
  warm_search_cache: () => null,
  cached_index: () => [
    { target: "prod::shop_app", objects: [{ schema: "public", name: "payments", kind: "table" }, { schema: "public", name: "refunds", kind: "table" }], columns: [{ schema: "public", name: "refunds", columns: [["payment_id", "bigint"]] }] },
  ],
  object_index: () => [
    { schema: "public", name: "public", kind: "schema" },
    { schema: "billing", name: "billing", kind: "schema" },
    { schema: "public", name: "users", kind: "table" },
    { schema: "public", name: "user_payment_history", kind: "table" },
    { schema: "public", name: "payments", kind: "table" },
    { schema: "public", name: "contracts", kind: "table" },
    { schema: "public", name: "active_users", kind: "view" },
    { schema: "public", name: "users_id_seq", kind: "sequence" },
    { schema: "public", name: "calc_fee", kind: "function" },
  ],
  full_text_search: (a: any) => [
    { schema: "public", table: "users", whereClause: `"user_name"::text ilike '%${a.req.text}%'`, result: { ...result, rows: result.rows.slice(0, 3), truncated: false } },
  ],
  list_schemas: (a: any) => (String(a.id).startsWith("rd") ? [{ name: String(a.id).split("::")[1] || "db0", isSystem: false }] : null) ?? (String(a.id).startsWith("my") ? [{ name: String(a.id).split("::")[1] || "shop", isSystem: false }] : null) ?? [{ name: "public", isSystem: false }, { name: "billing", isSystem: false }, { name: "pg_catalog", isSystem: true }],
  list_objects: (a: any) => String(a.id).startsWith("rd") ? { relations: [["events", "stream"], ["rank", "zset"], ["session:abc", "string"], ["tags", "set"], ["user:1", "hash"]].map(([name, t]) => ({ name, kind: "key", comment: t, rowEstimate: null })), routines: [] } : a.schema === "billing" ? {
    // users in two schemas (same-name table tabs)
    relations: [{ name: "invoices", kind: "table", comment: null, rowEstimate: 800 }, { name: "users", kind: "table", comment: "billing accounts", rowEstimate: 40 }],
    routines: [],
  } : ({
    relations: [
      { name: "contracts", kind: "table", comment: "계약", rowEstimate: 1200 },
      { name: "payments", kind: "table", comment: null, rowEstimate: 50000 },
      { name: "users", kind: "table", comment: "회원", rowEstimate: 3000 },
      { name: "v_active_users", kind: "view", comment: null, rowEstimate: null },
      { name: "users_id_seq", kind: "sequence", comment: null, rowEstimate: null },
    ],
    routines: [{ name: "calc_fee", args: "amount numeric", returns: "numeric", kind: "function" }],
  }),
  table_detail: () => ({
    columns: [
      { name: "id", dataType: "bigint", nullable: false, defaultValue: "nextval('users_id_seq'::regclass)", comment: null, isPk: true },
      { name: "user_name", dataType: "character varying(50)", nullable: false, defaultValue: null, comment: "이름", isPk: false },
      { name: "contract_id", dataType: "bigint", nullable: true, defaultValue: null, comment: null, isPk: false },
      { name: "created_at", dataType: "timestamp with time zone", nullable: false, defaultValue: "now()", comment: null, isPk: false },
    ],
    indexes: [{ name: "users_pkey", definition: "CREATE UNIQUE INDEX users_pkey ON public.users USING btree (id)", isUnique: true, isPrimary: true }],
    constraints: [
      { name: "users_pkey", kind: "p", definition: "PRIMARY KEY (id)" },
      { name: "users_contract_fk", kind: "f", definition: "FOREIGN KEY (contract_id) REFERENCES contracts(id)" },
    ],
  }),
  // DDL tab → Structure: every section filled. billing.* answers like a driver plugin (only columns · keys · indexes, no index
  // columns); window.__mockStructureFail (a message) makes it fail; window.__mockStructureCalls counts the loads
  table_structure: (a: any) => {
    (globalThis as any).__mockStructureCalls = ((globalThis as any).__mockStructureCalls ?? 0) + 1;
    const fail = (globalThis as any).__mockStructureFail;
    if (fail) throw fail;
    const columns = [
      { name: "id", dataType: "bigint", nullable: false, defaultValue: "nextval('users_id_seq'::regclass)", comment: null, isPk: true },
      { name: "user_name", dataType: "character varying(50)", nullable: false, defaultValue: null, comment: "이름", isPk: false },
      { name: "email", dataType: "text", nullable: true, defaultValue: "concat('user-', currval('users_id_seq'::regclass), '@', current_setting('app.mail_domain'))", comment: "Login and receipts; unique when set", isPk: false },
      { name: "contract_id", dataType: "bigint", nullable: true, defaultValue: null, comment: null, isPk: false },
      { name: "created_at", dataType: "timestamp with time zone", nullable: false, defaultValue: "now()", comment: null, isPk: false },
    ];
    const t = `${a.schema}.${a.table}`;
    const basics = {
      columns,
      indexes: [
        { name: `${a.table}_pkey`, definition: `CREATE UNIQUE INDEX ${a.table}_pkey ON ${t} USING btree (id)`, isUnique: true, isPrimary: true, columns: ["id"] },
        { name: `${a.table}_email_key`, definition: `CREATE UNIQUE INDEX ${a.table}_email_key ON ${t} USING btree (lower(email))`, isUnique: true, isPrimary: false, columns: ["lower(email)"] },
        { name: `${a.table}_name_created`, definition: `CREATE INDEX ${a.table}_name_created ON ${t} USING btree (user_name, created_at DESC)`, isUnique: false, isPrimary: false, columns: ["user_name", "created_at"] },
      ],
      constraints: [
        { name: `${a.table}_pkey`, kind: "p", definition: "PRIMARY KEY (id)" },
        { name: `${a.table}_contract_fk`, kind: "f", definition: "FOREIGN KEY (contract_id) REFERENCES contracts(id) ON DELETE SET NULL" },
        { name: `${a.table}_email_key`, kind: "u", definition: "UNIQUE (email)" },
        { name: `${a.table}_name_check`, kind: "c", definition: "CHECK (length(user_name) > 0)" },
      ],
    };
    if (a.schema === "billing") return { ...basics, indexes: basics.indexes.map((i) => ({ ...i, columns: [] })), foreignKeys: null, referencedBy: null, triggers: null, options: null, comment: null, onlyVisible: false };
    const fk = (name: string, schema: string, table: string, columns: string[], refTable: string, onDelete: string) => ({ name, schema, table, columns, refSchema: "public", refTable, refColumns: ["id"], onUpdate: "NO ACTION", onDelete });
    return {
      ...basics,
      foreignKeys: [fk(`${a.table}_contract_fk`, a.schema, a.table, ["contract_id"], "contracts", "SET NULL")],
      referencedBy: [fk("orders_user_fk", "public", "orders", ["user_id"], a.table, "CASCADE"), fk("payments_user_fk", "public", "payments", ["user_id"], a.table, "RESTRICT")],
      triggers: [{ name: `${a.table}_touch`, timing: "BEFORE", events: "INSERT OR UPDATE", definition: `CREATE TRIGGER ${a.table}_touch BEFORE INSERT OR UPDATE ON ${t} FOR EACH ROW EXECUTE FUNCTION touch_updated_at()` }],
      options: [["Owner", "app"], ["Tablespace", "pg_default"], ["Storage parameters", "fillfactor=90"]],
      comment: "Members of the shop",
      onlyVisible: false,
    };
  },
  // MySQL: the schema is the database (shop is the data source's default)
  // Foreign keys (FK-aware joins) and a user function (signature help) as completion_meta returns them
  completion_meta: (a: any) => a.id.split("::")[0] === "my" ? [{ schema: "shop", name: "orders", columns: [["id", "bigint"], ["status", "varchar"]] }] : [
    { schema: "public", name: "users", columns: [["id", "bigint"], ["user_name", "varchar"]] },
    { schema: "public", name: "orders", columns: [["id", "bigint"], ["user_id", "bigint"], ["status", "varchar"], ["amount", "numeric"], ["data", "jsonb"], ["reasons", "jsonb"]],
      fks: [{ columns: ["user_id"], refSchema: "public", refTable: "users", refColumns: ["id"] }] },
    { schema: "public", name: "order_total", columns: [], routine: { args: "order_id bigint, with_tax boolean", returns: "numeric" } },
  ],
  // Shared workspace list (mimics the file shared with the TUI). Tests replace it via window.__sharedWs
  shared_workspaces: () => (window as any).__sharedWs ?? null,
  adopt_workspaces: (a: any) => {
    const w = window as any;
    if (!w.__sharedWs) w.__sharedWs = a.local;
    return { list: w.__sharedWs, renamed: [] };
  },
  sync_workspaces: (a: any) => {
    const w = window as any;
    const list: any[] = (w.__sharedWs ?? a.prev).filter((x: any) => a.next.some((n: any) => n.id === x.id) || !a.prev.some((p: any) => p.id === x.id));
    for (const n of a.next) {
      const i = list.findIndex((x) => x.id === n.id);
      if (i < 0) list.push(n);
      else list[i] = { ...list[i], name: n.name, connIds: n.connIds };
    }
    w.__sharedWs = list;
    return list;
  },
  column_hints: (a: { req: { column: string; path: string[]; keys: boolean; pairs?: boolean } }) =>
    a.req.pairs
      ? [{ value: '[{"code": "out_of_scope_payment_type"}]', count: 120 }, { value: '[{"code": "self_transfer"}]', count: 31 }]
      : a.req.keys && a.req.column === "reasons"
      ? []
      : a.req.keys
      ? a.req.path.length ? [{ value: "src", count: 30 }, { value: "ip", count: 12 }] : [{ value: "kind", count: 30 }, { value: "meta", count: 30 }, { value: "coupon", count: 4 }]
      : a.req.column === "status" ? [{ value: "ready", count: 20 }, { value: "paid", count: 10 }, { value: "it's late", count: 1 }]
      : a.req.column === "amount" ? [{ value: "1000", count: 7 }, { value: "2500", count: 3 }]
      : [{ value: "k0", count: 15 }, { value: "k1", count: 15 }],
  // A statement on no_such_table fails (console gutter marks) at its position, as PostgreSQL says it (qurry_shared::errors::with_position)
  run_query: (a: any) =>
    /\bno_such_table\b/i.test(a.sql) ? Promise.reject(`ERROR: relation "no_such_table" does not exist (position ${[...a.sql.slice(0, a.sql.search(/\bno_such_table\b/i))].length + 1})`) : (String(a.id).startsWith("lan") && (tls.runs++, lanCheck(a.id)), /^EXPLAIN/i.test(a.sql)
      ? { ...result, columns: [{ name: "QUERY PLAN", typeName: "JSON" }], rows: [[JSON.stringify([{ Plan: { "Node Type": "Hash Join", "Join Type": "Inner", "Startup Cost": 1.5, "Total Cost": 45.2, "Plan Rows": 120, "Plan Width": 64, "Hash Cond": "(u.contract_id = c.id)", Plans: [{ "Node Type": "Seq Scan", "Relation Name": "users", Schema: "public", Alias: "u", "Startup Cost": 0, "Total Cost": 22.1, "Plan Rows": 1200, "Plan Width": 40 }, { "Node Type": "Hash", "Startup Cost": 1.2, "Total Cost": 1.2, "Plan Rows": 12, "Plan Width": 24, Plans: [{ "Node Type": "Index Scan", "Relation Name": "contracts", "Index Name": "contracts_pkey", "Startup Cost": 0.1, "Total Cost": 1.2, "Plan Rows": 12, "Plan Width": 24 }] }] } }])]], truncated: false }
      : /^\s*(insert|update|delete)/i.test(a.sql)
        ? { ...result, columns: [], rows: [], rowsAffected: 3, truncated: false, hasResultSet: false, inTransaction: (globalThis as any).__mockManual === true }
        : /\bwide_demo\b/i.test(a.sql)
          ? { ...wide, inTransaction: (globalThis as any).__mockManual === true }
          : /\bcjk_demo\b/i.test(a.sql)
          ? { ...cjkDemo, inTransaction: (globalThis as any).__mockManual === true }
          : /\bjson_demo\b/i.test(a.sql)
          ? { ...jsonDemo, inTransaction: (globalThis as any).__mockManual === true }
          : /\bpayments\b/i.test(a.sql)
          ? { ...payments, inTransaction: (globalThis as any).__mockManual === true }
          : /\bvoc\b[\s\S]*\bjoin\b/i.test(a.sql)
          ? { ...vocJoin, inTransaction: (globalThis as any).__mockManual === true }
          : { ...(usersLimit(a.sql) ?? { ...result, ...consoleEdit(a.sql) }), inTransaction: (globalThis as any).__mockManual === true }),
  set_tx_mode: (a: any) => { (globalThis as any).__mockManual = a.manual; return null; },
  // e2e sets __mockCommitError for a COMMIT that fails (e.g. the connection lost while committing)
  commit_tx: () => {
    const e = (globalThis as any).__mockCommitError;
    if (e) throw e;
    return null;
  },
  rollback_tx: () => null,
  rag_indexes: () => (globalThis as any).__rag ?? ((globalThis as any).__rag = [{ id: "r1", name: "tickets search", source: "dev", schema: "public", table: "users", idColumn: "id", textColumns: ["user_name"], provider: "ollama", model: "nomic-embed-text", baseUrl: "", dest: "dev", collection: "users_embeddings", dim: 768, rows: 3000, builtAt: "2026-10-27 14:50" }]),
  rag_save: (a: any) => { const x = { ...a.index, id: a.index.id || "r" + Date.now() }; const l = (globalThis as any).__rag ?? []; (globalThis as any).__rag = [...l.filter((y: any) => y.id !== x.id), x]; return x; },
  rag_delete: (a: any) => { (globalThis as any).__rag = ((globalThis as any).__rag ?? []).filter((y: any) => y.id !== a.id); return null; },
  rag_has_key: () => false,
  rag_set_key: () => null,
  rag_build: () => 3000,
  rag_search: () => ({ ...result, columns: [{ name: "score", typeName: "FLOAT8" }, ...result.columns], rows: result.rows.slice(0, 5).map((r, i) => [0.91 - i * 0.07, ...r]), truncated: false }),
  similar_rows_sql: (a: any) => `SELECT ${a.column} <-> (SELECT ${a.column} FROM ${a.schema}.${a.table} WHERE ${a.pk.map((p: any) => `${p[0]} = '${p[1]}'`).join(" AND ")}) AS distance, *\nFROM ${a.schema}.${a.table}\nORDER BY distance\nLIMIT 20`,
  // window.__mockDdlCalls counts the loads (DDL tab → Reload); window.__mockDdlExtra is put in front of the text
  object_ddl: (a: any) => {
    (globalThis as any).__mockDdlCalls = ((globalThis as any).__mockDdlCalls ?? 0) + 1;
    return ((globalThis as any).__mockDdlExtra ?? "") + `CREATE TABLE ${a.schema}.${a.name}\n(\n    id bigint DEFAULT nextval('users_id_seq'::regclass) NOT NULL,\n    user_name character varying(50) NOT NULL,\n    CONSTRAINT users_pkey PRIMARY KEY (id)\n);\n`;
  },
  // Instance i-00000000000000000 never connects (like a stopped one): the test runs until cancel_test. On i-0000000000000dead it
  // fails with the core's long message for a missing Session Manager plugin. window.__mockTestDelay (ms, set before load) makes
  // every test take that long, as a real connect does
  test_connection: async (a: any) => {
    const delay = (globalThis as any).__mockTestDelay;
    if (delay) await new Promise((r) => setTimeout(r, delay));
    if (a.config.host === lan.host && !a.config.options?.allowPlaintext) throw NEEDS_TLS;
    if (a.config.ssm?.enabled && a.config.ssm.target === "i-0000000000000dead") throw SSM_NO_PLUGIN;
    return a.config.ssm?.enabled && a.config.ssm.target === "i-00000000000000000"
      ? new Promise((_, rej) => (testsRunning[a.testId] = () => rej("Test cancelled")))
      : a.config.ssm?.enabled
        ? `PostgreSQL 16.11  (via AWS SSM ${a.config.ssm.target})`
        : "PostgreSQL 16.11  (via SSH ec2-user@bastion.example.com)";
  },
  // Schemas tab of a new data source: connects as test_connection does (its failures, and the instance that never connects)
  form_databases: async (a: any) => {
    await handlers.test_connection(a);
    return a.config.kind === "mysql" ? ["information_schema", "mysql", "shop", "sys"] : ["postgres", "shop_app", "shop_app_test"];
  },
  form_schemas: (a: any) => (a.config.kind === "mysql" ? [{ name: a.database, isSystem: false }] : [{ name: "public", isSystem: false }, { name: "billing", isSystem: false }, { name: "pg_catalog", isSystem: true }]),
  cancel_test: (a: any) => {
    testsRunning[a.testId]?.();
    delete testsRunning[a.testId];
    return null;
  },
  connect: (a: any) => {
    lanCheck(a.id);
    const c: any = conns.find((c) => c.id === a.id);
    if (c) c.connected = true;
    return "PostgreSQL 16.11 on mock";
  },
  allow_plaintext: () => {
    if (lan.options.confirmChanges) throw `"${lan.name}" is a Production data source, so Qurry won't turn off encryption for it from here.`;
    tls.allow++;
    lan.options = { ...lan.options, allowPlaintext: true };
    return lan;
  },
  save_connection: (a: any) => {
    tls.saves++;
    const i = conns.findIndex((c: any) => c.id === a.config.id);
    if (i >= 0) (conns as any)[i] = { ...conns[i], ...a.config };
    // A new data source gets its id here, as in the core
    if (!a.config.id) {
      const saved = { ...a.config, id: `new${conns.length}`, hasPassword: !!a.password, connected: false };
      (conns as any[]).push(saved);
      return saved;
    }
    return a.config;
  },
  set_connection_color: (a: any) => {
    const c: any = conns.find((c: any) => c.id === a.id);
    if (c) c.color = a.color;
    return c;
  },
  set_connection_scope: (a: any) => {
    const c: any = conns.find((c: any) => c.id === a.id);
    if (c) c.scope = a.scope;
    return c;
  },
  // e2e sets __mockFetchMoreError for a load-more the core refuses or that fails (e.g. after the data source reconnected)
  fetch_more: () => {
    const g = globalThis as any;
    g.__mockFetchMoreCalls = (g.__mockFetchMoreCalls ?? 0) + 1;
    if (g.__mockFetchMoreError) throw g.__mockFetchMoreError;
    const more = { ...result, rows: result.rows.slice(0, 120).map((r) => [Number(r[0]) + 500, ...r.slice(1)]), truncated: false };
    // __mockFetchMoreDelay (ms): an answer that arrives after the tab ran again, or fails as the server's does when cancelled meanwhile
    if (!g.__mockFetchMoreDelay) return more;
    return new Promise((ok, fail) => {
      const h = setTimeout(() => ((cancelMore = null), ok(more)), g.__mockFetchMoreDelay);
      cancelMore = () => (clearTimeout(h), fail("ERROR: canceling statement due to user request"));
    });
  },
  cancel_query: () => (cancelMore?.(), (cancelMore = null), true),
  close_session: () => null,
  // A console in Manual mode keeps them pending (tx_state)
  // As the core: what ran goes into the query history
  submit_edits: (a: any) => {
    const [connId, database] = String(a.id).split("::");
    for (const sql of a.statements) historyAdded.unshift({ id: `ha${historyAdded.length}`, connId, database: database || undefined, sql, ts: Date.now() });
    return { rows: a.statements.length };
  },
  load_keymap: () => ({}),
  load_workspace: () => JSON.parse(localStorage.getItem("mock-ws") ?? "null"),
  save_workspace: (a: any) => (localStorage.setItem("mock-ws", JSON.stringify(a.ws)), null),
  tx_state: () => (globalThis as any).__mockManual === true,
  explain_wrap: () => ((globalThis as any).__mockManual === true ? ["SAVEPOINT qurry_explain", "ROLLBACK TO SAVEPOINT qurry_explain"] : ["BEGIN", "ROLLBACK"]),
  // Production confirmation: rows a change would touch; like the backend, only UPDATE/DELETE are counted (null = can't count, e.g. INSERT or DDL)
  preview_impact: (a: any) => (/^\s*(update|delete)\b/i.test(a.sql) ? (/\bwhere\b/i.test(a.sql) ? 3 : 500) : null),
  // Redis as qurry_shared::guard::unsafe_statement words it
  unsafe_reason: (a: any) => a.kind === "redis" ? ((c) => (/^(FLUSHALL|FLUSHDB|DEL|UNLINK|SHUTDOWN|SWAPDB|REPLICAOF|SLAVEOF)$/.test(c) ? `${c} deletes or replaces data` : /^(EVAL|EVALSHA|EVAL_RO|EVALSHA_RO|FCALL|FUNCTION|SCRIPT)$/.test(c) ? `${c} runs a script that can change anything` : null))(String(a.sql).trim().split(/\s+/)[0].toUpperCase()) : (/^\s*(delete|update)\b(?![\s\S]*\bwhere\b)/i.test(a.sql) ? "DELETE without WHERE changes every row" : /^\s*(drop|truncate)\b/i.test(a.sql) ? "DROP removes data" : null),
  trust_ssh_host: () => "Trusted",
  export_data: (a: any) => new Promise((ok) => setTimeout(() => ok({ rows: 500, path: a.clipboard ? null : `~/Downloads/${a.defaultName}.${a.format}`, text: a.clipboard ? "id\tname\n1\ta\n" : null }), 4000)),
  cancel_job: () => null,
  // A report tab asks for the list before its first load: "flaky" starts over with a good load
  dba_reports: (a: any) => (a.id === "flaky" && (flakyLoads = 0), [
    { id: "sessions", title: "Sessions", group: "Activity", help: "Connections and what they run. blocking = how many sessions wait for this one.", actions: [{ kind: "kill", label: "Cancel Query", terminate: false }, { kind: "kill", label: "Terminate Session", terminate: true }, { kind: "sql", label: "Open Query in Console", template: "{=query}" }] },
    { id: "locks", title: "Lock Waits", group: "Activity", help: "Who waits for whom.", actions: [] },
    { id: "slow_queries", title: "Slow Queries (Top 100)", group: "Performance", help: "By total time.", actions: [{ kind: "sql", label: "Open Query in Console", template: "{=query}" }, { kind: "explain", label: "Explain in Console" }] },
    { id: "table_sizes", title: "Table Sizes", group: "Storage", help: "Biggest first.", actions: [] },
    { id: "unused_indexes", title: "Unused Indexes", group: "Storage", help: "Indexes never used since statistics were reset; counting_since tells since when.", actions: [{ kind: "sql", label: "Generate DROP INDEX", template: "DROP INDEX CONCURRENTLY {schema}.{index};" }] },
    { id: "roles", title: "Users & Roles", group: "Security", help: "Login roles and their powers. superuser / createrole are the dangerous ones.", actions: [] },
  ]),
  // Story targets: "busy" runs past the report time limit, "flaky" loads once and then its refreshes do, "maria" has no performance_schema.
  // Any target: "roles" is denied (app isn't a superuser), "table_sizes" takes 2 s (so a switch shows its loading state) and its sizes
  // depend on the database it ran on. window.__mockDbaTargets lists the targets the reports ran on
  dba_report: (a: any) =>
    (((globalThis as any).__mockDbaTargets ??= []).push(a.id),
    (globalThis as any).__mockDbRefuse && String(a.id).endsWith(`::${(globalThis as any).__mockDbRefuse}`)
      ? Promise.reject(`Couldn't connect to database ${(globalThis as any).__mockDbRefuse}. Pick another database for this report (GUI: the database list in its toolbar · TUI: d).\n(permission denied for database "${(globalThis as any).__mockDbRefuse}")`)
      : a.report === "table_sizes"
      ? new Promise((ok) => setTimeout(() => ok(tableSizes(a.id)), 2000))
      : a.report === "roles"
      ? Promise.reject("permission denied for table pg_authid")
      : a.id === "busy" || (a.id === "flaky" && flakyLoads++ > 0)
      ? Promise.reject("The report ran into its time limit (30 s, or a stricter one set on the server). The server may be busy or the statistics very large: try again later.\n(error returned from database: canceling statement due to statement timeout)")
      : a.id === "maria"
      ? Promise.reject("performance_schema / sys schema is not available on this server — enable performance_schema to see this (MariaDB has it off by default, and some of these tables only exist on MySQL).\n(error returned from database: 1146 (42S02): Table 'performance_schema.events_statements_summary_by_digest' doesn't exist)")
      : a.report === "slow_queries"
      ? {
          columns: ["total_ms", "calls", "mean_ms", "max_ms", "rows", "cache_hit_pct", "query"].map((name) => ({ name, typeName: name === "query" ? "TEXT" : "NUMERIC" })),
          rows: [
            ["91234.5", 18234, "5.00", "812.40", 18234, "99.1", "SELECT * FROM orders WHERE customer_id = $1 AND status = $2"],
            ["40211.0", 12, "3350.92", "4102.11", 1200000, "71.4", "SELECT count(*) FROM order_items"],
          ],
          rowsAffected: 0, truncated: false, elapsedMs: 4, hasResultSet: true,
        }
      : a.report === "unused_indexes"
      ? {
          columns: ["schema", "table", "index", "scans", "counting_since", "size", "definition"].map((name) => ({ name, typeName: name === "scans" ? "INT8" : name === "counting_since" ? "TIMESTAMPTZ" : "TEXT" })),
          rows: [
            ["public", "orders", "orders_status_idx", 0, "2026-09-01 04:00:12.5 +09:00", "182 MB", "CREATE INDEX orders_status_idx ON public.orders USING btree (status)"],
            ["public", "order_items", "order_items_qty_idx", 0, "2026-09-01 04:00:12.5 +09:00", "41 MB", "CREATE INDEX order_items_qty_idx ON public.order_items USING btree (qty)"],
          ],
          rowsAffected: 0, truncated: false, elapsedMs: 2, hasResultSet: true,
        }
      : {
          columns: ["pid", "user", "db", "state", "blocking", "blocked_by", "query_sec", "query"].map((name) => ({ name, typeName: name === "pid" || name === "blocking" ? "INT4" : "TEXT" })),
          rows: [
            [4123, "app", "shop_app", "idle in transaction", 2, "", "184.2", "UPDATE orders SET status = 'paid' WHERE id = 42"],
            [4188, "app", "shop_app", "active", 0, "4123", "61.0", "UPDATE orders SET status = 'shipped' WHERE id = 42"],
            [4190, "batch", "shop_app", "active", 0, "4123", "12.4", "DELETE FROM orders WHERE id = 42"],
            [4201, "dev", "shop_app", "idle", 0, "", null, "SELECT 1"],
          ],
          rowsAffected: 0, truncated: false, elapsedMs: 3, hasResultSet: true,
        }),
  kill_session: () => true,
  erd: () => {
    const c = (name: string, dataType: string, pk = false, nullable = true) => ({ name, dataType, pk, nullable: pk ? false : nullable });
    const tables = [
      { schema: "public", name: "customers", columns: [c("id", "bigint", true), c("name", "text", false, false), c("email", "text"), c("country", "char(2)")] },
      { schema: "public", name: "orders", columns: [c("id", "bigint", true), c("customer_id", "bigint", false, false), c("status", "text"), c("total", "numeric(12,2)"), c("created_at", "timestamptz")] },
      { schema: "public", name: "products", columns: [c("id", "bigint", true), c("name", "text"), c("price", "numeric(12,2)")] },
      { schema: "public", name: "order_items", columns: [c("order_id", "bigint", true), c("product_id", "bigint", true), c("qty", "integer")] },
      { schema: "billing", name: "invoices", columns: [c("id", "bigint", true), c("order_id", "bigint"), c("issued_at", "date")] },
    ];
    const r = (fs: string, ft: string, fc: string[], ts: string, tt: string, tc: string[]) => ({ name: `${ft}_${fc[0]}_fkey`, fromSchema: fs, fromTable: ft, fromColumns: fc, toSchema: ts, toTable: tt, toColumns: tc });
    return [{ tables, relations: [r("public", "orders", ["customer_id"], "public", "customers", ["id"]), r("public", "order_items", ["order_id"], "public", "orders", ["id"]), r("public", "order_items", ["product_id"], "public", "products", ["id"]), r("billing", "invoices", ["order_id"], "public", "orders", ["id"])] }, "erDiagram\n  customers {\n  }\n"];
  },
  save_file: () => null,
  compare_schemas: () => [
    { kind: "create_table", object: "orders", sql: "CREATE TABLE staging.orders (\n    id bigint NOT NULL,\n    customer_id integer,\n    CONSTRAINT orders_pkey PRIMARY KEY (id)\n);", destructive: false },
    { kind: "add_column", object: "customers.score", sql: "ALTER TABLE staging.customers ADD COLUMN score integer;", destructive: false },
    { kind: "alter_column", object: "customers.email", sql: "ALTER TABLE staging.customers ALTER COLUMN email SET NOT NULL;", destructive: false },
    { kind: "add_constraint", object: "orders.orders_customer_id_fkey", sql: "ALTER TABLE staging.orders ADD CONSTRAINT orders_customer_id_fkey FOREIGN KEY (customer_id) REFERENCES staging.customers(id);", destructive: false },
    { kind: "drop_column", object: "customers.legacy", sql: "ALTER TABLE staging.customers DROP COLUMN legacy;", destructive: true },
    { kind: "drop_table", object: "old_stuff", sql: "DROP TABLE staging.old_stuff;", destructive: true },
  ],
  compare_data: () => ({ same: 1203, different: 2, onlySource: 1, onlyTarget: 1, truncated: false, sql: "-- 1 row(s) only in the source\nINSERT INTO public.users (id, name) VALUES (7, 'kim');\n\n-- 2 row(s) with different values\nUPDATE public.users SET name = 'lee' WHERE id = 3;" }),
  pick_folder: () => "/Users/me/project/migrations",
  migration_status: () => [
    { version: "1", description: "create tables", file: "V1__create_tables.sql", checksum: "a", status: "applied", appliedAt: "2026-10-01 10:00:00+09" },
    { version: "2", description: "add index", file: "V2__add_index.sql", checksum: "b", status: "applied", appliedAt: "2026-10-05 10:00:00+09" },
    { version: "3", description: "sync staging", file: "V3__sync_staging.sql", checksum: "c", status: "pending", appliedAt: null },
  ],
  apply_migrations: () => ["3"],
  save_migration: (a: any) => `${a.dir}/V4__x.sql`,
  // Kept for the session, newest first; the core masks passwords (AUTH …) as history::redact does
  history_add: (a: any) => void historyAdded.unshift({ id: `ha${historyAdded.length}`, connId: a.connId, database: a.database ?? undefined, sql: String(a.sql).replace(/^(\s*auth\s+).*$/i, "$1***"), ts: Date.now() }),
  history_list: (a: any) =>
    [
      ...historyAdded,
      { id: "h1", connId: "dev", sql: "SELECT client_ip, count(*) FROM logs.user_activity_log GROUP BY 1 ORDER BY 2 DESC LIMIT 20", ts: Date.now() - 60000 },
      { id: "h2", connId: "dev", sql: "select * from users where id = 42", ts: Date.now() - 3600000 },
    ].filter((h) => !a?.connId || h.connId === a.connId),
  history_delete: () => null,
  fake_preview: () => ({
    plan: [
      { name: "id", dataType: "bigint", how: "left to the database (auto / generated)" },
      { name: "user_id", dataType: "integer", how: "picked from users (id)" },
      { name: "status", dataType: "order_status", how: "one of: new, paid, shipped" },
      { name: "price", dataType: "numeric(8,2)", how: "random values" },
      { name: "email", dataType: "character varying(60)", how: "unique values" },
      { name: "created_at", dataType: "timestamp with time zone", how: "random values" },
      { name: "note", dataType: "text", how: "random values · some NULL" },
    ],
    result: {
      columns: [{ name: "user_id", typeName: "integer" }, { name: "status", typeName: "order_status" }, { name: "price", typeName: "numeric(8,2)" }, { name: "email", typeName: "varchar(60)" }, { name: "created_at", typeName: "timestamptz" }, { name: "note", typeName: "text" }],
      rows: Array.from({ length: 8 }, (_, i) => [i + 3, ["new", "paid", "shipped"][i % 3], (i * 123.45 + 9).toFixed(2), `minjun.kim${i + 1}@example.com`, `2026-0${(i % 9) + 1}-1${i} 10:2${i}:00`, i % 4 ? "Quick sample data report." : null]),
      rowsAffected: 0, truncated: false, elapsedMs: 0, hasResultSet: true,
    },
    warnings: ["tags (text[]) will be NULL — this type isn't generated"],
  }),
  table_def: (a: any) => ({
    schema: a.schema, name: a.table, primaryKey: ["id"], primaryKeyName: `${a.table}_pkey`, comment: "Customers",
    columns: [
      { name: "id", orig: "id", dataType: "bigint", nullable: false, default: null, autoIncrement: true, comment: null },
      { name: "email", orig: "email", dataType: "character varying(100)", nullable: false, default: null, autoIncrement: false, comment: "login" },
      { name: "status", orig: "status", dataType: "text", nullable: true, default: "'active'::text", autoIncrement: false, comment: null },
      { name: "org_id", orig: "org_id", dataType: "integer", nullable: true, default: null, autoIncrement: false, comment: null },
    ],
    indexes: [{ name: `${a.table}_email_key`, orig: `${a.table}_email_key`, columns: ["email"], unique: true, constraint: true, definition: null }, { name: `${a.table}_lower_idx`, orig: `${a.table}_lower_idx`, columns: [], unique: false, definition: `CREATE INDEX ${a.table}_lower_idx ON public.${a.table} USING btree (lower(email))` }],
    foreignKeys: [{ name: `${a.table}_org_id_fkey`, orig: `${a.table}_org_id_fkey`, columns: ["org_id"], refSchema: "public", refTable: "orgs", refColumns: ["id"], onDelete: "CASCADE", onUpdate: "NO ACTION" }],
  }),
  table_ddl: (a: any) => {
    const t = a.after;
    if (!t.name) throw "Give the table a name";
    if (!t.columns.length) throw "Add at least one column";
    if (!a.before) return [`CREATE TABLE ${t.schema}.${t.name} (\n${t.columns.map((c: any) => `    ${c.name} ${c.dataType}${c.nullable ? "" : " NOT NULL"}`).join(",\n")}${t.primaryKey.length ? `,\n    PRIMARY KEY (${t.primaryKey.join(", ")})` : ""}\n)`];
    const out: string[] = [];
    if (a.before.name !== t.name) out.push(`ALTER TABLE ${t.schema}.${a.before.name} RENAME TO ${t.name}`);
    for (const c of t.columns) if (!c.orig) out.push(`ALTER TABLE ${t.schema}.${t.name} ADD COLUMN ${c.name} ${c.dataType}${c.nullable ? "" : " NOT NULL"}`);
    for (const c of a.before.columns) if (!t.columns.some((x: any) => x.orig === c.name)) out.push(`ALTER TABLE ${t.schema}.${t.name} DROP COLUMN ${c.name}`);
    return out;
  },
  new_object_ddl: (a: any) => {
    const o = a.object;
    if (!o.name) throw `Give the ${o.kind} a name`;
    return [o.kind === "database" ? `CREATE DATABASE ${o.name}` : o.kind === "schema" ? `CREATE SCHEMA ${o.name}` : o.kind === "view" ? `CREATE ${o.materialized ? "MATERIALIZED " : ""}VIEW ${o.schema}.${o.name} AS\n${o.query}` : `CREATE SEQUENCE ${o.schema}.${o.name}`];
  },
  rename_ddl: (a: any) => `ALTER TABLE ${a.schema}.${a.old} RENAME TO ${a.new}`,
  // The order of crates/core ddl::drop_sql (no quoting, no IF EXISTS, no dependencies): one statement per kind on PostgreSQL,
  // one per object elsewhere
  drop_ddl: (a: any) => {
    const pg = conns.find((c) => c.id === a.id.split("::")[0])?.kind === "postgres";
    return ([["view", "VIEW"], ["matview", "MATERIALIZED VIEW"], ["table", "TABLE"], ["partitioned", "TABLE"], ["foreign", "FOREIGN TABLE"], ["sequence", "SEQUENCE"]] as const).flatMap(([k, kw]) => {
      const names = a.objects.filter((o: any) => o.kind === k).map((o: any) => `${o.schema}.${o.name}`);
      return (pg && names.length ? [names] : names.map((n: string) => [n])).map((ns: string[]) => `DROP ${kw} ${ns.join(", ")};`);
    });
  },
  // e2e sets __mockDdlWarning for "ran, but the Production log couldn't record it"
  run_ddl: () => new Promise((ok) => setTimeout(() => ok((globalThis as any).__mockDdlWarning ?? null), 300)),
  subset_preview: () => ({
    total: 9,
    tables: [
      { schema: "public", table: "users", rows: 2, masked: [{ column: "email", mask: "email" }, { column: "user_name", mask: "name" }, { column: "phone", mask: "phone" }], reason: "start" },
      { schema: "public", table: "products", rows: 2, masked: [], reason: "parent of order_items" },
      { schema: "public", table: "orders", rows: 2, masked: [{ column: "recipient_name", mask: "name" }, { column: "address", mask: "address" }], reason: "child of users" },
      { schema: "public", table: "order_items", rows: 3, masked: [], reason: "child of orders" },
    ],
  }),
  subset_copy: () => new Promise((ok) => setTimeout(() => ok({ total: 9, tables: [] }), 800)),
  fake_fill: (a: any) => new Promise((ok) => setTimeout(() => ok({ rows: a.rows }), 600)),
  audit_log: () => [
    { id: "a5", ts: Date.now() - 2000, connId: "prod", connName: "[PROD] shop_app", sql: "COMMIT outcome unknown (connection lost)", inTx: false, error: "error communicating with database: Connection reset by peer (os error 54)", note: "The server may have committed before the connection was lost. Check the data before you run these changes again." },
    { id: "a4", ts: Date.now() - 5000, connId: "prod", connName: "[PROD] shop_app", sql: "update orders set status = 'refunded' where id = 7", inTx: false, started: true,
      note: "Running, or stopped before its result was recorded (Qurry quit, or couldn't write the log after it ran)" },
    { id: "a6", ts: Date.now() - 20000, connId: "prod", connName: "[PROD] shop_app", sql: "ENDED (committed or rolled back)", inTx: false,
      note: "The transaction ended in the statement before this entry (a CALL, a deadlock or a lock wait timeout), and MySQL doesn't say whether it committed or rolled back the changes made before it. Check the data before you run them again." },
    { id: "a3", ts: Date.now() - 30000, connId: "prod", connName: "[PROD] shop_app", sql: "COMMIT", inTx: false },
    { id: "a2", ts: Date.now() - 60000, connId: "prod", connName: "[PROD] shop_app", sql: "update users set user_name = 'kim' where id <= 3", rows: 3, inTx: true,
      undo: "-- Undo for: update users set user_name = 'kim' where id <= 3\n-- Saved just before it ran. Rows changed again since then are overwritten.\nUPDATE users AS qurry_t SET user_name = qurry_old.user_name\nFROM jsonb_populate_recordset(NULL::users, $qurry$[{\"id\":1,\"user_name\":\"user_1\"}]$qurry$) AS qurry_old\nWHERE qurry_t.id = qurry_old.id;\n" },
    { id: "a1", ts: Date.now() - 3600000, connId: "prod", connName: "[PROD] shop_app", sql: "delete from logs.user_activity_log where ts < now() - interval '1 year'", rows: 12408, inTx: false, note: "More than 1000 rows change: no undo kept" },
    { id: "a0", ts: Date.now() - 7200000, connId: "prod", connName: "[PROD] shop_app", sql: "alter table users add column email text", inTx: false, error: "ERROR: column \"email\" already exists" },
  ],
  history_import: () => 0,
  snippets: () => [{ id: "s1", name: "Suspicious IPs (failed logins)", sql: "SELECT client_ip, count(*) FILTER (WHERE code NOT LIKE 'S%') AS failed FROM logs.user_activity_log GROUP BY 1 HAVING count(*) >= 3", connId: null }],
  snippet_save: (a: any) => ({ ...a.snippet, id: "s2" }),
  snippet_delete: () => null,
  analyze_plan: () => ({
    op: "Gather Merge", detail: "", startup: 1000, total: 6200.5, rows: 12, actualTime: 45.0, actualRows: 766, loops: 1, selfTime: 19.2, pct: 42.6,
    warnings: ["Rows estimate off ×64 (estimated 12, actual 766) — run ANALYZE; stale statistics lead to bad plans"], props: [["Workers Planned", "2"], ["Workers Launched", "2"]],
    children: [{ op: "Sort", detail: "key: at", startup: 990, total: 990.03, rows: 12, actualTime: 25.8, actualRows: 383, loops: 2, selfTime: 0.6, pct: 1.3, warnings: [], props: [["Sort Method", "quicksort"]],
      children: [{ op: "Seq Scan", detail: "on big_t  filter: (h ~~ 'ab%'::text)", startup: 0, total: 989.9, rows: 12, actualTime: 25.2, actualRows: 383, loops: 2, selfTime: 25.2, pct: 56.0,
        warnings: ["Seq Scan reads the whole table (~200,000 rows) — an index on the filter columns may help", "Filter removed 99% of rows (199,234 rows)"], props: [["Relation Name", "big_t"], ["Rows Removed by Filter", "99617"]], children: [] }] }],
  }),
  dba_explain: (a: any) => (/\$\d/.test(a.query) ? `-- Replace the parameters ($1, $2 …) with real values, then run:\nEXPLAIN ${a.query}` : `EXPLAIN ${a.query}`),
  dba_action_sql: (a: any) => a.template.replace(/\{=?(\w+)\}/g, (_: string, k: string) => a.row[k] ?? ""),
  pick_data_file: () => ["~/Downloads/new_users.csv", { format: "csv", columns: ["ID", "Name", "Email", "signup date"], rows: [["1", "Kim", "kim@example.com", "2026-01-02"], ["2", "Lee", null, "2026-01-03"]], bytes: 12_400_000 }],
  import_data: () => new Promise((ok) => setTimeout(() => ok(1234), 4000)),
  save_text_file: () => null,
  export_connections: () => null,
  // window.__mockImportNone: nothing found on this computer; __mockImportDelay: ms before the list comes back (a slow disk)
  import_locations: () => {
    const list = (window as any).__mockImportNone ? [] : structuredClone(IMPORT_LOCATIONS);
    const ms = (window as any).__mockImportDelay;
    return ms ? new Promise((ok) => setTimeout(() => ok(list), ms)) : list;
  },
  import_location: (a: any) => importPlan(IMPORT_LOCATIONS.find((l) => l.path === a.path)?.label ?? a.path, a.path),
  // window.__mockImportPick: what the native picker returns (null = canceled); "broken" = a file that can't be read
  pick_import: () => {
    const p = (window as any).__mockImportPick;
    if (p === null) return null;
    if (p === "broken") throw "Can't read ~/work/shop-api/.idea/dataSources.xml:\nline 42, column 7: the element <data-source> isn't closed\n\nNothing was imported.";
    return importPlan("DBeaver · General", "~/Library/DBeaverData/workspace6/General/.dbeaver");
  },
  import_picked: (a: any) => {
    const n = (a.picked as boolean[]).filter(Boolean).length;
    (window as any).__mockImported = a.picked;
    return { added: n, updated: 0, message: `Imported ${n} data source${n === 1 ? "" : "s"} — passwords are asked on the first connect` };
  },
  save_keymap: () => null,
  install_update: () => new Promise((_, rej) => setTimeout(() => rej("Update failed: mock"), 800)),
  update_channel: () => updateChannel,
  // Fonts that ship with macOS (the specs run as on macOS), one of them proportional
  font_families: () => [
    { name: "Andale Mono", mono: true },
    { name: "Courier New", mono: true },
    { name: "Helvetica", mono: false },
    { name: "Menlo", mono: true },
    { name: "Monaco", mono: true },
    { name: "PT Mono", mono: true },
  ],
  custom_fonts: () => structuredClone(customFonts),
  add_font_file: () => {
    // window.__mockFontAddFails: the picked file isn't a font
    if ((window as any).__mockFontAddFails) throw "notes.ttf isn't a font file (TrueType, OpenType, WOFF or WOFF2)";
    let name = "Fira Code";
    for (let n = 2; customFonts.some((f) => f.name === name); n++) name = `Fira Code ${n}`;
    const f = { name, file: `${name}.ttf` };
    customFonts = [...customFonts, f].sort((a, b) => a.name.toLowerCase().localeCompare(b.name.toLowerCase()));
    return f;
  },
  remove_font_file: (a: any) => {
    if (!customFonts.some((f) => f.file === a.file)) throw `No added font file named ${a.file}`;
    customFonts = customFonts.filter((f) => f.file !== a.file);
  },
  // window.__mockFontBroken: the file passes the magic-byte check but isn't a loadable font (truncated)
  font_file: () => ((window as any).__mockFontBroken ? new Uint8Array([0, 1, 0, 0]).buffer : fetch(fontBytesUrl).then((r) => r.arrayBuffer())),
  theme_settings: () => themeSettings(),
  // As the core: the id must exist (or be "system"). window.__mockChannelSaveFails: the file can't be written
  set_theme: (a: any) => {
    if ((window as any).__mockChannelSaveFails) throw "Couldn't save the settings: permission denied";
    const s = themeSettings();
    if (a.id !== "system" && !THEMES.themes.some((t) => t.id === a.id) && !s.custom.some((c: any) => c.id === a.id)) throw `There is no theme "${a.id}"`;
    saveThemeSettings({ ...s, theme: a.id });
  },
  set_theme_sync: (a: any) => {
    if ((window as any).__mockChannelSaveFails) throw "Couldn't save the settings: permission denied";
    saveThemeSettings({ ...themeSettings(), sync: a.sync });
  },
  save_custom_theme: (a: any) => {
    checkTheme(a.theme);
    const s = themeSettings();
    const t = { ...a.theme, name: a.theme.name.trim() };
    if (!t.id) for (let n = 1; !t.id; n++) if (!s.custom.some((c: any) => c.id === `custom-${n}`)) t.id = `custom-${n}`;
    const custom = s.custom.some((c: any) => c.id === t.id) ? s.custom.map((c: any) => (c.id === t.id ? t : c)) : [...s.custom, t];
    saveThemeSettings({ ...s, custom });
    return t;
  },
  // As the core: where the deleted theme is picked, its base takes its place
  delete_custom_theme: (a: any) => {
    const s = themeSettings();
    const gone = s.custom.find((c: any) => c.id === a.id);
    if (!gone) return;
    const side = (v: string) => (v === a.id ? gone.base : v);
    saveThemeSettings({ theme: s.theme === a.id ? gone.base : s.theme, sync: { dark: side(s.sync.dark), light: side(s.sync.light) }, custom: s.custom.filter((c: any) => c.id !== a.id) });
  },
  // window.__mockThemeFile: the text of the file picked (unset: cancelled); refused as the core refuses it
  import_theme: () => {
    const text = (window as any).__mockThemeFile;
    return text == null ? null : shared().parseThemeFile(text);
  },
  // The file's text goes to window.__mockThemeExported
  export_theme: (a: any) => {
    checkTheme(a.theme);
    (window as any).__mockThemeExported = shared().themeFileText(a.theme);
    return `/Users/me/Downloads/${a.theme.name}.qurry-theme.json`;
  },
  keyword_case: () => keywordCase,
  set_keyword_case: (a: any) => {
    if ((window as any).__mockChannelSaveFails) throw "Couldn't save the settings: permission denied";
    keywordCase = a.case;
    localStorage.setItem("mock-keywordCase", a.case);
  },
  grid_align: () => gridAlign,
  set_grid_align: (a: any) => {
    if ((window as any).__mockChannelSaveFails) throw "Couldn't save the settings: permission denied";
    // As the core: the default (numbers on the right, NULL where its column's type sits) drops the override
    const next: Record<string, string> = { ...gridAlign };
    if (a.align === (a.class === "number" ? "right" : a.class === "null" ? "column" : "left")) delete next[a.class];
    else next[a.class] = a.align;
    gridAlign = next;
    localStorage.setItem("mock-gridAlign", JSON.stringify(next));
    return next;
  },
  column_widths: (a: any) => columnWidths()[widthsKey(a.table)] ?? {},
  // As the core: merged into what is saved, kept between 3 and 200 characters. window.__mockChannelSaveFails: the file can't be written
  set_column_widths: (a: any) => {
    if ((window as any).__mockChannelSaveFails) throw "Couldn't save the settings: permission denied";
    const [all, k] = [columnWidths(), widthsKey(a.table)];
    // 0 forgets a column's width
    all[k] = { ...all[k], ...Object.fromEntries(Object.entries(a.widths as Record<string, number>).map(([c, w]) => [c, Math.min(Math.max(Math.round(w), 3), 200)])) };
    for (const [c, w] of Object.entries(a.widths as Record<string, number>)) if (w === 0) delete all[k][c];
    saveWidths(all);
    return all[k];
  },
  reset_column_widths: (a: any) => {
    if ((window as any).__mockChannelSaveFails) throw "Couldn't save the settings: permission denied";
    const all = columnWidths();
    delete all[widthsKey(a.table)];
    saveWidths(all);
  },
  // window.__mockChannelSaveFails: the settings file can't be written
  set_update_channel: (a: any) => {
    if ((window as any).__mockChannelSaveFails) throw "Couldn't save the settings: permission denied";
    updateChannel = a.channel;
    return updateChecked ? updateInfo() : null;
  },
  // window.__mockChecksOff: QURRY_NO_UPDATE_CHECK is set, so only a forced check (Check Now) asks
  check_update: (a: any) => {
    if ((window as any).__mockChecksOff && !a.force) throw "Update checks are off";
    return (updateChecked = true), updateInfo();
  },
};
/** window.__mockNoUpdate: the channel offers nothing newer (e.g. the TUI switched it back to stable) */
function updateInfo() {
  return {
    current: "0.1.0",
    channel: updateChannel,
    latest: (window as any).__mockNoUpdate ? null : { version: "0.1.2", url: "https://github.com/Hkhyyy/qurry/releases/tag/v0.1.2", notes: "## dbtool 0.1.2\n- something new", assets: [{ name: "dbtool_0.1.2_aarch64.dmg", url: "https://example.com/dbtool_0.1.2_aarch64.dmg" }] },
    changes: [
      { version: "0.1.2", changes: ["[feat] TUI 키 안내: ⌃P 시작, F1 Start here, 상태줄 (#18)", "[fix] ; 없는 문장 나누기 (#17)"] },
      { version: "0.1.1", changes: ["[fix] 분할 탭 바 구분선, GUI 탭 사이 간격 (#16)"] },
    ],
  };
}
(window as any).__TAURI_INTERNALS__ = {
  invoke: async (cmd: string, args: unknown) => {
    const h = handlers[cmd];
    if (!h) throw `mock: ${cmd} not implemented`;
    // Mimics a long-running query: pg_sleep(n)
    const sleep = cmd === "run_query" && /pg_sleep\((\d+(?:\.\d+)?)\)/.exec((args as any)?.sql ?? "");
    if (sleep) await new Promise((r) => setTimeout(r, Number(sleep[1]) * 1000));
    return h(args);
  },
  transformCallback: () => 0,
};
export {};
