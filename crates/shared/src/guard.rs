//! Statement checks: read-only (MCP · Read-only Data Source) and risky-statement confirmation (UPDATE/DELETE without WHERE, DROP, TRUNCATE).
//! Reads words while skipping strings and comments with the same rules as the server (same rules as sql::split_statements).
//! The last line of defense is the server's READ ONLY transaction; this blocks what leaks past it (multiple statements, file writes, admin functions)
//!
//! The GUI also gets these checks as WASM (qurry-shared-wasm), for UI decisions only (badges, confirm dialogs, which statement
//! is a read). Enforcement never trusts that copy: read-only Data Sources and MCP run these checks in the backend on every statement.
use crate::kind::DbKind;
use regex::Regex;
use std::sync::LazyLock;

/// db.collection.method(args) or db.getCollection("name").method(args), read as the MongoDB driver reads it
static MONGO_CALL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?s)^db\.(?:getCollection\(\s*["']([^"']+)["']\s*\)|([A-Za-z_][\w.-]*?))\.(\w+)\((.*)\)$"#).unwrap());

/// Words (lowercased) and symbols outside strings and comments. depth = parenthesis depth
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Tok {
    pub(crate) text: String,
    pub(crate) word: bool,
    pub(crate) depth: i32,
    /// Name wrapped in "…" / `…` (not a keyword)
    pub(crate) quoted: bool,
    /// Position in the original text (bytes)
    pub(crate) start: usize,
    pub(crate) end: usize,
}

pub(crate) struct Scan {
    pub(crate) toks: Vec<Tok>,
    /// Whether there was a MySQL /*! … */ · MariaDB /*M! … */ (executable comment)
    pub(crate) exec_comment: bool,
}

pub(crate) fn scan(sql: &str, kind: DbKind) -> Scan {
    scan_as(sql, kind, Quotes::Default)
}

/// How a server setting can make the server read quotes once there is a backslash
#[derive(Clone, Copy)]
enum Quotes {
    /// The kind's default (crate::sql::backslash_escapes)
    Default,
    /// The other backslash rule: MySQL NO_BACKSLASH_ESCAPES (\ is a plain char), PostgreSQL standard_conforming_strings = off (\ escapes in '…' too)
    OtherBackslash,
    /// MySQL ANSI_QUOTES: "…" is a name where \ is a plain char, while \ still escapes in '…'
    Ansi,
}

fn scan_as(sql: &str, kind: DbKind, quotes: Quotes) -> Scan {
    let b = sql.as_bytes();
    let n = b.len();
    let (mut toks, mut i, mut depth, mut exec_comment) = (Vec::new(), 0usize, 0i32, false);
    let my = kind == DbKind::Mysql;
    let backslash = crate::sql::backslash_escapes(kind);
    let escapes = |q: u8| match (quotes, kind) {
        (Quotes::OtherBackslash, DbKind::Mysql) => false,
        (Quotes::OtherBackslash, DbKind::Postgres) | (Quotes::Ansi, DbKind::Mysql) => q == b'\'',
        _ => backslash && crate::sql::escapes_in(kind, q),
    };
    while i < n {
        let c = b[i];
        let d = if i + 1 < n { b[i + 1] } else { 0 };
        if c.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        if c == b'-' && d == b'-' && (!my || i + 2 >= n || b[i + 2].is_ascii_whitespace() || b[i + 2].is_ascii_control()) {
            i = sql[i..].find('\n').map(|j| i + j + 1).unwrap_or(n);
            continue;
        }
        // MySQL # comment, ClickHouse "# " · "#!" comment
        if c == b'#' && crate::sql::hash_comment(kind, d) {
            i = sql[i..].find('\n').map(|j| i + j + 1).unwrap_or(n);
            continue;
        }
        if c == b'/' && d == b'*' {
            // The server executes MySQL /*! … */ · MariaDB /*M! … */
            let bang = if my && i + 2 < n && b[i + 2] == b'!' { 3 } else if my && i + 3 < n && b[i + 2] == b'M' && b[i + 3] == b'!' { 4 } else { 0 };
            if bang > 0 {
                exec_comment = true;
                i += bang;
                while i < n && b[i].is_ascii_digit() {
                    i += 1;
                }
                continue;
            }
            let (mut dep, mut j) = (1, i + 2);
            while j < n && dep > 0 {
                if b[j] == b'/' && j + 1 < n && b[j + 1] == b'*' && kind == DbKind::Postgres {
                    dep += 1;
                    j += 2;
                } else if b[j] == b'*' && j + 1 < n && b[j + 1] == b'/' {
                    dep -= 1;
                    j += 2;
                } else {
                    j += 1;
                }
            }
            i = j;
            continue;
        }
        // String / quoted identifier: one whole token (identifiers can be used as names, so they stay as words)
        if c == b'\'' || c == b'"' || c == b'`' {
            // In PG E'…', \ is an escape. Not an e at the end of a word like name' (same rules as sql::split_statements)
            let e_string = crate::sql::e_strings(kind) && c == b'\'' && crate::sql::is_e_string(b, i);
            let mut j = i + 1;
            while j < n {
                // \ inside a MySQL backtick name is not an escape
                if (escapes(c) || e_string) && b[j] == b'\\' {
                    j += 2;
                    continue;
                }
                if b[j] == c {
                    if j + 1 < n && b[j + 1] == c {
                        j += 2;
                        continue;
                    }
                    break;
                }
                j += 1;
            }
            let inner = &sql[i + 1..j.min(n)];
            let (start, end) = (i, (j + 1).min(n));
            toks.push(if c == b'\'' { Tok { text: "'".into(), word: false, depth, quoted: false, start, end } } else { Tok { text: inner.to_lowercase(), word: true, depth, quoted: true, start, end } });
            i = (j + 1).min(n);
            continue;
        }
        // SQL Server [name] (]] is a single ])
        if c == b'[' && crate::sql::bracket_names(kind) {
            let mut j = i + 1;
            while j < n && !(b[j] == b']' && !(j + 1 < n && b[j + 1] == b']')) {
                j += if b[j] == b']' { 2 } else { 1 };
            }
            toks.push(Tok { text: sql[i + 1..j.min(n)].to_lowercase(), word: true, depth, quoted: true, start: i, end: (j + 1).min(n) });
            i = (j + 1).min(n);
            continue;
        }
        if c == b'$' && crate::sql::dollar_quotes(kind) && crate::sql::dollar_opens(b, i) {
            let rest = &sql[i..];
            if let Some(te) = rest[1..].find('$').map(|j| j + 2) {
                let tag = &rest[..te];
                if crate::sql::is_dollar_tag(tag) {
                    let start = i;
                    i = sql[i + te..].find(tag).map(|j| i + te + j + te).unwrap_or(n);
                    toks.push(Tok { text: "'".into(), word: false, depth, quoted: false, start, end: i });
                    continue;
                }
            }
        }
        if c.is_ascii_alphanumeric() || c == b'_' || c >= 0x80 {
            let start = i;
            while i < n && (b[i].is_ascii_alphanumeric() || b[i] == b'_' || b[i] == b'$' || b[i] >= 0x80) {
                i += 1;
            }
            toks.push(Tok { text: sql[start..i].to_lowercase(), word: true, depth, quoted: false, start, end: i });
            continue;
        }
        if c == b'(' {
            depth += 1;
        }
        toks.push(Tok { text: (c as char).to_string(), word: false, depth, quoted: false, start: i, end: i + 1 });
        if c == b')' {
            depth -= 1;
        }
        i += 1;
    }
    Scan { toks, exec_comment }
}

/// Shape of a single simple UPDATE / DELETE statement (PROD safeguard: counting affected rows · undo SQL).
/// Only statements that change one table by a condition (None if there is a CTE, UPDATE … FROM, DELETE … USING, multiple tables, ORDER BY / LIMIT)
#[derive(Debug, Clone, PartialEq)]
pub struct SimpleDml {
    pub delete: bool,
    /// Table name (as written, without alias): public."Order Items"
    pub table: String,
    /// Text for FROM (with alias): public."Order Items" o
    pub from: String,
    /// WHERE condition (all rows if absent)
    pub cond: Option<String>,
    /// Columns the UPDATE changes (quotes stripped, unquoted PostgreSQL names lowercased)
    pub set_cols: Vec<String>,
}

pub fn simple_dml(sql: &str, kind: DbKind) -> Option<SimpleDml> {
    if kind == DbKind::Redis {
        return None;
    }
    let sc = scan(sql, kind);
    if sc.exec_comment {
        return None;
    }
    // First statement only (up to the top-level ;)
    let end = sc.toks.iter().position(|x| x.text == ";" && x.depth == 0).unwrap_or(sc.toks.len());
    let t = &sc.toks[..end];
    let first = t.iter().position(|x| x.word)?;
    let is = |i: usize, w: &str| t[i].word && !t[i].quoted && t[i].depth == 0 && t[i].text == w;
    let find = |w: &str, from: usize| (from..t.len()).find(|&i| is(i, w));
    let verb = t[first].text.clone();
    let has = |w: &str| find(w, first + 1).is_some();
    // Shapes that can't be counted: joined with other tables or changing only part
    if has("using") || has("join") || has("order") || has("limit") {
        return None;
    }
    let where_i = find("where", first + 1);
    let (a, b, set) = match verb.as_str() {
        "delete" => {
            let from = find("from", first + 1)?;
            if from != first + 1 {
                return None;
            }
            (from + 1, where_i.or(find("returning", from)).unwrap_or(t.len()), None)
        }
        "update" => {
            let set = find("set", first + 1)?;
            // UPDATE … FROM (PG) · multiple tables (MySQL a, b)
            if find("from", set).is_some_and(|f| where_i.is_none_or(|w| f < w)) || (first + 1..set).any(|i| t[i].text == "," && t[i].depth == 0) {
                return None;
            }
            (first + 1, set, Some(set))
        }
        _ => return None,
    };
    if a >= b || !t[a].word {
        return None;
    }
    // Name: a.b.c (up to the alias)
    let mut name_end = a;
    while name_end + 2 < b && t[name_end + 1].text == "." && t[name_end + 2].word {
        name_end += 2;
    }
    let table = sql[t[a].start..t[name_end].end].to_string();
    let from = sql[t[a].start..t[b - 1].end].to_string();
    // Condition: from after WHERE to before RETURNING (trailing comments excluded)
    let cond = match where_i {
        Some(w) => {
            let stop = find("returning", w).unwrap_or(t.len());
            if w + 1 >= stop {
                return None;
            }
            Some(sql[t[w + 1].start..t[stop - 1].end].to_string())
        }
        None => None,
    };
    // SET a = 1, o.b = 2, (c, d) = (…): the names before = (last part if dotted)
    let mut set_cols = Vec::new();
    if let Some(set) = set {
        let stop = where_i.or(find("returning", set)).unwrap_or(t.len());
        let name = |x: &Tok| {
            let raw = &sql[x.start..x.end];
            match raw.chars().next() {
                Some(q @ ('"' | '`')) => raw[1..raw.len() - 1].replace(&format!("{q}{q}"), &q.to_string()),
                _ if kind == DbKind::Postgres => raw.to_lowercase(),
                _ => raw.to_string(),
            }
        };
        let mut i = set + 1;
        let mut left = true;
        let mut last: Option<&Tok> = None;
        while i < stop {
            let x = &t[i];
            if x.depth == 0 && x.text == "," {
                left = true;
            } else if left && x.depth == 0 && x.text == "=" {
                if let Some(l) = last.take() {
                    set_cols.push(name(l));
                }
                left = false;
            } else if left && x.word {
                last = Some(x);
            } else if left && x.text == "," && x.depth == 1
                && let Some(l) = last.take()
            {
                set_cols.push(name(l));
            }
            i += 1;
        }
    }
    Some(SimpleDml { delete: verb == "delete", table, from, cond, set_cols })
}

/// PROD safeguard: SELECT count(*) counting the rows an UPDATE / DELETE would change. None if the statement can't be counted
pub fn impact_count_sql(sql: &str, kind: DbKind) -> Option<String> {
    let d = simple_dml(sql, kind)?;
    Some(match d.cond {
        Some(c) => format!("SELECT count(*) FROM {} WHERE {c}", d.from),
        None => format!("SELECT count(*) FROM {}", d.from),
    })
}

/// First words allowed in read-only mode
const READ_WORDS: &[&str] = &["select", "with", "values", "table", "show", "explain", "describe", "desc"];
/// The console of a read-only Data Source also allows transaction control and session settings. SAVEPOINT · RELEASE change no
/// data, and Explain Analyse inside an open transaction wraps the statement in SAVEPOINT qurry_explain … ROLLBACK TO SAVEPOINT
const SESSION_WORDS: &[&str] = &["begin", "start", "commit", "rollback", "end", "abort", "set", "use", "savepoint", "release"];

/// Functions with side effects outside the server even in read-only mode (blocked when someone else writes the query, as with MCP)
const SIDE_EFFECT_FUNCS: &[&str] = &[
    // PostgreSQL: killing other sessions · reading/writing server files · reloading config · external connections · locks
    "pg_terminate_backend", "pg_cancel_backend", "pg_reload_conf", "pg_rotate_logfile", "pg_read_file", "pg_read_binary_file",
    "pg_ls_dir", "pg_stat_file", "pg_ls_logdir", "pg_ls_waldir", "pg_ls_tmpdir", "pg_ls_archive_statusdir", "pg_file_write",
    "lo_import", "lo_export", "dblink", "dblink_exec", "dblink_connect", "dblink_send_query", "set_config", "pg_switch_wal",
    "pg_create_restore_point", "pg_promote", "pg_advisory_lock", "pg_advisory_xact_lock", "pg_try_advisory_lock", "pg_sleep",
    "pg_sleep_for", "pg_sleep_until", "pg_stat_reset", "pg_stat_reset_shared", "pg_stat_reset_single_table_counters",
    "pg_stat_reset_single_function_counters", "pg_stat_reset_slru", "pg_stat_reset_replication_slot", "pg_stat_reset_subscription_stats",
    "pg_drop_replication_slot", "pg_create_logical_replication_slot", "pg_create_physical_replication_slot", "pg_logical_emit_message",
    "pg_replication_origin_create", "pg_replication_origin_drop", "pg_log_backend_memory_contexts", "pg_replication_slot_advance",
    "dblink_connect_u", "setval",
    // sends a message to other sessions' LISTEN on commit
    "pg_notify",
    // run SQL given as a string (the name checks can't see inside it)
    "query_to_xml", "query_to_xmlschema", "query_to_xml_and_xmlschema", "cursor_to_xml", "cursor_to_xmlschema", "ts_stat",
    // MySQL: reading server files · delays · locks
    "load_file", "sleep", "benchmark", "get_lock", "release_lock", "release_all_locks",
];

/// Functions that change data although the statement reads like a SELECT: sequences and large objects (is_read only: an INSERT
/// over MCP execute may call nextval, and a read-only transaction refuses them anyway)
const WRITE_FUNCS: &[&str] = &["nextval", "lo_create", "lo_creat", "lo_unlink", "lo_put", "lo_from_bytea", "lowrite", "lo_truncate", "lo_truncate64"];

