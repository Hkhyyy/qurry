/** For development: opening in a browser with ?mock uses fake data instead of the Tauri backend (for checking screens). */
const conns = [
  { id: "dev", name: "[DEV] shop_app", kind: "postgres", host: "dev-db.example.com", port: 5432, user: "app", database: "shop_app", color: "#5f5b24", hasPassword: true, connected: true },
  { id: "prod", name: "[PROD] shop_app", kind: "postgres", host: "prod-db", port: 5432, user: "app", database: "shop_app", color: "#6e2a2a", hasPassword: true, connected: false,
    ssh: { enabled: true, host: "bastion.example.com", port: 22, user: "ec2-user", auth: "key", keyPath: "~/.ssh/id_ed25519" } },
  { id: "my", name: "[MySQL] shop", kind: "mysql", host: "127.0.0.1", port: 13306, user: "root", database: "shop", color: "#22524f", hasPassword: true, connected: true },
  { id: "rd", name: "[Redis] cache", kind: "redis", host: "127.0.0.1", port: 16379, user: "", database: "0", color: "#6e2a2a", hasPassword: false, connected: true },
  { id: "local", name: "[LOCAL] TestServer", kind: "postgres", host: "localhost", port: 5432, user: "dev", database: "", color: null, hasPassword: false, connected: false },
];
const result = {
  columns: [
    { name: "id", typeName: "INT8" },
    { name: "user_name", typeName: "VARCHAR" },
    { name: "amount", typeName: "NUMERIC" },
    { name: "created_at", typeName: "TIMESTAMPTZ" },
    { name: "memo", typeName: "TEXT" },
  ],
  rows: Array.from({ length: 500 }, (_, i) => [i + 1, `user_${i + 1}`, (i * 1234.5).toFixed(2), "2026-10-15 12:00:00.000 +09:00", i % 3 ? null : "note"]),
  rowsAffected: 0,
  truncated: true,
  elapsedMs: 12,
  hasResultSet: true,
};
const handlers: Record<string, (a: any) => unknown> = {
  list_connections: () => conns,
  list_databases: (a: any) => (a.id === "rd" ? ["db0", "db3"] : a.id === "my" ? ["information_schema", "mysql", "shop", "sys"] : ["postgres", "shop_app", "shop_app_test"]),
  redis_scan: (a: any) => ({
    cursor: "0",
    keys: [
      { key: "session:abc", type: "string", ttl: 3_540_000 },
      { key: "user:1", type: "hash", ttl: -1 },
      { key: "user:1:name", type: "string", ttl: -1 },
      { key: "queue:mail", type: "list", ttl: -1 },
      { key: "tags", type: "set", ttl: -1 },
      { key: "rank", type: "zset", ttl: 86_400_000 },
      { key: "events", type: "stream", ttl: -1 },
    ].filter((k) => !a.typeFilter || k.type === a.typeFilter),
  }),
  redis_key: (a: any) =>
    a.key === "user:1"
      ? { type: "hash", ttl: -1, length: 3, memory: 96, encoding: "listpack", truncated: false, value: [["age", "30"], ["email", "a@b.c"], ["profile", '{"city":"Seoul","tags":["vip","beta"]}']] }
      : a.key === "rank"
      ? { type: "zset", ttl: 86400000, length: 2, memory: 72, encoding: "listpack", truncated: false, value: [["alice", "10"], ["bob", "20"]] }
      : { type: "string", ttl: 3540000, length: 42, memory: 80, encoding: "embstr", truncated: false, value: '{"uid":1,"roles":["admin"],"exp":1760000000}' },
  redis_exec: () => ({ status: "OK" }),
  redis_exec_line: (a: any) => (/^hgetall/i.test(a.line) ? ["age", "30", "email", "a@b.c"] : /^incr/i.test(a.line) ? 7 : /^get/i.test(a.line) ? null : { status: "OK" }),
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
  list_objects: (a: any) => String(a.id).startsWith("rd") ? { relations: [["events", "stream"], ["rank", "zset"], ["session:abc", "string"], ["tags", "set"], ["user:1", "hash"]].map(([name, t]) => ({ name, kind: "key", comment: t, rowEstimate: null })), routines: [] } : ({
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
  completion_meta: () => [
    { schema: "public", name: "users", columns: [["id", "bigint"], ["user_name", "varchar"]] },
    { schema: "public", name: "orders", columns: [["id", "bigint"], ["status", "varchar"], ["amount", "numeric"], ["data", "jsonb"], ["reasons", "jsonb"]] },
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
  run_query: (a: any) =>
    /^EXPLAIN/i.test(a.sql)
      ? { ...result, columns: [{ name: "QUERY PLAN", typeName: "JSON" }], rows: [[JSON.stringify([{ Plan: { "Node Type": "Hash Join", "Join Type": "Inner", "Startup Cost": 1.5, "Total Cost": 45.2, "Plan Rows": 120, "Plan Width": 64, "Hash Cond": "(u.contract_id = c.id)", Plans: [{ "Node Type": "Seq Scan", "Relation Name": "users", Schema: "public", Alias: "u", "Startup Cost": 0, "Total Cost": 22.1, "Plan Rows": 1200, "Plan Width": 40 }, { "Node Type": "Hash", "Startup Cost": 1.2, "Total Cost": 1.2, "Plan Rows": 12, "Plan Width": 24, Plans: [{ "Node Type": "Index Scan", "Relation Name": "contracts", "Index Name": "contracts_pkey", "Startup Cost": 0.1, "Total Cost": 1.2, "Plan Rows": 12, "Plan Width": 24 }] }] } }])]], truncated: false }
      : /^\s*(insert|update|delete)/i.test(a.sql)
        ? { ...result, columns: [], rows: [], rowsAffected: 3, truncated: false, hasResultSet: false, inTransaction: (globalThis as any).__mockManual === true }
        : { ...result, inTransaction: (globalThis as any).__mockManual === true },
  set_tx_mode: (a: any) => { (globalThis as any).__mockManual = a.manual; return null; },
  commit_tx: () => null,
  rollback_tx: () => null,
  rag_indexes: () => (globalThis as any).__rag ?? ((globalThis as any).__rag = [{ id: "r1", name: "tickets search", source: "dev", schema: "public", table: "users", idColumn: "id", textColumns: ["user_name"], provider: "ollama", model: "nomic-embed-text", baseUrl: "", dest: "dev", collection: "users_embeddings", dim: 768, rows: 3000, builtAt: "2026-10-27 14:50" }]),
  rag_save: (a: any) => { const x = { ...a.index, id: a.index.id || "r" + Date.now() }; const l = (globalThis as any).__rag ?? []; (globalThis as any).__rag = [...l.filter((y: any) => y.id !== x.id), x]; return x; },
  rag_delete: (a: any) => { (globalThis as any).__rag = ((globalThis as any).__rag ?? []).filter((y: any) => y.id !== a.id); return null; },
  rag_has_key: () => false,
  rag_set_key: () => null,
  rag_build: () => 3000,
  rag_search: () => ({ ...result, columns: [{ name: "score", typeName: "FLOAT8" }, ...result.columns], rows: result.rows.slice(0, 5).map((r, i) => [0.91 - i * 0.07, ...r]), truncated: false }),
  similar_rows_sql: (a: any) => `SELECT ${a.column} <-> (SELECT ${a.column} FROM ${a.schema}.${a.table} WHERE ${a.pk.map((p: any) => `${p[0]} = '${p[1]}'`).join(" AND ")}) AS distance, *\nFROM ${a.schema}.${a.table}\nORDER BY distance\nLIMIT 20`,
  object_ddl: (a: any) => `CREATE TABLE ${a.schema}.${a.name}\n(\n    id bigint DEFAULT nextval('users_id_seq'::regclass) NOT NULL,\n    user_name character varying(50) NOT NULL,\n    CONSTRAINT users_pkey PRIMARY KEY (id)\n);\n`,
  test_connection: () => "PostgreSQL 16.11  (via SSH ec2-user@bastion.example.com)",
  save_connection: (a: any) => {
    const i = conns.findIndex((c: any) => c.id === a.config.id);
    if (i >= 0) (conns as any)[i] = { ...conns[i], ...a.config };
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
  fetch_more: () => ({ ...result, rows: result.rows.slice(0, 120).map((r) => [Number(r[0]) + 500, ...r.slice(1)]), truncated: false }),
  cancel_query: () => true,
  close_session: () => null,
  execute_batch: (a: any) => a.statements.length,
  load_keymap: () => ({}),
  load_workspace: () => JSON.parse(localStorage.getItem("mock-ws") ?? "null"),
  save_workspace: (a: any) => (localStorage.setItem("mock-ws", JSON.stringify(a.ws)), null),
  tx_state: () => (globalThis as any).__mockManual === true,
  unsafe_reason: (a: any) => (/^\s*(delete|update)\b(?![\s\S]*\bwhere\b)/i.test(a.sql) ? "DELETE without WHERE changes every row" : /^\s*(drop|truncate)\b/i.test(a.sql) ? "DROP removes data" : null),
  trust_ssh_host: () => "Trusted",
  export_data: (a: any) => new Promise((ok) => setTimeout(() => ok({ rows: 500, path: a.clipboard ? null : `~/Downloads/${a.defaultName}.${a.format}`, text: a.clipboard ? "id\tname\n1\ta\n" : null }), 4000)),
  cancel_job: () => null,
  dba_reports: () => [
    { id: "sessions", title: "Sessions", group: "Activity", help: "Connections and what they run. blocking = how many sessions wait for this one.", actions: [{ kind: "kill", label: "Cancel Query", terminate: false }, { kind: "kill", label: "Terminate Session", terminate: true }, { kind: "sql", label: "Open Query in Console", template: "{=query}" }] },
    { id: "locks", title: "Lock Waits", group: "Activity", help: "Who waits for whom.", actions: [] },
    { id: "slow_queries", title: "Slow Queries (Top 100)", group: "Performance", help: "By total time.", actions: [] },
    { id: "table_sizes", title: "Table Sizes", group: "Storage", help: "Biggest first.", actions: [] },
  ],
  dba_report: (a: any) =>
    a.report === "slow_queries"
      ? Promise.reject("pg_stat_statements is not enabled. Add it to shared_preload_libraries (restart), then run: CREATE EXTENSION pg_stat_statements;")
      : {
          columns: ["pid", "user", "db", "state", "blocking", "blocked_by", "query_sec", "query"].map((name) => ({ name, typeName: name === "pid" || name === "blocking" ? "INT4" : "TEXT" })),
          rows: [
            [4123, "app", "shop_app", "idle in transaction", 2, "", "184.2", "UPDATE orders SET status = 'paid' WHERE id = 42"],
            [4188, "app", "shop_app", "active", 0, "4123", "61.0", "UPDATE orders SET status = 'shipped' WHERE id = 42"],
            [4190, "batch", "shop_app", "active", 0, "4123", "12.4", "DELETE FROM orders WHERE id = 42"],
            [4201, "dev", "shop_app", "idle", 0, "", null, "SELECT 1"],
          ],
          rowsAffected: 0, truncated: false, elapsedMs: 3, hasResultSet: true,
        },
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
  history_add: () => null,
  history_list: () => [
    { id: "h1", connId: "dev", sql: "SELECT client_ip, count(*) FROM logs.user_activity_log GROUP BY 1 ORDER BY 2 DESC LIMIT 20", ts: Date.now() - 60000 },
    { id: "h2", connId: "dev", sql: "select * from users where id = 42", ts: Date.now() - 3600000 },
  ],
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
  run_ddl: () => new Promise((ok) => setTimeout(() => ok(null), 300)),
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
  fake_fill: (a: any) => new Promise((ok) => setTimeout(() => ok(a.rows), 600)),
  audit_log: () => [
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
  dba_action_sql: (a: any) => a.template.replace(/\{=?(\w+)\}/g, (_: string, k: string) => a.row[k] ?? ""),
  pick_data_file: () => ["~/Downloads/new_users.csv", { format: "csv", columns: ["ID", "Name", "Email", "signup date"], rows: [["1", "Kim", "kim@example.com", "2026-01-02"], ["2", "Lee", null, "2026-01-03"]], bytes: 12_400_000 }],
  import_data: () => new Promise((ok) => setTimeout(() => ok(1234), 4000)),
  save_text_file: () => null,
  export_connections: () => null,
  pick_import_file: () => ["~ update \"[PROD] shop_app\": turns Read-only OFF"],
  import_picked: () => ({ added: 0, updated: 1 }),
  save_keymap: () => null,
  install_update: () => new Promise((_, rej) => setTimeout(() => rej("Update failed: mock"), 800)),
  check_update: () => ({
    current: "0.1.0",
    latest: { version: "0.1.2", url: "https://github.com/Hkhyyy/qurry/releases/tag/v0.1.2", notes: "## dbtool 0.1.2\n- something new", assets: [{ name: "dbtool_0.1.2_aarch64.dmg", url: "https://example.com/dbtool_0.1.2_aarch64.dmg" }] },
    changes: [
      { version: "0.1.2", changes: ["[feat] TUI 키 안내: ⌃P 시작, F1 Start here, 상태줄 (#18)", "[fix] ; 없는 문장 나누기 (#17)"] },
      { version: "0.1.1", changes: ["[fix] 분할 탭 바 구분선, GUI 탭 사이 간격 (#16)"] },
    ],
  }),
};
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