/// SQL Server can chain statements without ; (SELECT 1 DROP TABLE t is one batch). If one of these words follows a read statement, it's another statement
const MSSQL_STATEMENT_WORDS: &[&str] = &[
    "drop", "alter", "create", "exec", "execute", "truncate", "grant", "revoke", "deny", "declare", "waitfor", "backup", "restore", "kill", "use",
    "dbcc", "shutdown", "reconfigure", "bulk", "set", "begin", "commit", "rollback", "save", "checkpoint", "if", "while", "goto", "print", "raiserror", "throw",
    "end", "setuser", "revert", "open", "close", "fetch", "deallocate", "add", "updatetext", "writetext",
];
/// MSSQL_STATEMENT_WORDS that are common column names (stock prices' open · close) and change no data as a statement of their own:
/// is_read doesn't count them as a chained statement (read_only_violation still refuses them, with the [name] hint)
const MSSQL_HARMLESS_STATEMENT_WORDS: &[&str] = &["open", "close", "add", "save", "use", "print", "end"];
/// SQL Server: functions that read other servers · files
const MSSQL_EXTERNAL_FUNCS: &[&str] = &["openrowset", "opendatasource", "openquery"];
/// ClickHouse table functions: read outside the server (other servers · files · URLs) (MCP)
const CLICKHOUSE_EXTERNAL_FUNCS: &[&str] = &[
    "url", "urlcluster", "file", "filecluster", "s3", "s3cluster", "gcs", "azureblobstorage", "azureblobstoragecluster", "hdfs", "hdfscluster", "remote",
    "remotesecure", "cluster", "clusterallreplicas", "mysql", "postgresql", "mongodb", "redis", "sqlite", "jdbc", "odbc", "executable", "iceberg", "icebergs3",
    "deltalake", "hudi", "input", "icebergaz", "icebergazure", "iceberghdfs", "iceberglocal", "icebergcluster", "icebergs3cluster", "deltalakecluster",
    "hudicluster", "hive", "arrowflight", "azureblob", "cosn", "oss", "ytsaurus",
];
/// ClickHouse table function families that keep growing new members (deltaLakeS3, icebergAzureCluster, paimonS3 …)
const CLICKHOUSE_EXTERNAL_PREFIXES: &[&str] = &["deltalake", "iceberg", "hudi", "paimon", "azureblob"];

fn clickhouse_external(name: &str) -> bool {
    CLICKHOUSE_EXTERNAL_FUNCS.contains(&name) || CLICKHOUSE_EXTERNAL_PREFIXES.iter().any(|p| name.starts_with(p))
}

/// SQL Server words that can start another statement but aren't reserved, so a read can't be judged by them (the driver rolls reads back).
/// MCP execute runs without that transaction, so it refuses them too
const MSSQL_UNRESERVED_STATEMENT_WORDS: &[&str] = &["disable", "enable", "receive", "send", "move", "get"];

/// DuckDB: functions that read files · URLs (MCP. The driver also opens with external access disabled — qurry_core::SANDBOX)
const DUCKDB_EXTERNAL_FUNCS: &[&str] = &[
    "read_text", "read_blob", "read_csv", "read_csv_auto", "read_json", "read_json_auto", "read_json_objects", "read_json_objects_auto", "read_ndjson",
    "read_ndjson_auto", "read_ndjson_objects", "read_parquet", "parquet_scan", "parquet_metadata", "parquet_schema", "parquet_file_metadata", "parquet_kv_metadata",
    "read_xlsx", "sniff_csv", "glob", "iceberg_scan", "delta_scan", "getenv", "query_table",
];

/// SQL Server runs a whole batch and needs no `;` between statements: the first bare word after the first word (at `fi`) that starts
/// another statement. CASE … END and OFFSET … ROWS FETCH NEXT are part of a SELECT. (Non-reserved words like RECEIVE or DISABLE are left
/// to the driver, which runs reads in a transaction it always rolls back). `skip`: words not to count
fn mssql_next_statement<'a>(toks: &'a [Tok], fi: usize, skip: &[&str]) -> Option<&'a Tok> {
    let mut case_depth = 0i32;
    let mut prev = "";
    toks.iter().skip(fi + 1).filter(|t| t.word && !t.quoted).find(|t| {
        let w = t.text.as_str();
        let hit = match w {
            "case" => {
                case_depth += 1;
                false
            }
            "end" if case_depth > 0 => {
                case_depth -= 1;
                false
            }
            "fetch" => !matches!(prev, "row" | "rows"),
            _ => MSSQL_STATEMENT_WORDS.contains(&w) && !skip.contains(&w),
        };
        prev = w;
        hit
    })
}

/// Whether it may run as read-only. strict = MCP (also blocks side-effect functions). Returns the reason when blocked
pub fn read_only_violation(sql: &str, kind: DbKind, strict: bool) -> Option<String> {
    if kind == DbKind::Redis {
        return None;
    }
    let head = sql.trim_start().split(|c: char| c.is_whitespace() || c == '(').next().unwrap_or("").to_lowercase();
    // SELECT * FROM … sent by the explorer goes through the SQL rules below
    if kind.is_vector() && head != "select" {
        if crate::sql::split_statements(sql, kind).len() != 1 {
            return Some("Read-only mode runs exactly one request".into());
        }
        return rest_read_violation(sql, kind);
    }
    if kind == DbKind::Mongodb && head != "select" {
        return mongo_read_violation(sql);
    }
    if crate::sql::split_statements(sql, kind).len() != 1 {
        return Some("Read-only mode runs exactly one statement".into());
    }
    let sc = scan(sql, kind);
    if sc.exec_comment {
        return Some("MySQL executable comments (/*! … */, /*M! … */) are not allowed in read-only mode".into());
    }
    let first = sc.toks.iter().find(|t| t.word).map(|t| t.text.as_str()).unwrap_or("");
    let allowed = READ_WORDS.contains(&first) || (!strict && SESSION_WORDS.contains(&first));
    if !allowed {
        return Some(format!("Read-only mode: '{first}' statements are not allowed"));
    }
    // SQL Server reads run inside a transaction the driver rolls back. COMMIT would end it and let the rest of the batch autocommit
    if kind == DbKind::Mssql && SESSION_WORDS.contains(&first) && !["set", "use"].contains(&first) {
        return Some(format!("Read-only mode: '{first}' is not allowed on SQL Server (reads run in a transaction Qurry rolls back)"));
    }
    let words: Vec<&str> = sc.toks.iter().map(|t| t.text.as_str()).collect();
    let bare = |t: &Tok| t.word && !t.quoted;
    if kind == DbKind::Mssql {
        let fi = sc.toks.iter().position(|t| t.word).unwrap_or(0);
        // Starting with [name], SQL Server treats it as a procedure call (without EXEC)
        if sc.toks.get(fi).is_some_and(|t| t.quoted) {
            return Some("Read-only mode: a statement can't start with a [name] (SQL Server runs it as a procedure)".into());
        }
        if let Some(t) = mssql_next_statement(&sc.toks, fi, &[]) {
            return Some(format!("Read-only mode: '{}' starts another statement (SQL Server runs the whole batch; write [{}] for a column)", t.text, t.text));
        }
    }
    // SELECT … INTO a new table (PostgreSQL · SQL Server)
    if matches!(kind, DbKind::Postgres | DbKind::Mssql) && sc.toks.iter().any(|t| bare(t) && t.text == "into") {
        return Some("Read-only mode: SELECT … INTO creates a table".into());
    }
    if kind == DbKind::Postgres {
        // U&"…" names can hide characters with \XXXX (U&"default_transaction_read\005fonly"). Only U& right before a quote (u&4 is a bitwise AND)
        let quote_after = |t: &Tok| matches!(sql.as_bytes().get(t.end), Some(b'\'' | b'"'));
        if sc.toks.windows(2).any(|w| w[0].word && !w[0].quoted && w[0].text == "u" && w[1].text == "&" && w[1].start == w[0].end && quote_after(&w[1])) {
            return Some("Read-only mode: U&\"…\" / U&'…' escapes are not allowed".into());
        }
        // Changing how \ is interpreted in strings makes the check and the server see string ends differently
        // (SHOW only reads them)
        if first != "show" && words.iter().any(|w| w.contains("standard_conforming_strings") || w.contains("backslash_quote")) {
            return Some("Read-only mode: changing how strings are read is not allowed".into());
        }
    }
    // Turning off read-only via session settings (SET default_transaction_read_only = off, SET SESSION TRANSACTION READ WRITE, START TRANSACTION READ WRITE)
    if words.iter().any(|w| w.contains("read_only")) || words.windows(2).any(|w| w == ["read", "write"]) {
        return Some("Read-only mode: changing the read-only setting is not allowed".into());
    }
    // SELECT … INTO OUTFILE / DUMPFILE (writing files on the MySQL server), SELECT … INTO new_table (PG)
    if words.windows(2).any(|w| w[0] == "into" && (w[1] == "outfile" || w[1] == "dumpfile")) {
        return Some("Read-only mode: SELECT … INTO OUTFILE/DUMPFILE is not allowed".into());
    }
    // Writes inside a statement (WITH d AS (DELETE …) SELECT …)
    if let Some(w) = dml_words(&sc.toks).next() {
        return Some(format!("Read-only mode: {} inside the statement is not allowed", sc.toks[w].text.to_uppercase()));
    }
    for (i, t) in sc.toks.iter().enumerate() {
        // Quoted names like "set_config"(…) · `file`(…) are function names too
        let call = t.word && sc.toks.get(i + 1).is_some_and(|n| n.text == "(");
        // set_config can turn off the read-only setting, so it's blocked even for read-only Data Sources. Other side-effect functions only in MCP (strict)
        if call && (t.text == "set_config" || (strict && SIDE_EFFECT_FUNCS.contains(&t.text.as_str()))) {
            return Some(format!("Read-only mode: {}() is not allowed (it has side effects outside the transaction)", t.text));
        }
        let external = match kind {
            DbKind::Mssql => MSSQL_EXTERNAL_FUNCS.contains(&t.text.as_str()),
            DbKind::Clickhouse => strict && clickhouse_external(&t.text),
            DbKind::Duckdb => strict && DUCKDB_EXTERNAL_FUNCS.contains(&t.text.as_str()),
            _ => false,
        };
        if call && external {
            return Some(format!("Read-only mode: {}() reads files or other servers", t.text));
        }
    }
    None
}

/// Vector DB REST: only GET and read POSTs (search · query · scroll · get · count · describe · list …)
/// The Milvus driver sends POST regardless of method, so only the path is checked
fn rest_read_violation(sql: &str, kind: DbKind) -> Option<String> {
    let first = sql.trim().lines().next().unwrap_or("");
    let mut parts = first.split_whitespace();
    let method = parts.next().unwrap_or("").to_uppercase();
    let raw = parts.next().unwrap_or("");
    let raw_path = raw.split('?').next().unwrap_or("");
    // Characters that can make the checked path differ from what the server receives (nothing after # is sent, the server decodes %2F · ..)
    if raw_path.contains(['#', '%', '\\']) || raw_path.split('/').any(|s| s == "." || s == "..") || raw.chars().any(|c| c.is_control()) {
        return Some(format!("Read-only mode: {raw} is not a plain request path"));
    }
    let path = raw_path.trim_end_matches('/').to_lowercase();
    let last = path.rsplit('/').next().unwrap_or("");
    const READS: &[&str] = &["search", "query", "scroll", "get", "count", "recommend", "discover", "facet", "groups", "describe", "list", "has", "get_stats", "get_load_state", "points", "hybrid_search"];
    let read_path = READS.contains(&last) || (last == "batch" && (path.contains("/search/") || path.contains("/query/") || path.contains("/recommend/")));
    let ok = if kind == DbKind::Milvus { read_path && last != "points" } else { method == "GET" || (method == "POST" && read_path) };
    (!ok).then(|| format!("Read-only mode: {method} {path} may change data (only GET and search/query requests run)"))
}

/// Text with JSON5 escapes (\uXXXX · \xXX · \n …, line continuations) decoded. Also decodes \uXXXX in unquoted names
fn json5_unescape(t: &str) -> String {
    let mut out = String::with_capacity(t.len());
    let mut it = t.chars().peekable();
    let hex = |it: &mut std::iter::Peekable<std::str::Chars>, n: usize| -> Option<char> {
        let h: String = (0..n).filter_map(|_| it.next()).collect();
        u32::from_str_radix(&h, 16).ok().and_then(char::from_u32)
    };
    while let Some(c) = it.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match it.next() {
            Some('u') => out.extend(hex(&mut it, 4)),
            Some('x') => out.extend(hex(&mut it, 2)),
            Some('\n' | '\r' | '\u{2028}' | '\u{2029}') => {}
            Some(o) => out.push(o),
            None => {}
        }
    }
    out
}

/// MongoDB shell: only single-line reads (find · aggregate (without \$out/\$merge) · count …)
fn mongo_read_violation(sql: &str) -> Option<String> {
    let t = sql.trim().trim_end_matches(';');
    const READS: &[&str] = &["find", "findone", "aggregate", "countdocuments", "estimateddocumentcount", "count", "distinct", "getindexes", "stats"];
    // db.<coll>.<method>( …  — collection names can contain dots, so look at the word before the last '('
    let call = t.strip_prefix("db.").and_then(|r| r.split_once('(')).map(|(head, _)| head.rsplit('.').next().unwrap_or("").to_lowercase());
    // The driver reads JSON5: "$out" · '\x24merge' are also $out · $merge. Decode escapes before checking
    let plain = json5_unescape(t);
    let writes_stage = ["$out", "$merge", "$function", "$accumulator"].iter().any(|k| plain.contains(k));
    // Chained calls (.sort().limit()) are fine, but not multiple statements
    let one = !t.contains(';');
    match call {
        Some(m) if READS.contains(&m.as_str()) && !writes_stage && one => None,
        _ => Some("Read-only mode: only find / aggregate / count / distinct queries run".into()),
    }
}

/// Positions of write verbs (INSERT/UPDATE/DELETE/MERGE). Excludes quoted names and non-write uses
/// (SELECT … FOR [NO KEY] UPDATE, ON CONFLICT DO UPDATE, ON DUPLICATE KEY UPDATE)
fn dml_words(toks: &[Tok]) -> impl Iterator<Item = usize> + '_ {
    toks.iter().enumerate().filter_map(move |(i, t)| {
        let dml = t.word && !t.quoted && ["insert", "update", "delete", "merge"].contains(&t.text.as_str());
        let prev = i.checked_sub(1).and_then(|p| toks.get(p)).map(|p| p.text.as_str()).unwrap_or("");
        (dml && !(t.text == "update" && ["for", "key", "do"].contains(&prev))).then_some(i)
    })
}

/// Number of elements a single Redis command may return in MCP
pub const REDIS_STRICT_MAX: i64 = 1000;

/// Options of the Redis reads the MCP limits check: how many leading arguments (key · range …) come first, and each option with
/// how many values it takes
fn redis_opt_spec(cmd: &str) -> (usize, &'static [(&'static str, usize)]) {
    match cmd {
        "ZRANGE" => (3, &[("BYSCORE", 0), ("BYLEX", 0), ("REV", 0), ("LIMIT", 2), ("WITHSCORES", 0)]),
        "ZRANGEBYSCORE" | "ZREVRANGEBYSCORE" | "ZRANGEBYLEX" | "ZREVRANGEBYLEX" | "ZREVRANGE" => (3, &[("LIMIT", 2), ("WITHSCORES", 0)]),
        "XRANGE" | "XREVRANGE" => (3, &[("COUNT", 1)]),
        "LPOS" => (2, &[("RANK", 1), ("COUNT", 1), ("MAXLEN", 1)]),
        "SCAN" => (1, &[("MATCH", 1), ("COUNT", 1), ("TYPE", 1)]),
        "HSCAN" | "SSCAN" | "ZSCAN" => (2, &[("MATCH", 1), ("COUNT", 1), ("NOVALUES", 0)]),
        "GEOSEARCH" => (1, &[("FROMMEMBER", 1), ("FROMLONLAT", 2), ("BYRADIUS", 2), ("BYBOX", 3), ("ASC", 0), ("DESC", 0), ("COUNT", 1), ("ANY", 0),
            ("WITHCOORD", 0), ("WITHDIST", 0), ("WITHHASH", 0)]),
        "XINFO" => (2, &[("FULL", 0), ("COUNT", 1)]),
        _ => (3, &[]),
    }
}

/// Options in a Redis command's arguments as Redis reads them: (name, position), in order. Skips each option's values
/// (so in SCAN 0 MATCH COUNT, COUNT is the pattern) and the leading arguments (so a key named LIMIT · COUNT doesn't fool it).
/// Unknown words are skipped (Redis refuses them)
fn redis_opts(cmd: &str, args: &[String]) -> Vec<(&'static str, usize)> {
    let (fixed, spec) = redis_opt_spec(cmd);
    let mut out = Vec::new();
    let mut i = fixed;
    while i < args.len() {
        match spec.iter().find(|(n, _)| args[i].eq_ignore_ascii_case(n)) {
            Some(&(n, arity)) => {
                out.push((n, i));
                i += 1 + arity;
            }
            None => i += 1,
        }
    }
    out
}

/// Position of an option (LIMIT, COUNT, BYSCORE …) in a Redis command's arguments. The last one when repeated, as Redis reads it
/// (redis_strict_violation refuses repeats anyway)
fn redis_opt(cmd: &str, args: &[String], name: &str) -> Option<usize> {
    redis_opts(cmd, args).into_iter().rev().find(|(n, _)| *n == name).map(|(_, i)| i)
}

/// Elements LRANGE / ZRANGE start stop returns, from the indexes alone (both from the start, or both from the end).
/// None when one counts from the start and the other from the end: that depends on the length
fn redis_range_len(start: i64, stop: i64) -> Option<i64> {
    ((start >= 0) == (stop >= 0)).then(|| stop.saturating_sub(start).saturating_add(1).max(0))
}

/// A read of a whole key (or of whole sets) that MCP runs only when it is small: count the elements with `count` on each key first
/// (at most REDIS_STRICT_MAX in all), otherwise suggest `instead`
#[derive(Debug, PartialEq)]
pub struct RedisSizeCheck {
    pub count: &'static str,
    pub keys: Vec<String>,
    pub instead: &'static str,
}

/// Whether a Redis read command needs a size check before it runs in MCP (redis_strict_violation lets these through). args as in redis_strict_violation
pub fn redis_size_check(cmd: &str, args: &[String]) -> Option<RedisSizeCheck> {
    let num = |i: usize| args.get(i).and_then(|a| a.parse::<i64>().ok());
    let opt = |name: &str| redis_opt(cmd, args, name);
    let unbounded = matches!((num(1), num(2)), (Some(a), Some(b)) if redis_range_len(a, b).is_none());
    let by = opt("BYSCORE").or(opt("BYLEX")).is_some();
    let check = |count, instead| Some(RedisSizeCheck { count, keys: args.first().cloned().into_iter().collect(), instead });
    match cmd {
        "HGETALL" | "HKEYS" | "HVALS" => check("HLEN", "HSCAN"),
        "SMEMBERS" => check("SCARD", "SSCAN"),
        "SINTER" | "SUNION" | "SDIFF" => Some(RedisSizeCheck { count: "SCARD", keys: args.to_vec(), instead: "SSCAN on each set" }),
        "LRANGE" if unbounded => check("LLEN", "a bounded range such as LRANGE key 0 99"),
        "ZREVRANGE" if unbounded => check("ZCARD", "ZSCAN or a bounded range"),
        "ZRANGE" if !by && unbounded => check("ZCARD", "ZSCAN or a bounded range"),
        "ZRANGE" if by && opt("LIMIT").is_none() => check("ZCARD", "LIMIT offset count"),
        "ZRANGEBYSCORE" | "ZRANGEBYLEX" | "ZREVRANGEBYSCORE" | "ZREVRANGEBYLEX" if opt("LIMIT").is_none() => check("ZCARD", "LIMIT offset count"),
        "XRANGE" | "XREVRANGE" if opt("COUNT").is_none() => check("XLEN", "COUNT"),
        _ => None,
    }
}

/// Read commands additionally blocked in Redis read-only (MCP): ones that expose config · passwords (CONFIG GET) or can stall the server (KEYS),
/// and reads of more than REDIS_STRICT_MAX elements (with millions of elements the server stalls and MCP runs out of memory).
/// Whole-key reads (HGETALL, LRANGE 0 -1, XRANGE without COUNT …) pass here: they also need redis_size_check at run time. args = arguments after the command (not uppercased)
pub fn redis_strict_violation(cmd: &str, args: &[String]) -> Option<String> {
    let num = |i: usize| args.get(i).and_then(|a| a.parse::<i64>().ok());
    let opt = |name: &str| redis_opt(cmd, args, name);
    // LIMIT · COUNT must be 1 or more (some commands treat 0 · negative as "all")
    let too_many = |n: Option<i64>| n.is_none_or(|n| n <= 0 || n > REDIS_STRICT_MAX);
    // Unbounded ranges (0 -1) are left to the size check
    let range = |start: Option<i64>, stop: Option<i64>| match (start, stop) {
        (Some(a), Some(b)) => redis_range_len(a, b).is_some_and(|n| n > REDIS_STRICT_MAX),
        _ => true,
    };
    let need_limit = |what: &str| Some(format!("{cmd} needs {what} of at most {REDIS_STRICT_MAX} in read-only mode"));
    // Redis takes the last of a repeated option (LIMIT 0 1 LIMIT 0 -1 reads everything)
    let opts = redis_opts(cmd, args);
    if let Some((n, _)) = opts.iter().enumerate().find_map(|(k, o)| opts[..k].iter().any(|p| p.0 == o.0).then_some(o)) {
        return Some(format!("{cmd}: {n} given more than once — not allowed in read-only mode"));
    }
    match cmd {
        "CONFIG" => Some("CONFIG is not allowed in read-only mode (it can reveal passwords)".into()),
        "KEYS" => Some("KEYS is not allowed in read-only mode (it blocks the server on large databases) — use redis_scan".into()),
        "LRANGE" | "ZREVRANGE" if range(num(1), num(2)) => need_limit("a start..stop range"),
        // ZRANGE key start stop [BYSCORE | BYLEX] [REV] [LIMIT offset count]
        "ZRANGE" if opt("BYSCORE").or(opt("BYLEX")).is_some() && opt("LIMIT").is_some_and(|i| too_many(num(i + 2))) => need_limit("LIMIT offset count"),
        "ZRANGE" if opt("BYSCORE").or(opt("BYLEX")).is_none() && range(num(1), num(2)) => need_limit("a start..stop range"),
        "ZRANGEBYSCORE" | "ZRANGEBYLEX" | "ZREVRANGEBYSCORE" | "ZREVRANGEBYLEX" if opt("LIMIT").is_some_and(|i| too_many(num(i + 2))) => need_limit("LIMIT offset count"),
        "XRANGE" | "XREVRANGE" if opt("COUNT").is_some_and(|i| too_many(num(i + 1))) => need_limit("COUNT"),
        // A negative count returns that many (with repeats)
        "SRANDMEMBER" | "HRANDFIELD" | "ZRANDMEMBER" if args.len() > 1 && num(1).is_none_or(|n| n.abs() > REDIS_STRICT_MAX) => need_limit("a count"),
        "SCAN" | "HSCAN" | "SSCAN" | "ZSCAN" if opt("COUNT").is_some_and(|i| too_many(num(i + 1))) => need_limit("COUNT"),
        "LPOS" if opt("COUNT").is_some_and(|i| too_many(num(i + 1))) => need_limit("COUNT"),
        "GEOSEARCH" if opt("COUNT").is_none_or(|i| too_many(num(i + 1))) => need_limit("COUNT"),
        "XINFO" if opt("FULL").is_some() && opt("COUNT").is_some_and(|i| too_many(num(i + 1))) => need_limit("COUNT"),
        _ => None,
    }
}

/// Reason if the statement is risky and needs confirmation before running: UPDATE/DELETE without WHERE (including inside a CTE), DROP, TRUNCATE.
/// EXPLAIN ANALYZE actually runs, so check the statement after it (plain EXPLAIN does not run)
pub fn unsafe_statement(sql: &str, kind: DbKind) -> Option<String> {
    if kind == DbKind::Redis {
        // Read it the same way as when running (quoting like "FLUSHALL" is the same command)
        let cmd = redis_args(sql).into_iter().next().unwrap_or_default().to_uppercase();
        return match cmd.as_str() {
            "FLUSHALL" | "FLUSHDB" | "DEL" | "UNLINK" | "SHUTDOWN" | "SWAPDB" | "REPLICAOF" | "SLAVEOF" => Some(format!("{cmd} deletes or replaces data")),
            "EVAL" | "EVALSHA" | "EVAL_RO" | "EVALSHA_RO" | "FCALL" | "FUNCTION" | "SCRIPT" => Some(format!("{cmd} runs a script that can change anything")),
            _ => None,
        };
    }
    if kind == DbKind::Mongodb {
        return mongo_unsafe(sql);
    }
    if kind.is_vector() && !sql.trim_start().to_lowercase().starts_with("select") {
        return rest_unsafe(sql);
    }
    let sc = scan(sql, kind);
    let mut toks: &[Tok] = &sc.toks;
    if toks.first().is_some_and(|t| t.word && t.text == "explain") {
        // Skip options (ANALYZE, (ANALYZE, BUFFERS), FORMAT …) up to the first verb
        let body = toks.iter().position(|t| t.word && t.depth == 0 && ["select", "with", "insert", "update", "delete", "merge", "values", "table"].contains(&t.text.as_str()))?;
        if !toks[..body].iter().any(|t| t.word && (t.text == "analyze" || t.text == "analyse")) {
            return None;
        }
        toks = &toks[body..];
    }
    let first = toks.iter().find(|t| t.word).map(|t| t.text.as_str()).unwrap_or("");
    match first {
        "drop" => return Some(format!("DROP {} removes the object and its data", toks.iter().filter(|t| t.word).nth(1).map(|t| t.text.to_uppercase()).unwrap_or_default())),
        "truncate" => return Some("TRUNCATE deletes every row".into()),
        // UPDATE/DELETE inside trigger · privilege definitions are not writes
        "create" | "alter" | "grant" | "revoke" | "comment" => return None,
        _ => {}
    }
    // For each UPDATE/DELETE: whether a WHERE exists at the same paren depth (top level up to the end of the statement, CTE up to its closing paren)
    for i in dml_words(toks) {
        let v = &toks[i];
        if v.text != "update" && v.text != "delete" {
            continue;
        }
        let d = v.depth;
        let scope = toks[i + 1..].iter().take_while(|t| t.depth >= d && !(t.text == ")" && t.depth == d));
        if !scope.filter(|t| t.depth == d).any(|t| t.word && !t.quoted && t.text == "where") {
            return Some(format!("{} without WHERE changes every row", v.text.to_uppercase()));
        }
    }
    None
}

/// Arguments of a Redis command line (same rules as running: redis_cmd::parse_command). Empty list if unreadable
fn redis_args(line: &str) -> Vec<String> {
    crate::redis_cmd::parse_command(line.trim().trim_end_matches(';')).map(|a| a.iter().map(|x| String::from_utf8_lossy(x).into_owned()).collect()).unwrap_or_default()
}

/// MongoDB: dropping collections · DBs, deleting or updating all documents with an empty filter
fn mongo_unsafe(sql: &str) -> Option<String> {
    let plain: String = json5_unescape(sql).chars().filter(|c| !c.is_whitespace()).collect::<String>().to_lowercase();
    if plain.starts_with('{') {
        let first = plain.trim_start_matches('{').trim_start_matches(['"', '\'']).split(['"', '\'', ':']).next().unwrap_or("").to_string();
        return ["drop", "dropdatabase", "dropindexes", "delete", "renamecollection", "update", "converttocapped", "applyops", "emptycapped", "dropallusersfromdatabase", "dropallrolesfromdatabase"]
            .contains(&first.as_str()).then(|| format!("the {first} command can remove or change every document"));
    }
    if plain.contains(".drop(") || plain.contains(".dropdatabase(") || plain.contains(".dropindexes(") || plain.contains(".renamecollection(") {
        return Some("drop removes the collection and its documents".into());
    }
    if plain.contains("$out") || plain.contains("$merge") {
        return Some("$out / $merge overwrite a collection".into());
    }
    // Commands that rewrite or drop data whichever way they are sent (runCommand, adminCommand, a command document)
    for c in ["converttocapped", "applyops", "emptycapped", "dropallusersfromdatabase", "dropallrolesfromdatabase"] {
        let c = c.to_lowercase();
        if plain.contains(&format!("{c}\"")) || plain.contains(&format!("{c}'")) || plain.contains(&format!("{c}:")) || plain.contains(&format!(".{c}(")) {
            return Some(format!("{c} can rewrite or remove data"));
        }
    }
    // Read the filter like the driver does: missing, null, {} or {/* comment */} all match every document
    if let Some(c) = MONGO_CALL.captures(sql.trim().trim_end_matches(';').trim()) {
        let m = c[3].to_lowercase();
        if ["deletemany", "updatemany", "remove", "replaceone", "deleteone", "updateone"].contains(&m.as_str()) {
            let filter = mongo_args(&c[4]).and_then(|a| a.into_iter().next());
            if !filter.as_ref().is_some_and(mongo_filter_doc) {
                return Some(format!("{} with an empty filter changes every document", &c[3]));
            }
        }
    }
    None
}

/// Vector DB REST: dropping collections · indexes (DELETE, …/drop), deleting points without a filter
fn rest_unsafe(sql: &str) -> Option<String> {
    let (first, body) = sql.trim().split_once('\n').unwrap_or((sql.trim(), ""));
    let mut parts = first.split_whitespace();
    let method = parts.next().unwrap_or("").to_uppercase();
    let raw = parts.next().unwrap_or("");
    // An encoded or relative path (%64rop, ../) could be anything once the server decodes it
    if raw.contains(['%', '#', '\\']) || raw.split('/').any(|s| s == "." || s == "..") {
        return Some(format!("{raw} is not a plain path — check what it does"));
    }
    let path = raw.split('?').next().unwrap_or("").trim_end_matches('/').to_lowercase();
    let inline: String = parts.collect::<Vec<_>>().join(" ");
    if method == "DELETE" {
        return Some(format!("DELETE {path} removes it and everything in it"));
    }
    let last = path.rsplit('/').next().unwrap_or("");
    if last == "drop" || last == "drop_index" || last == "release" {
        return Some(format!("{path} removes it and everything in it"));
    }
    // Snapshot recovery replaces the collection (and fetches a URL the server chooses to trust)
    if path.split('/').any(|s| s == "snapshots") && method != "GET" {
        return Some(format!("{path} can replace the collection"));
    }
    if last == "batch" {
        return Some(format!("{path} can delete points or whole payloads"));
    }
    if last == "delete" {
        // Only deleting listed ids is clearly bounded. A filter (even {"must": []}) may match every point
        let v = json5::from_str::<serde_json::Value>(&format!("{inline}{body}")).ok();
        let listed = |k: &str| v.as_ref().and_then(|v| v.get(k)).and_then(|x| x.as_array()).is_some_and(|a| !a.is_empty());
        let filtered = v.as_ref().is_some_and(|v| v.get("filter").is_some_and(|f| !f.is_string()) || v.get("where").is_some() || v.get("where_document").is_some());
        // Milvus deletes only by a filter expression ("id in [1, 2]"); an empty one matches everything
        let expr = v.as_ref().and_then(|v| v.get("filter")).and_then(|f| f.as_str()).is_some_and(|f| !f.trim().is_empty());
        if !(listed("points") || listed("ids") || expr) || filtered {
            return Some(format!("{path} with a filter or without ids may delete every point — check the filter"));
        }
    }
    None
}

/// MCP execute is an allow-list: everything else is refused with "run it in Qurry".
/// SQL: one INSERT, or UPDATE / DELETE with a WHERE (no DDL, no MERGE, no functions with side effects or external reads).
/// MongoDB: insertOne / insertMany, and updateOne / updateMany / replaceOne / deleteOne / deleteMany with a non-empty filter.
/// Vector DBs: adding / upserting points, and deleting points by id. Redis: redis_mcp_write_violation
pub fn mcp_execute_violation(sql: &str, kind: DbKind) -> Option<String> {
    let refuse = |why: &str| Some(format!("{why} — over MCP only INSERT, and UPDATE / DELETE with a WHERE, are allowed; ask the user to run it in Qurry"));
    match kind {
        DbKind::Redis => return redis_mcp_write_violation(sql),
        DbKind::Mongodb => return mongo_mcp_write_violation(sql),
        k if k.is_vector() => return rest_mcp_write_violation(sql, k),
        _ => {}
    }
    if crate::sql::split_statements(sql, kind).len() != 1 {
        return refuse("one statement per item");
    }
    let sc = scan(sql, kind);
    if sc.exec_comment {
        return refuse("MySQL executable comments");
    }
    let fi = sc.toks.iter().position(|t| t.word)?;
    let first = sc.toks[fi].text.as_str();
    if sc.toks[fi].quoted || !["insert", "update", "delete"].contains(&first) {
        return refuse(&format!("'{first}' statements are not allowed"));
    }
    if let Some(why) = unsafe_statement(sql, kind) {
        return refuse(&why);
    }
    let bare = |t: &Tok| t.word && !t.quoted;
    // SQL Server runs a whole batch: no second statement after the first one. UPDATE gets one SET, INSERT one INTO and one SELECT
    // (plus UNION branches); any other INTO (SELECT … INTO, OUTPUT … INTO) writes another table
    if kind == DbKind::Mssql {
        let mut case_depth = 0i32;
        let (mut sets, mut selects, mut prev) = (0, 0, "");
        for (n, t) in sc.toks.iter().skip(fi + 1).filter(|t| bare(t)).enumerate() {
            let w = t.text.as_str();
            let bad = match w {
                "case" => {
                    case_depth += 1;
                    false
                }
                "end" if case_depth > 0 => {
                    case_depth -= 1;
                    false
                }
                "set" if first == "update" && t.depth == 0 => {
                    sets += 1;
                    sets > 1
                }
                "into" => !(first == "insert" && n == 0),
                "select" if t.depth == 0 => {
                    selects += 1;
                    first != "insert" || (selects > 1 && !matches!(prev, "union" | "all" | "except" | "intersect"))
                }
                "select" => false,
                "merge" | "insert" | "update" | "delete" => true,
                _ => MSSQL_STATEMENT_WORDS.contains(&w) || MSSQL_UNRESERVED_STATEMENT_WORDS.contains(&w),
            };
            if bad {
                return refuse(&format!("'{w}' starts another statement or writes another table"));
            }
            prev = w;
        }
    }
    if sc.toks.iter().any(|t| bare(t) && t.text == "merge") {
        return refuse("MERGE is not allowed");
    }
    for (i, t) in sc.toks.iter().enumerate() {
        let call = t.word && sc.toks.get(i + 1).is_some_and(|n| n.text == "(");
        let name = t.text.as_str();
        let external = match kind {
            DbKind::Mssql => MSSQL_EXTERNAL_FUNCS.contains(&name),
            DbKind::Clickhouse => clickhouse_external(name),
            DbKind::Duckdb => DUCKDB_EXTERNAL_FUNCS.contains(&name),
            _ => false,
        };
        if call && (SIDE_EFFECT_FUNCS.contains(&name) || name == "set_config" || external) {
            return refuse(&format!("{name}() is not allowed"));
        }
    }
    None
}

/// MongoDB writes over MCP: single-collection insert / update / delete with a real filter (no drop, aggregate, commands or empty filters)
fn mongo_mcp_write_violation(sql: &str) -> Option<String> {
    let refuse = |why: &str| Some(format!("{why} — over MCP only insert, and update / delete with a filter, are allowed; ask the user to run it in Qurry"));
    let t = sql.trim().trim_end_matches(';').trim();
    let Some(c) = MONGO_CALL.captures(t) else { return refuse("only db.collection.method(…)") };
    let method = &c[3];
    let args = mongo_args(&c[4]);
    let Some(args) = args else { return refuse("can't read the arguments") };
    if ["$out", "$merge", "$function", "$accumulator", "$where"].iter().any(|k| json_has_key(&serde_json::Value::Array(args.clone()), k)) {
        return refuse("$out, $merge, $function, $accumulator and $where are not allowed");
    }
    let filter_ok = || args.first().is_some_and(mongo_filter_doc);
    match method {
        "insertOne" | "insertMany" => None,
        "updateOne" | "updateMany" | "replaceOne" | "deleteOne" | "deleteMany" if filter_ok() => None,
        "updateOne" | "updateMany" | "replaceOne" | "deleteOne" | "deleteMany" => refuse(&format!("{method} needs a non-empty filter")),
        m => refuse(&format!("{m} is not allowed")),
    }
}

/// The arguments of a MongoDB shell call, read like the driver (shell helpers → extended JSON, then JSON5)
fn mongo_args(s: &str) -> Option<Vec<serde_json::Value>> {
    static OID: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"ObjectId\(\s*["']([0-9a-fA-F]{24})["']\s*\)"#).unwrap());
    static DATE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"(?:ISODate|new Date)\(\s*["']([^"']+)["']\s*\)"#).unwrap());
    static LONG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"Number(?:Long|Int)\(\s*["']?(-?\d+)["']?\s*\)"#).unwrap());
    let s = OID.replace_all(s, r#"{"$$oid": "$1"}"#);
    let s = DATE.replace_all(&s, r#"{"$$date": "$1"}"#);
    let s = LONG.replace_all(&s, "$1");
    json5::from_str::<serde_json::Value>(&format!("[{s}]")).ok()?.as_array().cloned()
}

/// A filter the driver keeps as a non-empty document. Extended JSON such as {"$oid": …} or {"$numberLong": …}
/// becomes a scalar in the driver, which then refuses it; it must never count as a real filter here.
fn mongo_filter_doc(v: &serde_json::Value) -> bool {
    const SCALAR_KEYS: &[&str] = &[
        "$oid", "$date", "$numberlong", "$numberint", "$numberdouble", "$numberdecimal", "$binary", "$uuid", "$regularexpression",
        "$regex", "$timestamp", "$minkey", "$maxkey", "$undefined", "$symbol", "$code", "$dbpointer",
    ];
    v.as_object().is_some_and(|o| !o.is_empty() && !o.keys().any(|k| SCALAR_KEYS.contains(&k.to_lowercase().as_str())))
}

fn json_has_key(v: &serde_json::Value, key: &str) -> bool {
    match v {
        serde_json::Value::Object(o) => o.iter().any(|(k, v)| k == key || json_has_key(v, key)),
        serde_json::Value::Array(a) => a.iter().any(|v| json_has_key(v, key)),
        _ => false,
    }
}

/// Vector DB writes over MCP: add / upsert points, delete points by id (no collection changes, filters that may match everything, snapshots …)
fn rest_mcp_write_violation(sql: &str, kind: DbKind) -> Option<String> {
    let refuse = |why: &str| Some(format!("{why} — over MCP only adding points and deleting points by id are allowed; ask the user to run it in Qurry"));
    let (first, rest) = sql.trim().split_once('\n').unwrap_or((sql.trim(), ""));
    let mut parts = first.split_whitespace();
    let method = parts.next().unwrap_or("").to_uppercase();
    let raw = parts.next().unwrap_or("");
    let inline: String = parts.collect::<Vec<_>>().join(" ");
    if raw.contains(['#', '%', '\\', '?']) || raw.split('/').any(|s| s == "." || s == "..") {
        return refuse("not a plain request path");
    }
    let path = raw.trim_end_matches('/').to_lowercase();
    let seg: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    let body: serde_json::Value = json5::from_str(format!("{inline}\n{rest}").trim()).unwrap_or(serde_json::Value::Null);
    let ids = |k: &str| body.get(k).and_then(|v| v.as_array()).is_some_and(|a| !a.is_empty());
    static IDS_IN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s*\w+\s+in\s*\[[^\]]+\]\s*$").unwrap());
    let ok = match kind {
        // PUT /collections/c/points (upsert), POST /collections/c/points/delete {points: [ids]}
        DbKind::Qdrant => matches!((method.as_str(), seg.as_slice()), ("PUT", ["collections", _, "points"])) || (method == "POST" && matches!(seg.as_slice(), ["collections", _, "points", "delete"]) && ids("points") && body.get("filter").is_none()),
        // POST …/collections/c/add | upsert | update, …/delete {ids: [...]} (no where)
        DbKind::Chroma => {
            method == "POST"
                && match seg.last() {
                    Some(&("add" | "upsert" | "update")) => true,
                    Some(&"delete") => ids("ids") && body.get("where").is_none() && body.get("where_document").is_none(),
                    _ => false,
                }
        }
        // POST /entities/insert | upsert, /entities/delete {filter: "id in [..]"}
        DbKind::Milvus => match seg.as_slice() {
            [.., "entities", "insert" | "upsert"] => true,
            [.., "entities", "delete"] => body.get("filter").and_then(|f| f.as_str()).is_some_and(|f| IDS_IN.is_match(f)),
            _ => false,
        },
        _ => false,
    };
    if ok { None } else { refuse(&format!("{method} {path}")) }
}

/// What MCP execute can do on Redis: reads and writes that change one or a few keys only (not scripts · config · replication · deletes · renames · range deletes)
/// Largest SETRANGE byte offset (SETBIT: bits) an MCP client may write at
const REDIS_MCP_MAX_OFFSET: u64 = 1 << 20;

pub fn redis_mcp_write_violation(line: &str) -> Option<String> {
    let args = redis_args(line);
    let cmd = args.first().map(|c| c.to_uppercase()).unwrap_or_default();
    let sub = args.get(1).map(|c| c.to_uppercase()).unwrap_or_default();
    const WRITES: &[&str] = &[
        "SET", "SETNX", "MSET", "MSETNX", "APPEND", "SETRANGE", "SETBIT", "INCR", "INCRBY", "INCRBYFLOAT", "DECR", "DECRBY", "GETSET",
        "HSET", "HSETNX", "HMSET", "HDEL", "HINCRBY", "HINCRBYFLOAT", "LPUSH", "RPUSH", "LPUSHX", "RPUSHX", "LSET", "LINSERT", "LPOP", "RPOP", "LREM",
        "SADD", "SREM", "SMOVE", "ZADD", "ZREM", "ZINCRBY", "XADD", "XDEL", "XACK", "PFADD", "GEOADD", "PERSIST",
        "JSON.SET", "JSON.NUMINCRBY", "JSON.ARRAPPEND", "JSON.ARRINSERT", "JSON.STRAPPEND", "JSON.TOGGLE",
    ];
    if crate::redis_cmd::is_read_command(&cmd, &sub) {
        return redis_strict_violation(&cmd, &args[1.min(args.len())..]);
    }
    if !WRITES.contains(&cmd.as_str()) {
        return Some(format!("{cmd} is not allowed over MCP — ask the user to run it in Qurry"));
    }
    // An expiry (even 1 ms) or popping many elements amounts to deleting data
    let expiry = cmd == "SET" && args.iter().skip(3).any(|a| ["EX", "PX", "EXAT", "PXAT"].contains(&a.to_uppercase().as_str()));
    let many = matches!(cmd.as_str(), "LPOP" | "RPOP") && args.get(2).is_some_and(|n| n.parse::<i64>() != Ok(1));
    if expiry || many {
        return Some(format!("{cmd} with that option can delete data — refused over MCP"));
    }
    // A far offset makes Redis allocate the whole value up to it (up to 512 MB per key)
    let limit = match cmd.as_str() {
        "SETRANGE" => Some(REDIS_MCP_MAX_OFFSET),
        "SETBIT" => Some(REDIS_MCP_MAX_OFFSET * 8),
        _ => None,
    };
    let far = limit.is_some_and(|max| args.get(2).is_none_or(|o| o.parse::<u64>().map_or(true, |o| o >= max)));
    far.then(|| format!("{cmd} offset must be below {} bytes over MCP", REDIS_MCP_MAX_OFFSET))
}

/// Whether the statement doesn't change data (Production confirmation, Explain Analyse rollback, the Production log, and
/// counting "already committed changes" in script failure notes). A call to a side-effect function (dblink_exec, setval, nextval …) is not a read,
/// nor is SELECT … INTO (a new table in PostgreSQL · SQL Server, a file on the MySQL server, or variables).
/// Text with several statements (a selection run as one statement) is a read only if every statement is: `select 1; do $$ … $$` is not.
/// With a backslash the server's settings decide where strings end (MySQL NO_BACKSLASH_ESCAPES and ANSI_QUOTES,
/// PostgreSQL standard_conforming_strings), so the statement must be a read under every rule
pub fn is_read(sql: &str, kind: DbKind) -> bool {
    // Redis: one line = one command, read as redis_cmd::is_read_command reads it (the Terminal's command table agrees)
    if kind == DbKind::Redis {
        let lines = crate::sql::split_statements(sql, kind);
        return !lines.is_empty()
            && lines.iter().all(|&(a, b)| {
                let args = redis_args(&sql[a..b]);
                let up = |i: usize| args.get(i).map(|s| s.to_uppercase()).unwrap_or_default();
                !args.is_empty() && crate::redis_cmd::is_read_command(&up(0), &up(1))
            });
    }
    let pieces = statements(sql, kind);
    // Each statement as the server splits them, and the whole text read at once (where another quote rule ends the strings)
    let read = |sc: Scan| read_one(sc, kind);
    !pieces.is_empty() && pieces.iter().all(|p| every_reading(p, kind, read)) && every_reading(sql, kind, read)
}

/// The statements in the text as the server splits them, without the ones that are only comments
fn statements(sql: &str, kind: DbKind) -> Vec<&str> {
    crate::sql::split_statements(sql, kind).into_iter().map(|(a, b)| &sql[a..b]).filter(|p| scan(p, kind).toks.iter().any(|t| t.word)).collect()
}

fn read_one(sc: Scan, kind: DbKind) -> bool {
    let call = |i: usize| sc.toks.get(i + 1).is_some_and(|n| n.text == "(");
    // The first word of each statement in this reading: under another quote rule a `;` can end a string early and start one
    let mut firsts = vec![];
    let mut next = true;
    for t in &sc.toks {
        if !t.word && t.text == ";" {
            next = true;
        } else if next && t.word {
            firsts.push(t.text.as_str());
            next = false;
        }
    }
    // SQL Server also starts a statement without `;` (`select 1 exec purge`)
    let chained = kind == DbKind::Mssql && sc.toks.iter().position(|t| t.word).is_some_and(|fi| mssql_next_statement(&sc.toks, fi, MSSQL_HARMLESS_STATEMENT_WORDS).is_some());
    !firsts.is_empty()
        && !chained
        && firsts.iter().all(|w| READ_WORDS.contains(w))
        && !sc.toks.iter().enumerate().any(|(i, t)| {
            let w = t.text.as_str();
            t.word
                && (["insert", "update", "delete", "merge"].contains(&w)
                    || (!t.quoted && w == "into")
                    || (call(i) && (SIDE_EFFECT_FUNCS.contains(&w) || WRITE_FUNCS.contains(&w))))
        })
}

/// Whether a read locks the rows it returns: FOR UPDATE / FOR NO KEY UPDATE / FOR SHARE / FOR KEY SHARE, MySQL LOCK IN SHARE MODE.
/// Running it again (loading more after a reconnect) would take those locks again; under every quote reading, like is_read
pub fn locks_rows(sql: &str, kind: DbKind) -> bool {
    !every_reading(sql, kind, |sc| {
        let words: Vec<&str> = sc.toks.iter().filter(|t| t.word && !t.quoted).map(|t| t.text.as_str()).collect();
        !words.windows(2).any(|w| matches!(w, ["for", "update" | "share" | "no" | "key"] | ["lock", "in"]))
    })
}

/// `f` holds for the text under every way the server may read its quotes once there is a backslash (see is_read)
fn every_reading(sql: &str, kind: DbKind, f: impl Fn(Scan) -> bool) -> bool {
    let backslash = matches!(kind, DbKind::Mysql | DbKind::Postgres) && sql.contains('\\');
    let ansi = backslash && kind == DbKind::Mysql && sql.contains('"');
    f(scan(sql, kind)) && (!backslash || f(scan_as(sql, kind, Quotes::OtherBackslash))) && (!ansi || f(scan_as(sql, kind, Quotes::Ansi)))
}

/// Calls whose effect a ROLLBACK doesn't undo (a narrower question than SIDE_EFFECT_FUNCS: set_config, pg_advisory_xact_lock and
/// lo_import are undone; sleeping and reading files change nothing)
const ROLLBACK_SURVIVES: &[&str] = &[
    // PostgreSQL: sequences · other sessions · server files and config · other servers · session locks · WAL, statistics and replication
    "nextval", "setval", "pg_terminate_backend", "pg_cancel_backend", "pg_reload_conf", "pg_rotate_logfile", "pg_file_write", "lo_export",
    "dblink", "dblink_exec", "dblink_connect", "dblink_connect_u", "dblink_send_query", "pg_advisory_lock", "pg_try_advisory_lock",
    "pg_switch_wal", "pg_create_restore_point", "pg_promote", "pg_stat_reset", "pg_stat_reset_shared", "pg_stat_reset_single_table_counters",
    "pg_stat_reset_single_function_counters", "pg_stat_reset_slru", "pg_stat_reset_replication_slot", "pg_stat_reset_subscription_stats",
    "pg_drop_replication_slot", "pg_create_logical_replication_slot", "pg_create_physical_replication_slot", "pg_logical_emit_message",
    "pg_replication_origin_create", "pg_replication_origin_drop", "pg_replication_slot_advance",
    // run SQL given as a string (the name checks can't see inside it)
    "query_to_xml", "query_to_xmlschema", "query_to_xml_and_xmlschema", "ts_stat",
    // MySQL: session locks
    "get_lock", "release_lock", "release_all_locks",
];

/// Whether rolling back the transaction Explain Analyse wraps a write in undoes all it did. Not for calls whose effects live outside
/// the transaction (nextval and setval, dblink_exec, server files, advisory locks, other sessions …) nor SELECT … INTO OUTFILE / DUMPFILE:
/// the GUI and the TUI ask before running those and say afterwards that the rollback didn't undo them. Nor for text with several
/// statements (a selection run as one statement): only the first is explained, and a COMMIT or a MySQL DDL among the others ends the transaction
pub fn rollback_undoes(sql: &str, kind: DbKind) -> bool {
    statements(sql, kind).len() <= 1
        && every_reading(sql, kind, |sc| {
            let call = |i: usize| sc.toks.get(i + 1).is_some_and(|n| n.text == "(");
            !sc.toks.iter().enumerate().any(|(i, t)| t.word && call(i) && ROLLBACK_SURVIVES.contains(&t.text.as_str()))
                && !sc.toks.windows(2).any(|w| w[0].word && !w[0].quoted && w[0].text == "into" && w[1].word && !w[1].quoted && ["outfile", "dumpfile"].contains(&w[1].text.as_str()))
        })
}

/// Explain Analyse of a write a rollback can't undo (rollback_undoes): why the GUI and the TUI ask first
const ANALYZE_UNDONE_ASK: &str =
    "a rollback can't undo what it does outside the transaction: sequence values, other servers, files, locks, or a COMMIT · MySQL DDL among several statements";
/// What a trigger or function may do that a rollback doesn't undo (their bodies can't be checked by name)
const FUNCTION_EFFECTS: &str = "sequence values, other servers, files, locks";

/// Why Explain Analyse should ask before it runs `sql` (it really runs the statement), or None to run it without asking.
/// PostgreSQL · MySQL run it in a transaction they roll back, reads too: they ask for a statement the rollback doesn't fully undo
/// by what it names (rollback_undoes), and for an UPDATE / DELETE without WHERE (it locks every row until the rollback, and a MySQL
/// MyISAM or MEMORY table keeps the change). What a user-defined function does outside the transaction can't be seen by its name,
/// so that isn't asked (docs/safety.md says so). Other kinds aren't wrapped (drivers may not support transactions), so they ask
/// for every write. GUI and TUI ask with these words
pub fn explain_analyze_ask(sql: &str, kind: DbKind) -> Option<String> {
    let read = is_read(sql, kind);
    if !matches!(kind, DbKind::Postgres | DbKind::Mysql) {
        return (!read).then(|| unsafe_statement(sql, kind).unwrap_or_else(|| "it changes data".into()));
    }
    if !read && !rollback_undoes(sql, kind) {
        return Some(ANALYZE_UNDONE_ASK.into());
    }
    let my = if kind == DbKind::Mysql { "; a MyISAM or MEMORY table keeps the change" } else { "" };
    unsafe_statement(sql, kind).filter(|_| !read).map(|why| format!("{why}, holding their locks until the rollback undoes it{my}"))
}

/// What Output says after Explain Analyse ran `sql` in the transaction it rolled back (PostgreSQL · MySQL), as the words after
/// "Explain Analyse ": (warn, text). None for a read (nothing to say)
pub fn explain_analyze_note(sql: &str, kind: DbKind) -> Option<(bool, String)> {
    const RAN: &str = "ran the statement in a transaction and rolled it back";
    if is_read(sql, kind) {
        return None;
    }
    Some(match rollback_undoes(sql, kind) {
        false => (true, format!("{RAN}, but what it did outside the transaction stays ({FUNCTION_EFFECTS}, or a COMMIT · MySQL DDL among several statements)")),
        // A trigger, or a function the statement calls, can still reach outside the transaction: no name check sees inside them
        true => (false, format!("{RAN}: its table changes were undone, but not what a trigger or function did outside the transaction ({FUNCTION_EFFECTS})")),
    })
}

/// Whether the statement changes structure (the explorer tree is reloaded after running). Skips leading comments
pub fn is_ddl(sql: &str, kind: DbKind) -> bool {
    if kind == DbKind::Redis {
        return false;
    }
    let sc = scan(sql, kind);
    let first = sc.toks.iter().find(|t| t.word).map(|t| t.text.as_str()).unwrap_or("");
    ["create", "alter", "drop", "truncate", "comment", "rename", "grant", "revoke"].contains(&first)
}

#[cfg(test)]
mod tests {

    #[test]
    fn ddl_statements() {
        let pg = |q: &str| is_ddl(q, DbKind::Postgres);
        assert!(pg("ALTER TABLE users ADD COLUMN email text"));
        assert!(pg("-- add_column: users.email\nALTER TABLE users ADD COLUMN email text"));
        assert!(pg("/* x */ create index i on t(a)"));
        assert!(pg("drop table t"));
        assert!(!pg("select 1"));
        assert!(!pg("update t set a = 1 where id = 2"));
        assert!(!pg("with x as (select 1) select * from x"));
        assert!(is_ddl("RENAME TABLE a TO b", DbKind::Mysql));
        assert!(!is_ddl("DEL key", DbKind::Redis));
    }

    /// PROD safeguard: query counting rows to change. Simple UPDATE / DELETE only (None otherwise)
    #[test]
    fn impact_count_queries() {
        let pg = |q: &str| impact_count_sql(q, DbKind::Postgres);
        assert_eq!(pg("UPDATE orders SET status = 'paid' WHERE id = 42;").as_deref(), Some("SELECT count(*) FROM orders WHERE id = 42"));
        assert_eq!(pg("update public.\"Order Items\" o set qty = qty + 1 where o.qty < 0 returning *").as_deref(), Some("SELECT count(*) FROM public.\"Order Items\" o WHERE o.qty < 0"));
        assert_eq!(pg("DELETE FROM logs.user_activity_log WHERE ts < now() - interval '30 days'").as_deref(), Some("SELECT count(*) FROM logs.user_activity_log WHERE ts < now() - interval '30 days'"));
        assert_eq!(pg("delete from t").as_deref(), Some("SELECT count(*) FROM t"));
        assert_eq!(pg("delete from t returning id").as_deref(), Some("SELECT count(*) FROM t"));
        // Keywords inside parens · strings in the condition stay as is
        assert_eq!(pg("update t set a = 'where' where b in (select x from y where z) -- c").as_deref(), Some("SELECT count(*) FROM t WHERE b in (select x from y where z)"));
        // Not countable
        assert_eq!(pg("UPDATE t SET a = s.a FROM s WHERE t.id = s.id"), None);
        assert_eq!(pg("DELETE FROM t USING s WHERE t.id = s.id"), None);
        assert_eq!(pg("WITH x AS (SELECT 1) DELETE FROM t"), None);
        assert_eq!(pg("select 1"), None);
        assert_eq!(pg("insert into t values (1)"), None);
        let my = |q: &str| impact_count_sql(q, DbKind::Mysql);
        assert_eq!(my("UPDATE `order` SET a = 1 WHERE b = 2").as_deref(), Some("SELECT count(*) FROM `order` WHERE b = 2"));
        assert_eq!(my("DELETE FROM t WHERE a = 1 ORDER BY id LIMIT 10"), None);
        assert_eq!(my("UPDATE a JOIN b ON a.id = b.id SET a.x = 1"), None);
        assert_eq!(my("UPDATE a, b SET a.x = b.x WHERE a.id = b.id"), None);
    }

    /// Shape used for undo SQL: table name (without alias) · changed columns
    #[test]
    fn simple_dml_shape() {
        let d = simple_dml("update public.\"Order Items\" o set qty = qty + 1, o.\"Note\" = 'x', (a, B) = (1, 2) where o.qty < 0", DbKind::Postgres).unwrap();
        assert_eq!(d.table, "public.\"Order Items\"");
        assert_eq!(d.from, "public.\"Order Items\" o");
        assert_eq!(d.cond.as_deref(), Some("o.qty < 0"));
        assert_eq!(d.set_cols, vec!["qty", "Note", "a", "b"]);
        assert!(!d.delete);
        let d = simple_dml("DELETE FROM `Shop`.`Order` WHERE id = 1", DbKind::Mysql).unwrap();
        assert!(d.delete && d.set_cols.is_empty());
        assert_eq!(d.table, "`Shop`.`Order`");
        assert_eq!(simple_dml("update t set a = (select max(x) from y where z = 1), b = 2", DbKind::Postgres).unwrap().set_cols, vec!["a", "b"]);
    }
    use super::*;
    use DbKind::{Chroma, Milvus, Mongodb, Mysql, Postgres, Qdrant};

    /// The MongoDB driver reads commands with Unicode \w \s (drivers/mongodb): the guard must too, or it stops warning about
    /// commands the driver runs. Fails if shared's regex loses Unicode classes or case folding (regex-lite, a feature cut)
    #[test]
    fn regex_stays_unicode() {
        assert!(regex::Regex::new(r"^\w+$").unwrap().is_match("users_한"));
        assert!(regex::Regex::new(r"^\s$").unwrap().is_match("\u{3000}"));
        assert!(regex::Regex::new(r"(?i)^like$").unwrap().is_match("li\u{212A}e")); // Kelvin sign folds to k
        for q in ["db.users_한.deleteMany({})", "db.getCollection(\u{3000}\"users\").deleteMany({})", "db.getCollection(\u{a0}\"users\").updateMany({}, {$set: {a: 1}})"] {
            assert!(unsafe_statement(q, Mongodb).is_some_and(|w| w.contains("empty filter")), "{q:?}");
        }
        assert_eq!(mcp_execute_violation("db.users_한.deleteMany({a: 1})", Mongodb), None);
        assert_eq!(unsafe_statement("db.users.deleteMany({_id: ObjectId(\u{3000}\"5f1d7f0b2c3e4a5b6c7d8e9f\")})", Mongodb), None);
        // Value hints on non-ASCII columns
        assert!(crate::complete::context("select * from t where 상태 = '", false).is_some());
    }

    #[test]
    fn read_only_rest_and_mongo() {
        let ok = |s: &str, k| read_only_violation(s, k, false).is_none();
        assert!(ok("GET /collections", Qdrant));
        assert!(ok("POST /collections/x/points/search\n{vector: [1]}", Qdrant));
        assert!(ok("POST /collections/x/points/search/batch\n{}", Qdrant));
        assert!(ok("POST /entities/query\n{collectionName: 'x'}", Milvus));
        assert!(ok("SELECT * FROM qdrant.x", Qdrant));
        assert!(!ok("PUT /collections/x/points\n{points: []}", Qdrant));
        assert!(!ok("POST /collections/x/points/delete\n{}", Qdrant));
        assert!(!ok("DELETE /collections/x", Chroma));
        assert!(!ok("POST /collections/x/add\n{}", Chroma));
        assert!(!ok("POST /entities/insert\n{}", Milvus));
        assert!(!ok("GET /collections\nDELETE /collections/x", Qdrant));
        assert!(ok("db.users.find({age: {$gt: 1}}).sort({age: -1}).limit(5)", Mongodb));
        assert!(ok("db.users.aggregate([{$group: {_id: null}}])", Mongodb));
        assert!(ok("SELECT * FROM shop.users", Mongodb));
        assert!(!ok("db.users.aggregate([{$out: 'x'}])", Mongodb));
        assert!(!ok("db.users.deleteMany({})", Mongodb));
        assert!(!ok("db.users.find({}); db.users.drop()", Mongodb));
        assert!(!ok("{\"dropDatabase\": 1}", Mongodb));
    }

    #[test]
    fn read_only_blocks_known_bypasses() {
        let bad = |s: &str, k| read_only_violation(s, k, true).is_some();
        // MySQL forms that actually got through
        assert!(bad("select 1 --1; COMMIT; DELETE FROM t WHERE id=1", Mysql));
        assert!(bad("/*! COMMIT; DELETE FROM t; */ select 1", Mysql));
        assert!(bad("select /*!50000 1 */", Mysql));
        assert!(bad("select * from t into outfile '/tmp/x'", Mysql));
        assert!(bad("select load_file('/etc/passwd')", Mysql));
        assert!(bad("select 1; delete from t", Postgres));
        assert!(bad("delete from t", Postgres));
        assert!(bad("/* /* */ select */ delete from t", Postgres));
        assert!(bad("select pg_terminate_backend(123)", Postgres));
        assert!(bad("select pg_catalog.pg_read_file('/etc/passwd')", Postgres));
        assert!(bad("select set_config('transaction_read_only','off',true)", Postgres));
        assert!(bad("set default_transaction_read_only = off", Postgres));
        // Words inside strings · comments don't matter
        assert!(!bad("select 'pg_terminate_backend(1); delete'", Postgres));
        assert!(!bad("select $$; delete from t; $$", Postgres));
        assert!(!bad("select 1 -- x; delete", Mysql));
        assert!(!bad("with a as (select 1) select * from a", Postgres));
        assert!(!bad("explain select 1", Postgres));
        assert!(!bad("show tables", Mysql));
    }

    /// Bypasses found in the security review (2026-10)
    #[test]
    fn read_only_blocks_review_findings() {
        let bad = |s: &str, k| read_only_violation(s, k, true).is_some();
        // MariaDB executable comment /*M! … */ too, like /*! … */
        assert!(bad("select 1 /*M!100000 into outfile '/tmp/x' */", Mysql));
        assert!(bad("/*M! commit; delete from t */ select 1", Mysql));
        // \ inside MySQL backticks is not an escape: the server sees two statements
        assert!(bad("select 1 as `a\\`, 1 as `b`; delete from t; -- `", Mysql));
        // Writes inside a CTE (the first word alone is with)
        assert!(bad("with d as (delete from t returning *) select * from d", Postgres));
        // Read-only Data Sources (non-strict) also block turning off read-only via set_config
        let ds = |s: &str| read_only_violation(s, Postgres, false).is_some();
        assert!(ds("select set_config('default_transaction_read_only','off',false)"));
        assert!(ds("select pg_catalog.set_config('transaction_read_only', 'off', true)"));
        assert!(ds("with d as (update t set a = 1 returning *) select * from d"));
        // Admin functions that work even in a read-only transaction (MCP)
        assert!(bad("select pg_stat_reset()", Postgres));
        assert!(bad("select pg_drop_replication_slot('s')", Postgres));
        assert!(bad("select pg_logical_emit_message(true, 'p', 'x')", Postgres));
        // Reads pass as is
        assert!(!ds("select * from t for update"));
        assert!(!ds("select * from t for no key update"));
        assert!(!ds("select \"delete\" from t"));
        assert!(!bad("select `update` from t", Mysql));
    }

    /// Driver DBs and lexer edge cases (2026-10 review, round 2)
    #[test]
    fn read_only_plugin_and_lexer_edges() {
        use DbKind::{Clickhouse, Duckdb, Mssql};
        let bad = |s: &str, k| read_only_violation(s, k, true).is_some();
        let ds = |s: &str, k| read_only_violation(s, k, false).is_some();
        // SQL Server: statements chained without ; · SELECT INTO · external reads
        assert!(bad("SELECT 1 DROP TABLE dbo.orders", Mssql));
        assert!(ds("SELECT 1 EXEC xp_cmdshell 'whoami'", Mssql));
        assert!(ds("select 1\nexec('drop table t')", Mssql));
        assert!(ds("SELECT * INTO dbo.x FROM dbo.y", Mssql));
        assert!(ds("SELECT * FROM OPENROWSET('SQLNCLI', 'Server=x;', 'select 1')", Mssql));
        assert!(!ds("SELECT name, create_date FROM sys.tables WHERE name <> 'drop'", Mssql));
        assert!(!ds("SELECT [drop], [use] FROM dbo.t", Mssql));
        assert!(!ds("SELECT TOP 10 * FROM dbo.t ORDER BY id", Mssql));
        // REST: Milvus uses POST regardless of method, # · % · .. in the path
        assert!(bad("GET /collections/drop\n{\"collectionName\": \"prod\"}", Milvus));
        assert!(bad("GET /entities/delete\n{}", Milvus));
        assert!(!bad("POST /collections/list\n{}", Milvus));
        assert!(!bad("POST /entities/search\n{}", Milvus));
        assert!(bad("POST /collections/prod/points/delete#/search\n{}", Qdrant));
        assert!(bad("POST /api/v1/collections/x/delete#/query\n{}", Chroma));
        assert!(bad("POST /collections/x/points/delete%2F..%2Fsearch\n{}", Qdrant));
        assert!(bad("POST /collections/x/points/delete/../search\n{}", Qdrant));
        assert!(!bad("POST /collections/x/points/search?timeout=5 {\"limit\": 3}", Qdrant));
        // MongoDB: $out / $merge hidden with JSON5 escapes
        assert!(bad("db.users.aggregate([{\"\\u0024out\": \"users\"}])", Mongodb));
        assert!(bad("db.users.aggregate([{'\\x24merge': {into: 'x'}}])", Mongodb));
        assert!(bad("db.users.aggregate([{$o\\u0075t: 'users'}])", Mongodb));
        assert!(bad("db.users.aggregate([{$facet: {a: [{$merge: 'x'}]}}])", Mongodb));
        assert!(bad("db.users.find({$expr: {$function: {body: 'x', args: [], lang: 'js'}}})", Mongodb));
        assert!(!bad("db.users.find({name: {$regex: '\\\\d+'}})", Mongodb));
        // PostgreSQL: e' at the end of a word is not an E-string (so the next token isn't hidden)
        assert!(bad("select name'\\', pg_terminate_backend(pid) from pg_stat_activity --'", Postgres));
        assert!(ds("select name'\\', set_config('default_transaction_read_only','off',false) --'", Postgres));
        assert!(bad("select é'\\'; delete from t; --'", Postgres));
        assert!(!bad("select E'it\\'s', 1", Postgres));
        // PostgreSQL: U& names · changing string interpretation · SELECT INTO
        assert!(ds("SET U&\"default_transaction_read\\005fonly\" = off", Postgres));
        assert!(ds("set standard_conforming_strings = off", Postgres));
        assert!(bad("select u&'\\0041', 1", Postgres) && bad("SELECT U&\"x\" FROM t", Postgres) && bad("select 1 from t where U&'a' = 'a'", Postgres));
        for s in ["SELECT u&4 FROM t", "select u & 4, u&b from t", "show standard_conforming_strings", "SHOW backslash_quote", "select '--U&''x'''"] {
            assert!(!bad(s, Postgres) && !ds(s, Postgres), "{s}");
        }
        assert!(ds("reset standard_conforming_strings", Postgres) && ds("set session backslash_quote = on", Postgres));
        assert!(ds("select * into t2 from t", Postgres));
        assert!(!ds("select 'into', \"into\" from t", Postgres));
        // DuckDB · ClickHouse (MCP): reading files · other servers
        assert!(bad("SELECT * FROM read_text('/Users/x/.ssh/id_rsa')", Duckdb));
        assert!(bad("select getenv('HOME')", Duckdb));
        assert!(bad("SELECT * FROM url('http://169.254.169.254/', 'RawBLOB', 'x String')", Clickhouse));
        assert!(bad("select * from file('/etc/passwd', 'LineAsString')", Clickhouse));
        assert!(!bad("SELECT count() FROM system.tables", Clickhouse));
        assert!(!ds("SELECT * FROM read_csv('local.csv')", Duckdb), "the user's own read-only data source may read local files");
    }

    /// Bypasses found in re-review (2026-10 review, round 3)
    #[test]
    fn read_only_quoted_names_and_lexer_gaps() {
        use DbKind::{Clickhouse, Mssql};
        let bad = |s: &str, k| read_only_violation(s, k, true).is_some();
        let ds = |s: &str, k| read_only_violation(s, k, false).is_some();
        // Quoted function names are functions too
        assert!(ds("select \"set_config\"('default_transaction_read_only','off',false)", Postgres));
        assert!(ds("select pg_catalog.\"set_config\"('transaction_read_only','off',true)", Postgres));
        assert!(bad("select \"pg_terminate_backend\"(pid) from pg_stat_activity", Postgres));
        assert!(bad("select pg_terminate_backend (pid) from pg_stat_activity", Postgres));
        assert!(bad("SELECT * FROM `file`('/etc/passwd', 'LineAsString')", Clickhouse));
        // Non-ASCII dollar-quote tags
        assert!(bad("select $é$ ' $é$, pg_terminate_backend(pid) from pg_stat_activity --'", Postgres));
        assert!(!bad("select $é$ pg_terminate_backend(1) $é$", Postgres));
        // \' in ClickHouse strings and # comments
        assert!(bad("SELECT '\\'' , * FROM url('http://169.254.169.254/', 'RawBLOB', 'x String') --'", Clickhouse));
        assert!(bad("select 1 # '\n, * from file('/etc/passwd', 'LineAsString') --'", Clickhouse));
        assert!(!bad("SELECT 'it\\'s' AS a", Clickhouse));
        // SQL Server: check the following statement even if the first word is [name], more statement words
        assert!(ds("[desc] DROP TABLE dbo.orders", Mssql));
        // DISABLE TRIGGER / RECEIVE … after a read are undone by the driver, which runs reads in a transaction it rolls back
        assert!(ds("SELECT 1 EXEC xp_cmdshell 'whoami'", Mssql));
        assert!(!ds("SELECT [open], [close] FROM dbo.prices", Mssql));
    }

    /// Second review round (2026-10)
    #[test]
    fn read_only_review_round_two() {
        use DbKind::{Clickhouse, Duckdb, Mssql, Sqlite};
        let bad = |s: &str, k| read_only_violation(s, k, true).is_some();
        let ds = |s: &str, k| read_only_violation(s, k, false).is_some();
        // SQL Server: ordinary reads pass (CASE … END, OFFSET … FETCH, non-reserved column names)
        assert!(!ds("SELECT CASE WHEN a > 0 THEN 1 ELSE 0 END AS x FROM dbo.t", Mssql));
        assert!(!ds("SELECT * FROM dbo.t ORDER BY id OFFSET 0 ROWS FETCH NEXT 10 ROWS ONLY", Mssql));
        assert!(!ds("SELECT get, move, send FROM dbo.t", Mssql));
        assert!(ds("SELECT 1 END", Mssql), "END without CASE starts another statement");
        assert!(ds("SELECT CASE WHEN 1 = 1 THEN 1 END END", Mssql));
        assert!(ds("SELECT 1 FETCH NEXT FROM c", Mssql));
        // COMMIT would end the transaction the driver rolls back
        for s in ["COMMIT DISABLE TRIGGER ALL ON DATABASE", "COMMIT RECEIVE TOP(100000) * FROM q", "ROLLBACK", "BEGIN TRAN", "SAVE TRAN x"] {
            assert!(ds(s, Mssql), "{s}");
        }
        assert!(!ds("SET NOCOUNT ON", Mssql) && !ds("COMMIT", Postgres));
        assert!(bad("SELECT * FROM deltaLakeAzure('x')", Clickhouse) && bad("SELECT * FROM icebergAzureCluster('c', 'x')", Clickhouse) && bad("SELECT * FROM ytsaurus('x')", Clickhouse));
        assert!(!bad("SELECT encodeURLComponent(x), fileSize(y) FROM t", Clickhouse));
        // PostgreSQL: $ inside an identifier doesn't open a dollar quote
        assert_eq!(crate::sql::split_statements("SELECT 1 AS x$q$; DROP TABLE t; SELECT 1 AS y$q$", Postgres).len(), 3);
        assert!(bad("select query_to_xml('select pg_terminate_backend(1)', true, false, '')", Postgres));
        assert!(bad("select * from ts_stat('select 1')", Postgres));
        // SQLite [names], DuckDB E-strings and dollar quotes
        assert!(bad("WITH a AS (SELECT 1 AS [']) DELETE FROM t --']", Sqlite));
        assert!(bad("SELECT E'\\''; DROP TABLE t; --'", Duckdb));
        assert!(bad("SELECT $q$'$q$; DROP TABLE t; --'", Duckdb));
        // ClickHouse: heredocs, \ in quoted names, more external functions
        assert!(bad("SELECT $q$'$q$, * FROM url('http://x/', 'RawBLOB', 'x String') --'", Clickhouse));
        assert!(bad("SELECT 1 AS `\\``, * FROM url('http://x/', 'RawBLOB', 'x String') --`", Clickhouse));
        assert!(bad("SELECT * FROM icebergLocal('/tmp/x')", Clickhouse) && bad("SELECT * FROM hive('h', 'd', 't')", Clickhouse));
    }

    #[test]
    fn read_only_data_source_allows_session_statements() {
        let ok = |s: &str| read_only_violation(s, Postgres, false).is_none();
        assert!(ok("begin") && ok("rollback") && ok("set search_path = app"));
        // Explain Analyse inside an open transaction: SAVEPOINT qurry_explain … ROLLBACK TO SAVEPOINT qurry_explain
        for k in [Postgres, Mysql] {
            assert_eq!(read_only_violation("SAVEPOINT qurry_explain", k, false), None);
            assert_eq!(read_only_violation("ROLLBACK TO SAVEPOINT qurry_explain", k, false), None);
            assert_eq!(read_only_violation("RELEASE SAVEPOINT qurry_explain", k, false), None);
            assert!(read_only_violation("SAVEPOINT qurry_explain", k, true).is_some());
        }
        assert!(!ok("set default_transaction_read_only = off"));
        assert!(!ok("start transaction read write"));
        assert!(!ok("update t set a = 1"));
        // Data Source read-only doesn't block things like pg_sleep (MCP only)
        assert!(ok("select pg_sleep(0)"));
    }

    #[test]
    fn redis_strict_limits() {
        let v = |line: &str| {
            let mut w = line.split_whitespace();
            let cmd = w.next().unwrap().to_uppercase();
            redis_strict_violation(&cmd, &w.map(String::from).collect::<Vec<_>>())
        };
        let size = |line: &str| {
            let mut w = line.split_whitespace();
            let cmd = w.next().unwrap().to_uppercase();
            redis_size_check(&cmd, &w.map(String::from).collect::<Vec<_>>())
        };
        for bad in ["LRANGE l 0 5000", "LRANGE l -5000 -1", "ZREVRANGE z -1001 -1", "LRANGE l x 5", "ZRANGEBYSCORE z -inf +inf LIMIT 0 100000", "SRANDMEMBER s -100000",
            "SCAN 0 COUNT 1000000", "KEYS *", "CONFIG GET *", "XRANGE s - + COUNT 100000"] {
            assert!(v(bad).is_some(), "{bad}");
        }
        for bad in ["ZRANGEBYSCORE z -inf +inf LIMIT 0 -1", "ZRANGE z 0 +inf BYSCORE LIMIT 0 0", "LPOS l x COUNT 0", "GEOSEARCH g FROMLONLAT 0 0 BYRADIUS 9 km", "XINFO STREAM s FULL COUNT 0",
            "XRANGE s - + COUNT -1"] {
            assert!(v(bad).is_some(), "{bad}");
        }
        // Bounded by the indexes alone: the last 10, or an empty range
        for ok in ["LRANGE l -10 -1", "ZRANGE z -1000 -1", "ZREVRANGE z -3 -2", "LRANGE l 5 2", "LRANGE l -1 -5", "LRANGE l 0 999"] {
            assert!(v(ok).is_none() && size(ok).is_none(), "{ok}");
        }
        // Whole keys pass here and are counted at run time (qurry_core::redisdb::check_read)
        for (line, count, keys) in [("HGETALL big", "HLEN", vec!["big"]), ("HVALS h", "HLEN", vec!["h"]), ("SMEMBERS s", "SCARD", vec!["s"]), ("SUNION a b", "SCARD", vec!["a", "b"]),
            ("LRANGE l 0 -1", "LLEN", vec!["l"]), ("LRANGE l -10 5", "LLEN", vec!["l"]), ("ZRANGE z 0 -1", "ZCARD", vec!["z"]), ("ZREVRANGE z 0 -1", "ZCARD", vec!["z"]),
            ("ZRANGE z -inf +inf BYSCORE", "ZCARD", vec!["z"]), ("ZRANGEBYSCORE z -inf +inf", "ZCARD", vec!["z"]), ("XRANGE s - +", "XLEN", vec!["s"]),
            ("ZRANGEBYSCORE LIMIT 0 5", "ZCARD", vec!["LIMIT"]), ("XRANGE COUNT 5 +", "XLEN", vec!["COUNT"])] {
            assert!(v(line).is_none(), "{line}");
            let c = size(line).unwrap_or_else(|| panic!("{line}"));
            assert_eq!((c.count, c.keys.iter().map(String::as_str).collect::<Vec<_>>()), (count, keys), "{line}");
        }
        for ok in ["GET a", "HGET h f", "LRANGE l 0 99", "ZRANGEBYSCORE z 0 1 LIMIT 0 10", "XRANGE s - + COUNT 10", "ZRANGE z 0 +inf BYSCORE LIMIT 0 10"] {
            assert!(size(ok).is_none(), "{ok}");
        }
        for ok in ["LPOS l x", "LPOS l x COUNT 5", "GEOSEARCH g FROMLONLAT 0 0 BYRADIUS 9 km COUNT 10", "XINFO STREAM s", "XINFO STREAM s FULL COUNT 10", "SRANDMEMBER s -5"] {
            assert!(v(ok).is_none(), "{ok}");
        }
        for ok in ["GET a", "HGET h f", "HSCAN h 0 COUNT 100", "LRANGE l 0 99", "ZRANGE z 0 9", "ZRANGE z 0 +inf BYSCORE LIMIT 0 10", "ZRANGEBYSCORE z 0 1 LIMIT 0 10",
            "XRANGE s - + COUNT 10", "SRANDMEMBER s 5", "SRANDMEMBER s", "SCAN 0 MATCH a:* COUNT 100", "HLEN h"] {
            assert!(v(ok).is_none(), "{ok}");
        }
    }

    /// Redis takes the last of a repeated option (LIMIT 0 1 LIMIT 0 -1 reads everything), so a repeat is refused,
    /// and an option's value that looks like an option name is not read as one
    #[test]
    fn redis_repeated_options() {
        let split = |line: &str| {
            let mut w = line.split_whitespace();
            (w.next().unwrap().to_uppercase(), w.map(String::from).collect::<Vec<_>>())
        };
        let v = |line: &str| {
            let (cmd, a) = split(line);
            redis_strict_violation(&cmd, &a)
        };
        let size = |line: &str| {
            let (cmd, a) = split(line);
            redis_size_check(&cmd, &a)
        };
        for bad in ["ZRANGEBYSCORE z -inf +inf LIMIT 0 1 LIMIT 0 -1", "ZRANGE z -inf +inf BYSCORE LIMIT 0 1 LIMIT 0 -1", "XRANGE s - + COUNT 1 COUNT 100",
            "LPOS l x COUNT 1 COUNT 0", "XREVRANGE s + - count 1 COUNT 5000", "ZREVRANGEBYLEX z + - LIMIT 0 1 limit 0 -1", "SCAN 0 COUNT 10 COUNT 1000000",
            "HSCAN h 0 COUNT 10 COUNT 100000", "GEOSEARCH g FROMLONLAT 0 0 BYRADIUS 9 km COUNT 1 COUNT 100000", "XINFO STREAM s FULL COUNT 1 COUNT 0",
            "ZRANGE z 0 1 BYSCORE BYSCORE LIMIT 0 1", "ZRANGEBYSCORE z 0 1 WITHSCORES WITHSCORES LIMIT 0 1", "SCAN 0 MATCH a MATCH b", "SCAN 0 TYPE hash TYPE set",
            "LPOS l x RANK 1 RANK 2", "LPOS l x MAXLEN 1 MAXLEN 0", "ZRANGE z 0 1 REV REV"] {
            assert!(v(bad).is_some_and(|e| e.contains("more than once")), "{bad}: {:?}", v(bad));
        }
        // A value equal to an option name is a value: SCAN's MATCH COUNT is the pattern "COUNT"
        for bad in ["SCAN 0 MATCH COUNT COUNT 1000000", "SCAN 0 COUNT 1000000 MATCH COUNT", "LPOS l x RANK COUNT COUNT 0"] {
            assert!(v(bad).is_some(), "{bad}");
        }
        for ok in ["SCAN 0 MATCH COUNT COUNT 10", "SCAN 0 COUNT 10 MATCH COUNT", "LPOS l COUNT COUNT 5", "XRANGE s COUNT + COUNT 5", "ZRANGEBYSCORE z LIMIT +inf LIMIT 0 5"] {
            assert!(v(ok).is_none(), "{ok}: {:?}", v(ok));
        }
        assert!(size("XRANGE s COUNT + COUNT 5").is_none() && size("ZRANGEBYSCORE z LIMIT +inf LIMIT 0 5").is_none());
        // Other forms that read everything: LIMIT with a negative count, COUNT 0, a huge SCAN COUNT
        for bad in ["ZRANGE z -inf +inf BYSCORE LIMIT 0 -5", "ZRANGEBYLEX z - + LIMIT 0 -1", "LPOS l x COUNT 0 MAXLEN 0", "SSCAN s 0 COUNT 99999999", "ZRANGE z [a [z BYLEX REV LIMIT 0 -1"] {
            assert!(v(bad).is_some(), "{bad}");
        }
        // MAXLEN 0 only scans the whole list; COUNT still caps what comes back
        assert!(v("LPOS l x COUNT 5 MAXLEN 0").is_none());
    }

    #[test]
    fn unsafe_non_sql() {
        use DbKind::Redis;
        let u = |s: &str, k| unsafe_statement(s, k).is_some();
        assert!(u("\"FLUSHALL\"", Redis) && u("eval \"return 1\" 0", Redis) && u("FUNCTION FLUSH", Redis) && u("REPLICAOF evil 6379", Redis));
        assert!(!u("SET a 1", Redis));
        assert!(u("db.users.deleteMany({})", Mongodb) && u("db.users.deleteMany( { } )", Mongodb) && u("db.users.drop()", Mongodb) && u("db.dropDatabase()", Mongodb));
        assert!(u("db.users.updateMany({}, {$set: {a: 1}})", Mongodb) && u("{\"drop\": \"users\"}", Mongodb));
        assert!(!u("db.users.deleteMany({age: {$lt: 0}})", Mongodb) && !u("db.users.find({})", Mongodb));
        assert!(u("db.users.deleteMany(null)", Mongodb) && u("db.users.deleteMany({/*x*/})", Mongodb) && u("db.users.aggregate([{$out: 'x'}])", Mongodb));
        // Extended JSON scalars become "match everything" if a driver drops them, so they never count as a filter
        for s in ["db.c.deleteMany(ObjectId(\"507f1f77bcf86cd799439011\"))", "db.c.deleteMany({$numberLong: \"1\"})", "db.c.updateMany({$date: \"2020-01-01\"}, {$set: {a: 1}})"] {
            assert!(u(s, Mongodb), "{s}");
            assert!(mcp_execute_violation(s, Mongodb).is_some(), "{s}");
        }
        assert!(!u("db.c.deleteMany({_id: ObjectId(\"507f1f77bcf86cd799439011\")})", Mongodb));
        assert!(u("POST /collections/%64rop\n{}", Milvus) && u("POST /collections/x/../drop\n{}", Milvus));
        assert!(u("POST /collections/drop\n{\"collectionName\": \"x\"}", Milvus) && u("DELETE /collections/x", Qdrant));
        assert!(u("POST /collections/x/points/delete\n{\"filter\": {}}", Qdrant) && u("POST /collections/x/points/delete", Qdrant));
        assert!(!u("POST /collections/x/points/delete\n{\"points\": [1, 2]}", Qdrant) && !u("POST /entities/delete {\"filter\": \"id in [1]\"}", Milvus));
        // Confirmation for whatever may rewrite or delete everything
        for s in ["db.runCommand({convertToCapped: 'c', size: 1})", "db.adminCommand({applyOps: []})", "{\"convertToCapped\": \"c\", \"size\": 1}"] {
            assert!(u(s, Mongodb), "{s}");
        }
        for s in ["POST /collections/x/points/delete\n{\"filter\":{\"must\":[]}}", "POST /collections/x/points/batch\n{\"operations\": []}",
            "POST /collections/x/snapshots/recover\n{\"location\": \"http://x\"}", "PUT /collections/x/snapshots/upload"] {
            assert!(u(s, Qdrant), "{s}");
        }
        assert!(u("POST /entities/delete {\"filter\": \"\"}", Milvus) && u("POST /api/v1/collections/x/delete\n{where: {a: 1}}", Chroma));
    }

    #[test]
    fn redis_mcp_writes() {
        let v = |s: &str| redis_mcp_write_violation(s);
        for ok in ["SET a 1", "HSET h f v", "\"SET\" a 1", "PERSIST k", "GET a", "ZADD z 1 m", "LPOP l"] {
            assert!(v(ok).is_none(), "{ok}");
        }
        for bad in ["\"FLUSHALL\"", "EVAL \"redis.call('FLUSHALL')\" 0", "FUNCTION FLUSH", "CONFIG SET dir /tmp", "SAVE", "REPLICAOF h 1", "SWAPDB 0 1", "MIGRATE h 1 k 0 1",
            "RENAME a b", "ZREMRANGEBYRANK z 0 -1", "LTRIM l 0 0", "EXPIRE k 0", "EXPIRE k 1", "EXPIREAT k 1", "SET k v PX 1", "SETEX k 1 v", "LPOP k 1000000",
            "DEBUG SLEEP 1", "MODULE LOAD x", "DEL a", "LRANGE l 0 5000", "KEYS *", "FLUSHALL;"] {
            assert!(v(bad).is_some(), "{bad}");
        }
    }

    /// MCP execute allow-list
    #[test]
    fn mcp_execute_allow_list() {
        use DbKind::{Mssql, Sqlite};
        let ok = |s: &str, k| mcp_execute_violation(s, k).is_none();
        for s in ["insert into t (a) values (1)", "update t set a = 1 where id = 1", "delete from t where id = 1 returning id", "insert into t select * from s where x = 1"] {
            assert!(ok(s, Postgres), "{s}");
        }
        for s in ["create table t (a int)", "alter table t add b int", "drop table t", "truncate t", "delete from t", "update t set a = 1", "merge into t using s on true when matched then delete",
            "select 1", "update t set a = 1 where id = 1; drop table t", "insert into t select pg_terminate_backend(1)", "with d as (delete from t where id = 1) select 1"] {
            assert!(!ok(s, Postgres), "{s}");
        }
        assert!(!ok("UPDATE t SET a=1 WHERE id=1 DROP TABLE t", Mssql) && !ok("SELECT 1 TRUNCATE TABLE t", Mssql) && !ok("DELETE FROM t WHERE id = 1 EXEC xp_cmdshell 'x'", Mssql));
        assert!(ok("UPDATE t SET a = CASE WHEN b = 1 THEN 2 END WHERE id = 1", Mssql) && ok("INSERT INTO t (a) SELECT a FROM s WHERE b = 1", Mssql));
        for s in ["DELETE FROM t WHERE id=1 DELETE FROM u WHERE 1=1", "DELETE FROM t WHERE id=1 DISABLE TRIGGER ALL ON t", "INSERT INTO t VALUES (1) RECEIVE * FROM q",
            "UPDATE t SET a = 1 WHERE id=1 SET IMPLICIT_TRANSACTIONS ON", "INSERT INTO t SELECT * INTO t_copy FROM big", "INSERT INTO t SELECT 1 SELECT 2",
            "UPDATE t SET a = 1 WHERE id = 1 UPDATE u SET b = 2 WHERE id = 1", "DELETE FROM t OUTPUT deleted.a INTO audit WHERE id = 1"] {
            assert!(!ok(s, Mssql), "{s}");
        }
        assert!(ok("INSERT INTO t (a) SELECT a FROM s WHERE b = 1 UNION ALL SELECT a FROM u WHERE b = 2", Mssql) && ok("UPDATE t SET a = (SELECT max(b) FROM s) WHERE id = 1", Mssql));
        assert!(!ok("INSERT INTO FUNCTION deltaLakeS3('http://x') VALUES (1)", DbKind::Clickhouse) && !ok("INSERT INTO t SELECT * FROM paimonS3('x')", DbKind::Clickhouse));
        assert!(!ok("INSERT INTO t SELECT setval('s', 1)", Postgres));
        assert!(ok("INSERT INTO t VALUES (1)", Sqlite) && !ok("ATTACH 'x' AS y", Sqlite));
        // MongoDB
        assert!(ok("db.users.insertOne({name: 'a'})", Mongodb) && ok("db.users.deleteMany({age: {$lt: 0}})", Mongodb) && ok("db.users.updateOne({_id: ObjectId('0123456789abcdef01234567')}, {$set: {a: 1}})", Mongodb));
        for s in ["db.users.deleteMany(null)", "db.users.deleteMany({/*x*/})", "db.users.deleteMany({})", "db.users.drop()", "db.users.aggregate([{$out: 'x'}])", "db.runCommand({convertToCapped: 'x'})",
            "{\"drop\": \"users\"}", "db.users.updateMany({a: 1}, [{$set: {b: {$function: {body: 'x', args: [], lang: 'js'}}}}])"] {
            assert!(!ok(s, Mongodb), "{s}");
        }
        // Vector DBs
        assert!(ok("PUT /collections/c/points\n{points: [{id: 1, vector: [0.1]}]}", Qdrant) && ok("POST /collections/c/points/delete\n{points: [1, 2]}", Qdrant));
        for s in ["POST /collections/c/points/delete\n{filter: {must: []}}", "POST /collections/c/points/batch\n{}", "POST /collections/c/snapshots/recover\n{location: 'http://x'}", "DELETE /collections/c", "POST /collections/%64rop\n{}"] {
            assert!(!ok(s, Qdrant), "{s}");
        }
        assert!(ok("POST /entities/delete\n{collectionName: 'c', filter: 'id in [1, 2]'}", Milvus) && !ok("POST /entities/delete\n{collectionName: 'c', filter: 'id > 0'}", Milvus) && !ok("POST /collections/drop\n{}", Milvus));
        assert!(ok("POST /api/v1/collections/x/delete\n{ids: ['a']}", Chroma) && !ok("POST /api/v1/collections/x/delete\n{where: {}}", Chroma));
        // Redis offsets that make the server allocate the value up to them
        assert!(ok("SETRANGE k 10 x", DbKind::Redis) && !ok("SETRANGE k 536870000 x", DbKind::Redis) && !ok("SETBIT k 4294967295 1", DbKind::Redis) && ok("SETBIT k 7 1", DbKind::Redis));
    }

    #[test]
    fn unsafe_statements() {
        let u = |s: &str| unsafe_statement(s, Postgres);
        assert!(u("delete from t").is_some());
        assert!(u("update t set a = 1").is_some());
        assert!(u("update t set a = (select max(x) from y where y.k = 1)").is_some(), "WHERE inside a subquery doesn't count");
        assert!(u("delete from t where 'where' = 'x'").is_none());
        assert!(u("delete from t -- where id = 1").is_some(), "WHERE in a comment doesn't count");
        assert!(u("with d as (select 1) delete from t").is_some());
        assert!(u("drop table t").is_some() && u("truncate t").is_some());
        assert!(u("delete from t where id = 1").is_none());
        assert!(u("select * from t").is_none());
        // EXPLAIN ANALYZE actually runs. Plain EXPLAIN does not
        assert!(u("explain analyze delete from t").is_some());
        assert!(u("explain (analyze, buffers) update t set a = 1").is_some());
        assert!(u("explain delete from t").is_none());
        assert!(unsafe_statement("explain analyze delete from t", Mysql).is_some());
        // Write without WHERE inside a CTE
        assert!(u("with d as (delete from t returning *) select * from d").is_some());
        assert!(u("with d as (delete from t where id = 1 returning *) select * from d").is_none());
        // The word update used as a non-write
        assert!(u("insert into t values (1) on conflict (id) do update set a = 1").is_none());
        assert!(u("create trigger tr before update on t for each row execute function f()").is_none());
        assert!(u("grant update on t to u").is_none());
        assert!(u("select * from t for update").is_none());
        assert!(unsafe_statement("FLUSHALL", DbKind::Redis).is_some());
        assert!(unsafe_statement("GET a", DbKind::Redis).is_none());
    }

    /// Redis reads by the command table's rule (is_read_command): the Production question and the console ask only for writes;
    /// every line must read, a quoted command name is the same command, and an unknown command is a write
    #[test]
    fn is_read_redis() {
        use DbKind::Redis;
        for r in ["GET a", "hgetall h", "CONFIG GET maxmemory", "GET a\n# note\nTTL a", "\"GET\" a"] {
            assert!(is_read(r, Redis), "{r}");
        }
        for w in ["SET a 1", "GET a\nDEL a", "CONFIG SET maxmemory 1", "NOSUCH a", "", "\"GET"] {
            assert!(!is_read(w, Redis), "{w}");
        }
    }

    /// Where strings end depends on a server setting once there is a backslash: a read must be a read under both rules
    #[test]
    fn is_read_backslash_rules() {
        // MySQL NO_BACKSLASH_ESCAPES: the string ends after x\ and the DELETE runs
        assert!(!is_read("with a as (select 'x\\') delete from t -- ') select 1", Mysql));
        // Also when the scanner sees the write in a # comment the server reads as code
        assert!(!is_read("select 'a\\' , ' # ' delete from t -- '", Mysql));
        // PostgreSQL standard_conforming_strings = off: \' escapes and the CTE's DELETE is code
        assert!(!is_read("with a as (select 'x\\' as c, '), d as (delete from t returning 1) select 1 -- ' as e) select 1", Postgres));
        // Reads stay reads
        for s in ["select 'C:\\path', 'delete' as action", "select 'it\\'s'", "select 'delete' as action"] {
            assert!(is_read(s, Mysql) && is_read(s, Postgres), "{s}");
        }
        assert!(is_read("# delete\nselect 1", Mysql));
        // MySQL ANSI_QUOTES: \ escapes in '…' but "…" is a name where \ is plain, so the DELETE is code
        let ansi = r#"with a as (select 'p\' "  ' as "a\") delete from t -- ""#;
        assert!(!is_read(ansi, Mysql), "{ansi}");
        assert!(is_read(r#"select 'C:\path' as "a\""#, Mysql));
    }

    /// Dynamic SQL and other side-effect calls are not reads (Production asks, Explain Analyse rolls back)
    #[test]
    fn is_read_side_effect_calls() {
        for s in ["select dblink_exec('dbname=prod', 'delete from orders')", "select setval('s', 1)", r#"select "set_config"('a', 'b', false)"#] {
            assert!(!is_read(s, Postgres), "{s}");
        }
        assert!(!is_read("select get_lock('a', 10)", Mysql));
        // The name alone (a column, a string) is not a call
        assert!(is_read("select setval from t", Postgres));
        assert!(is_read("select 'dblink_exec(x)'", Postgres));
        // Sequences and large objects change data from a SELECT
        for s in ["select nextval('s')", "select lo_unlink(1)", "select lo_create(0)", "select lo_put(1, 0, 'x')"] {
            assert!(!is_read(s, Postgres), "{s}");
        }
        assert!(is_read("select nextval from t", Postgres));
        // NOTIFY reaches other sessions
        assert!(!is_read("select pg_notify('jobs', 'x')", Postgres));
        assert!(is_read("select pg_notify from t", Postgres));
    }

    /// Reads that lock their rows (loading more after a reconnect won't run them again)
    #[test]
    fn locks_rows_finds_the_locking_clauses() {
        for (s, kind) in [
            ("select * from t for update", Postgres),
            ("select * from t for no key update", Postgres),
            ("select * from t for share", Postgres),
            ("select * from t for key share of t skip locked", Postgres),
            ("select * from t FOR SHARE", Mysql),
            ("select * from t lock in share mode", Mysql),
        ] {
            assert!(locks_rows(s, kind), "{s}");
        }
        for (s, kind) in [
            ("select * from t order by id", Postgres),
            ("select 'for share', \"for\" from t -- for update", Postgres),
            ("select `lock`, `in` from t", Mysql),
            ("select substring(a for 2) from t", Postgres),
        ] {
            assert!(!locks_rows(s, kind), "{s}");
        }
        // A backslash: the clause outside a string under another quote rule counts
        assert!(locks_rows(r"select 'a\' for share --'", Postgres));
    }

    /// SELECT … INTO creates a table (PostgreSQL · SQL Server) or writes a file on the MySQL server: not a read
    #[test]
    fn is_read_select_into() {
        assert!(!is_read("select * into newtab from t", Postgres));
        assert!(!is_read("with x as (select 1 a) select * into newtab from x", Postgres));
        assert!(!is_read("select * from t into outfile '/tmp/x'", Mysql));
        assert!(!is_read("select a into @v from t", Mysql));
        assert!(!is_read("select * into newtab from t", DbKind::Mssql));
        // The word in a string, a comment or a quoted name is not INTO
        assert!(is_read("select 'into' as x, \"into\" from t -- into", Postgres));
        assert!(is_read("select `into` from t", Mysql));
    }

    /// Text with several statements (a selection run as one statement) is a read only if every statement is
    #[test]
    fn is_read_multi_statement() {
        for s in [
            "select 1; do $$ begin delete from orders; end $$",
            "select 1; call purge()",
            "select 1; create table x (a int)",
            "select 1; copy orders from program 'true'",
            "select 1;\nvacuum",
        ] {
            assert!(!is_read(s, Postgres), "{s}");
        }
        for s in ["select 1; prepare s from 'delete from orders'; execute s", "select 1; call purge()", "show tables; do sleep(1)"] {
            assert!(!is_read(s, Mysql), "{s}");
        }
        // Under NO_BACKSLASH_ESCAPES the string ends after x\ and CALL is a statement of its own
        assert!(!is_read("select 'x\\'; call purge(); -- '", Mysql));
        // Reads stay reads
        for s in ["select 1; select 2;", "select 1; -- done", "select ';'; select 2", "select 1;\n\n(select 2)"] {
            assert!(is_read(s, Postgres), "{s}");
        }
        assert!(is_read("show tables; select 1 # x; delete", Mysql));
        // SQL Server starts another statement without `;`
        for s in ["select 1 exec dbo.purge_orders", "select 1 drop table t", "select 1\ntruncate table dbo.t", "select 1 exec purge", "with x as (select 1 a) select a from x set nocount on"] {
            assert!(!is_read(s, DbKind::Mssql), "{s}");
        }
        for s in ["select case when a = 1 then 1 end from t", "select a from t order by a offset 0 rows fetch next 5 rows only", "select [set], [exec] from t", "select 1; select 2"] {
            assert!(is_read(s, DbKind::Mssql), "{s}");
        }
        // Statement words that are common column names and change nothing on their own stay reads; read-only mode still asks for [open]
        for s in ["select open, close from prices", "select add, save, print from t", "select [open], [close] from prices"] {
            assert!(is_read(s, DbKind::Mssql), "{s}");
        }
        assert!(read_only_violation("select open, close from prices", DbKind::Mssql, false).is_some());
        assert!(!is_read("select open from prices exec purge", DbKind::Mssql));
        assert!(!is_read("", Postgres) && !is_read(" -- only a note", Postgres));
    }

    /// Explain Analyse's rollback undoes table writes, not sequences, other servers, files or session locks
    #[test]
    fn rollback_undoes_only_what_the_transaction_holds() {
        for s in ["delete from t", "update t set a = 1", "insert into t values (1)", "select * into newtab from t", "select lo_unlink(1)", "select 'nextval(1)', nextval from t"] {
            assert!(rollback_undoes(s, Postgres), "{s}");
        }
        // Undone by the rollback, or changing nothing
        for s in ["select pg_sleep(1)", "select set_config('x.y', '1', false)", "select pg_advisory_xact_lock(1)", "select * from pg_read_file('f')"] {
            assert!(rollback_undoes(s, Postgres), "{s}");
        }
        assert!(rollback_undoes("select sleep(1)", Mysql));
        assert!(!rollback_undoes("select get_lock('x', 1)", Mysql));
        for s in [
            "select pg_advisory_lock(1)",
            "select pg_terminate_backend(42)",
            "select query_to_xml('delete from t', true, true, '')",
            "select nextval('s')",
            "select setval('s', 1)",
            "insert into t values (nextval('s'))",
            "select dblink_exec('dbname=x', 'delete from t')",
            "select 1; select pg_advisory_lock(1)",
        ] {
            assert!(!rollback_undoes(s, Postgres), "{s}");
        }
        assert!(!rollback_undoes("select * from t into outfile '/tmp/x'", Mysql));
        assert!(!rollback_undoes("select 1 into dumpfile '/tmp/x'", Mysql));
        assert!(rollback_undoes("delete from t where a = 'into outfile'", Mysql));
        // Under NO_BACKSLASH_ESCAPES the string ends after x\ and get_lock() is a call
        assert!(!rollback_undoes("select 'x\\', get_lock('a', 1) -- '", Mysql));
        // Only the first of several statements is explained; the others run as they are (COMMIT, MySQL DDL commit)
        assert!(!rollback_undoes("delete from t; commit", Postgres));
        assert!(!rollback_undoes("delete from t; create table x (a int)", Mysql));
        assert!(rollback_undoes("delete from t; -- done", Postgres));
    }

    /// Explain Analyse asks before what a rollback can't undo by what the statement names and before an UPDATE / DELETE without
    /// WHERE. A function it doesn't know isn't asked about: its table writes are rolled back, and what it does outside the
    /// transaction can't be seen by name (no list of built-ins to keep, so a built-in never asks by mistake)
    #[test]
    fn explain_analyze_asks_before_what_a_rollback_cant_undo() {
        let ask = |s: &str, k: DbKind| explain_analyze_ask(s, k);
        for s in [
            "select nextval('s')",
            "with x as (select nextval('s') n) select n from x",
            // A record-returning call with a column definition list is a call, not a WITH name(cols) AS ( header
            "select * from dblink('dbname=x', 'select a, b from t') as (a int, b text)",
            "select * from t where x between symmetric dblink_exec('dbname=x', 'delete from t') and 2",
            "select pg_advisory_lock(1)",
        ] {
            assert_eq!(ask(s, Postgres).as_deref(), Some(ANALYZE_UNDONE_ASK), "{s}");
        }
        for s in ["select * from t into outfile '/tmp/x'", "select get_lock('a', 1)"] {
            assert_eq!(ask(s, Mysql).as_deref(), Some(ANALYZE_UNDONE_ASK), "{s}");
        }
        for s in [
            "select count(*), max(created_at), coalesce(name, '') from t where id in (1, 2) and exists (select 1 from u)",
            "select archive_old_orders()",
            "select * from archive_rows() as (a int, b text)",
            "select trim(both archive() from x) from t",
            "select * from t offset 0 rows fetch next archive_rows() rows only",
            // Built-ins a rollback undoes or that change nothing
            "select pg_sleep(1)",
            "select set_config('a.b', 'c', false)",
            "select xmlelement(name a), range_merge(int4range(1, 2), int4range(3, 4))",
            "update orders set note = archive_old_orders() where id = 1",
            "update t set (a, b) = (1, 2) where id = 1",
            "insert into t (a, b) values (1, lower('x')) on conflict (a) do nothing",
        ] {
            assert_eq!(ask(s, Postgres), None, "{s}");
        }
        assert_eq!(ask("select g()", Mysql), None);
        assert_eq!(ask("insert into t (a) value (1)", Mysql), None);
        // Writes: an UPDATE / DELETE without WHERE (every row locked until the rollback)
        assert_eq!(ask("delete from t where id = 1", Postgres), None);
        assert_eq!(ask("delete from t", Postgres).as_deref(), Some("DELETE without WHERE changes every row, holding their locks until the rollback undoes it"));
        assert_eq!(ask("delete t from t", Mysql).as_deref(), Some("DELETE without WHERE changes every row, holding their locks until the rollback undoes it; a MyISAM or MEMORY table keeps the change"));
        // Not wrapped in a transaction: every write, as before
        assert_eq!(ask("delete from t where id = 1", DbKind::Sqlite).as_deref(), Some("it changes data"));
        assert_eq!(ask("delete from t", DbKind::Sqlite).as_deref(), Some("DELETE without WHERE changes every row"));
        assert_eq!(ask("select f()", DbKind::Sqlite), None);
    }

    /// What Output says after the rolled-back run: nothing for a read, a caveat for a write, a warning for what stays
    #[test]
    fn explain_analyze_note_says_what_the_rollback_left() {
        assert_eq!(explain_analyze_note("select * from t", Postgres), None);
        assert_eq!(explain_analyze_note("select archive()", Mysql), None);
        for s in ["update t set a = 1", "update t set a = f(1) where id = 1"] {
            let (warn, text) = explain_analyze_note(s, Postgres).unwrap();
            assert!(!warn && text.contains("its table changes were undone, but not what a trigger or function did"), "{text}");
        }
        let (warn, text) = explain_analyze_note("select nextval('s')", Postgres).unwrap();
        assert!(warn && text.contains("what it did outside the transaction stays"), "{text}");
    }
}
