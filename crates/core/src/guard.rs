//! 문장 검사: 읽기 전용(MCP·Read-only Data Source)과 위험 문장 확인(WHERE 없는 UPDATE/DELETE, DROP, TRUNCATE).
//! 서버와 같은 규칙으로 문자열·주석을 건너뛰고 단어를 읽는다 (sql::split_statements 와 같은 규칙).
//! 마지막 방어선은 서버의 READ ONLY 트랜잭션이고, 여기서는 그 밖으로 새는 것(문장 여러 개, 파일 쓰기, 관리 함수)을 막는다
use crate::store::DbKind;

/// 문자열·주석 밖의 단어(소문자)와 기호. depth = 괄호 깊이
#[derive(Debug, Clone, PartialEq)]
struct Tok {
    text: String,
    word: bool,
    depth: i32,
    /// "…" / `…` 로 감싼 이름 (키워드가 아니다)
    quoted: bool,
    /// 원래 글에서의 위치 (바이트)
    start: usize,
    end: usize,
}

struct Scan {
    toks: Vec<Tok>,
    /// MySQL /*! … */ · MariaDB /*M! … */ (실행되는 주석)이 있었는지
    exec_comment: bool,
}

fn scan(sql: &str, kind: DbKind) -> Scan {
    let b = sql.as_bytes();
    let n = b.len();
    let (mut toks, mut i, mut depth, mut exec_comment) = (Vec::new(), 0usize, 0i32, false);
    let my = kind == DbKind::Mysql;
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
        if c == b'#' && my {
            i = sql[i..].find('\n').map(|j| i + j + 1).unwrap_or(n);
            continue;
        }
        if c == b'/' && d == b'*' {
            // MySQL /*! … */ · MariaDB /*M! … */ 는 서버가 실행한다
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
        // 문자열 / 인용 식별자: 통째로 한 토큰 (식별자는 이름으로 쓰일 수 있어 단어로 남긴다)
        if c == b'\'' || c == b'"' || c == b'`' {
            // PG E'…' 는 \ 가 이스케이프. name' 처럼 낱말 끝의 e 는 아니다 (sql::split_statements 와 같은 규칙)
            let e_string = kind == DbKind::Postgres && c == b'\'' && i > 0 && (b[i - 1] == b'e' || b[i - 1] == b'E') && (i < 2 || !(b[i - 2].is_ascii_alphanumeric() || b[i - 2] == b'_' || b[i - 2] >= 0x80));
            let mut j = i + 1;
            while j < n {
                // MySQL 백틱 이름 안의 \ 는 이스케이프가 아니다
                if ((my && c != b'`') || e_string) && b[j] == b'\\' {
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
        // SQL Server [이름] (]] 는 ] 하나)
        if c == b'[' && kind == DbKind::Mssql {
            let mut j = i + 1;
            while j < n && !(b[j] == b']' && !(j + 1 < n && b[j + 1] == b']')) {
                j += if b[j] == b']' { 2 } else { 1 };
            }
            toks.push(Tok { text: sql[i + 1..j.min(n)].to_lowercase(), word: true, depth, quoted: true, start: i, end: (j + 1).min(n) });
            i = (j + 1).min(n);
            continue;
        }
        if c == b'$' && kind == DbKind::Postgres {
            let rest = &sql[i..];
            if let Some(te) = rest[1..].find('$').map(|j| j + 2) {
                let tag = &rest[..te];
                if tag[1..te - 1].chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '_') && !tag[1..].starts_with(|ch: char| ch.is_ascii_digit()) {
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

/// 단순한 UPDATE / DELETE 한 문장의 모양 (PROD 안전장치: 바뀔 행 수 세기 · 되돌리기 SQL).
/// 같은 테이블·조건으로만 바꾸는 문장만 (CTE, UPDATE … FROM, DELETE … USING, 여러 테이블, ORDER BY / LIMIT 이 있으면 None)
#[derive(Debug, Clone, PartialEq)]
pub struct SimpleDml {
    pub delete: bool,
    /// 테이블 이름 (쓴 그대로, 별칭 빼고): public."Order Items"
    pub table: String,
    /// FROM 에 쓸 글 (별칭 포함): public."Order Items" o
    pub from: String,
    /// WHERE 조건 (없으면 모든 행)
    pub cond: Option<String>,
    /// UPDATE 가 바꾸는 컬럼 (따옴표는 벗기고, 안 감싼 PostgreSQL 이름은 소문자)
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
    // 첫 문장만 (최상위 ; 까지)
    let end = sc.toks.iter().position(|x| x.text == ";" && x.depth == 0).unwrap_or(sc.toks.len());
    let t = &sc.toks[..end];
    let first = t.iter().position(|x| x.word)?;
    let is = |i: usize, w: &str| t[i].word && !t[i].quoted && t[i].depth == 0 && t[i].text == w;
    let find = |w: &str, from: usize| (from..t.len()).find(|&i| is(i, w));
    let verb = t[first].text.clone();
    let has = |w: &str| find(w, first + 1).is_some();
    // 셀 수 없는 모양: 다른 테이블과 묶거나 일부만 바꾸는 것
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
            // UPDATE … FROM (PG) · 여러 테이블 (MySQL a, b)
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
    // 이름: a.b.c (별칭 앞까지)
    let mut name_end = a;
    while name_end + 2 < b && t[name_end + 1].text == "." && t[name_end + 2].word {
        name_end += 2;
    }
    let table = sql[t[a].start..t[name_end].end].to_string();
    let from = sql[t[a].start..t[b - 1].end].to_string();
    // 조건: WHERE 다음부터 RETURNING 앞까지 (뒤 주석은 빼고)
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
    // SET a = 1, o.b = 2, (c, d) = (…): = 앞의 이름들 (점이 있으면 마지막)
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
            } else if left && x.text == "," && x.depth == 1 {
                if let Some(l) = last.take() {
                    set_cols.push(name(l));
                }
            }
            i += 1;
        }
    }
    Some(SimpleDml { delete: verb == "delete", table, from, cond, set_cols })
}

/// PROD 안전장치: UPDATE / DELETE 가 바꿀 행 수를 세는 SELECT count(*). 셀 수 없는 문장이면 None
pub fn impact_count_sql(sql: &str, kind: DbKind) -> Option<String> {
    let d = simple_dml(sql, kind)?;
    Some(match d.cond {
        Some(c) => format!("SELECT count(*) FROM {} WHERE {c}", d.from),
        None => format!("SELECT count(*) FROM {}", d.from),
    })
}

/// 읽기 전용에서 허용하는 첫 단어
const READ_WORDS: &[&str] = &["select", "with", "values", "table", "show", "explain", "describe", "desc"];
/// 읽기 전용 Data Source 의 콘솔에서는 트랜잭션 제어와 세션 설정도 허용한다
const SESSION_WORDS: &[&str] = &["begin", "start", "commit", "rollback", "end", "abort", "set", "use"];

/// 읽기 전용에서도 서버 밖에 부작용을 내는 함수 (MCP 처럼 남이 쿼리를 쓰는 경우에 막는다)
const SIDE_EFFECT_FUNCS: &[&str] = &[
    // PostgreSQL: 다른 세션 끊기·서버 파일 읽기/쓰기·설정 재적용·외부 접속·잠금
    "pg_terminate_backend", "pg_cancel_backend", "pg_reload_conf", "pg_rotate_logfile", "pg_read_file", "pg_read_binary_file",
    "pg_ls_dir", "pg_stat_file", "pg_ls_logdir", "pg_ls_waldir", "pg_ls_tmpdir", "pg_ls_archive_statusdir", "pg_file_write",
    "lo_import", "lo_export", "dblink", "dblink_exec", "dblink_connect", "dblink_send_query", "set_config", "pg_switch_wal",
    "pg_create_restore_point", "pg_promote", "pg_advisory_lock", "pg_advisory_xact_lock", "pg_try_advisory_lock", "pg_sleep",
    "pg_sleep_for", "pg_sleep_until", "pg_stat_reset", "pg_stat_reset_shared", "pg_stat_reset_single_table_counters",
    "pg_stat_reset_single_function_counters", "pg_stat_reset_slru", "pg_stat_reset_replication_slot", "pg_stat_reset_subscription_stats",
    "pg_drop_replication_slot", "pg_create_logical_replication_slot", "pg_create_physical_replication_slot", "pg_logical_emit_message",
    "pg_replication_origin_create", "pg_replication_origin_drop", "pg_log_backend_memory_contexts",
    // MySQL: 서버 파일 읽기·지연·잠금
    "load_file", "sleep", "benchmark", "get_lock", "release_lock", "release_all_locks",
];

/// SQL Server 는 ; 없이 문장을 이어 쓸 수 있다 (SELECT 1 DROP TABLE t 는 배치 하나). 읽기 문장 뒤에 이 낱말이 나오면 다른 문장이다
const MSSQL_STATEMENT_WORDS: &[&str] = &[
    "drop", "alter", "create", "exec", "execute", "truncate", "grant", "revoke", "deny", "declare", "waitfor", "backup", "restore", "kill", "use",
    "dbcc", "shutdown", "reconfigure", "bulk", "set", "begin", "commit", "rollback", "save", "checkpoint", "if", "while", "goto", "print", "raiserror", "throw",
];
/// SQL Server: 다른 서버·파일을 읽는 함수
const MSSQL_EXTERNAL_FUNCS: &[&str] = &["openrowset", "opendatasource", "openquery"];
/// ClickHouse 테이블 함수: 서버 밖 (다른 서버·파일·URL) 을 읽는다 (MCP)
const CLICKHOUSE_EXTERNAL_FUNCS: &[&str] = &[
    "url", "urlcluster", "file", "filecluster", "s3", "s3cluster", "gcs", "azureblobstorage", "azureblobstoragecluster", "hdfs", "hdfscluster", "remote",
    "remotesecure", "cluster", "clusterallreplicas", "mysql", "postgresql", "mongodb", "redis", "sqlite", "jdbc", "odbc", "executable", "iceberg", "icebergs3",
    "deltalake", "hudi", "input",
];
/// DuckDB: 파일·URL 을 읽는 함수 (MCP. 드라이버도 외부 접근을 끄고 연다 — crate::SANDBOX)
const DUCKDB_EXTERNAL_FUNCS: &[&str] = &[
    "read_text", "read_blob", "read_csv", "read_csv_auto", "read_json", "read_json_auto", "read_json_objects", "read_json_objects_auto", "read_ndjson",
    "read_ndjson_auto", "read_ndjson_objects", "read_parquet", "parquet_scan", "parquet_metadata", "parquet_schema", "parquet_file_metadata", "parquet_kv_metadata",
    "read_xlsx", "sniff_csv", "glob", "iceberg_scan", "delta_scan", "getenv", "query_table",
];

/// 읽기 전용으로 실행해도 되는지. strict = MCP (부작용 함수까지 막는다). 막으면 이유
pub fn read_only_violation(sql: &str, kind: DbKind, strict: bool) -> Option<String> {
    if kind == DbKind::Redis {
        return None;
    }
    let head = sql.trim_start().split(|c: char| c.is_whitespace() || c == '(').next().unwrap_or("").to_lowercase();
    // 탐색기가 보내는 SELECT * FROM … 은 아래 SQL 규칙으로
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
    let words: Vec<&str> = sc.toks.iter().map(|t| t.text.as_str()).collect();
    let bare = |t: &Tok| t.word && !t.quoted;
    if kind == DbKind::Mssql
        && let Some(t) = sc.toks.iter().filter(|t| bare(t)).skip(1).find(|t| MSSQL_STATEMENT_WORDS.contains(&t.text.as_str()))
    {
        return Some(format!("Read-only mode: '{}' starts another statement (SQL Server runs the whole batch)", t.text));
    }
    // SELECT … INTO 새 테이블 (PostgreSQL · SQL Server)
    if matches!(kind, DbKind::Postgres | DbKind::Mssql) && sc.toks.iter().any(|t| bare(t) && t.text == "into") {
        return Some("Read-only mode: SELECT … INTO creates a table".into());
    }
    if kind == DbKind::Postgres {
        // U&"…" 이름은 \XXXX 로 글자를 숨길 수 있다 (U&"default_transaction_read\005fonly")
        if sc.toks.windows(2).any(|w| w[0].word && !w[0].quoted && w[0].text == "u" && w[1].text == "&" && w[1].start == w[0].end) {
            return Some("Read-only mode: U&\"…\" / U&'…' escapes are not allowed".into());
        }
        // 문자열 안의 \ 해석을 바꾸면 검사와 서버가 문자열 끝을 다르게 본다
        if words.iter().any(|w| w.contains("standard_conforming_strings") || w.contains("backslash_quote")) {
            return Some("Read-only mode: changing how strings are read is not allowed".into());
        }
    }
    // 세션 설정으로 읽기 전용을 끄는 것 (SET default_transaction_read_only = off, SET SESSION TRANSACTION READ WRITE, START TRANSACTION READ WRITE)
    if words.iter().any(|w| w.contains("read_only")) || words.windows(2).any(|w| w == ["read", "write"]) {
        return Some("Read-only mode: changing the read-only setting is not allowed".into());
    }
    // SELECT … INTO OUTFILE / DUMPFILE (MySQL 서버에 파일 쓰기), SELECT … INTO new_table (PG)
    if words.windows(2).any(|w| w[0] == "into" && (w[1] == "outfile" || w[1] == "dumpfile")) {
        return Some("Read-only mode: SELECT … INTO OUTFILE/DUMPFILE is not allowed".into());
    }
    // 문장 안의 쓰기 (WITH d AS (DELETE …) SELECT …)
    if let Some(w) = dml_words(&sc.toks).next() {
        return Some(format!("Read-only mode: {} inside the statement is not allowed", sc.toks[w].text.to_uppercase()));
    }
    for (i, t) in sc.toks.iter().enumerate() {
        let call = t.word && !t.quoted && sc.toks.get(i + 1).is_some_and(|n| n.text == "(");
        // set_config 는 읽기 전용 설정을 끌 수 있어 Data Source 읽기 전용에서도 막는다. 나머지 부작용 함수는 MCP(strict)에서
        if call && (t.text == "set_config" || (strict && SIDE_EFFECT_FUNCS.contains(&t.text.as_str()))) {
            return Some(format!("Read-only mode: {}() is not allowed (it has side effects outside the transaction)", t.text));
        }
        let external = match kind {
            DbKind::Mssql => MSSQL_EXTERNAL_FUNCS.contains(&t.text.as_str()),
            DbKind::Clickhouse => strict && CLICKHOUSE_EXTERNAL_FUNCS.contains(&t.text.as_str()),
            DbKind::Duckdb => strict && DUCKDB_EXTERNAL_FUNCS.contains(&t.text.as_str()),
            _ => false,
        };
        if call && external {
            return Some(format!("Read-only mode: {}() reads files or other servers", t.text));
        }
    }
    None
}

/// 벡터 DB REST: GET 과 읽기 POST (search · query · scroll · get · count · describe · list …) 만
/// Milvus 드라이버는 메서드와 상관없이 POST 로 보내므로 경로만 본다
fn rest_read_violation(sql: &str, kind: DbKind) -> Option<String> {
    let first = sql.trim().lines().next().unwrap_or("");
    let mut parts = first.split_whitespace();
    let method = parts.next().unwrap_or("").to_uppercase();
    let raw = parts.next().unwrap_or("");
    let raw_path = raw.split('?').next().unwrap_or("");
    // 검사한 경로와 서버가 받는 경로가 달라질 수 있는 글자 (# 뒤는 보내지 않는다, %2F · .. 는 서버가 풀어 읽는다)
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

/// JSON5 이스케이프 (\uXXXX · \xXX · \n …, 줄 이어 쓰기) 를 푼 글. 따옴표 밖의 이름에 쓴 \uXXXX 도 푼다
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

/// MongoDB 셸: 한 줄짜리 읽기 (find · aggregate (\$out/\$merge 없이) · count …) 만
fn mongo_read_violation(sql: &str) -> Option<String> {
    let t = sql.trim().trim_end_matches(';');
    const READS: &[&str] = &["find", "findone", "aggregate", "countdocuments", "estimateddocumentcount", "count", "distinct", "getindexes", "stats"];
    // db.<coll>.<method>( …  — 컬렉션 이름에 점이 있을 수 있어 마지막 '(' 앞 낱말을 본다
    let call = t.strip_prefix("db.").and_then(|r| r.split_once('(')).map(|(head, _)| head.rsplit('.').next().unwrap_or("").to_lowercase());
    // 드라이버는 JSON5 로 읽는다: "\u0024out" · '\x24merge' 도 $out · $merge 다. 이스케이프를 풀어 본다
    let plain = json5_unescape(t);
    let writes_stage = ["$out", "$merge", "$function", "$accumulator"].iter().any(|k| plain.contains(k));
    // 이어진 호출 (.sort().limit()) 은 되지만 문장이 여럿이면 안 된다
    let one = !t.contains(';');
    match call {
        Some(m) if READS.contains(&m.as_str()) && !writes_stage && one => None,
        _ => Some("Read-only mode: only find / aggregate / count / distinct queries run".into()),
    }
}

/// 쓰기 동사 (INSERT/UPDATE/DELETE/MERGE) 의 위치. 따옴표 이름과 쓰기가 아닌 쓰임
/// (SELECT … FOR [NO KEY] UPDATE, ON CONFLICT DO UPDATE, ON DUPLICATE KEY UPDATE) 은 뺀다
fn dml_words(toks: &[Tok]) -> impl Iterator<Item = usize> + '_ {
    toks.iter().enumerate().filter_map(move |(i, t)| {
        let dml = t.word && !t.quoted && ["insert", "update", "delete", "merge"].contains(&t.text.as_str());
        let prev = i.checked_sub(1).and_then(|p| toks.get(p)).map(|p| p.text.as_str()).unwrap_or("");
        (dml && !(t.text == "update" && ["for", "key", "do"].contains(&prev))).then_some(i)
    })
}

/// Redis 읽기 전용(MCP) 에서 더 막는 읽기 명령: 설정·비밀번호가 보이거나(CONFIG GET) 서버를 멈출 수 있는 것(KEYS)
pub fn redis_strict_violation(cmd: &str) -> Option<String> {
    match cmd {
        "CONFIG" => Some("CONFIG is not allowed in read-only mode (it can reveal passwords)".into()),
        "KEYS" => Some("KEYS is not allowed in read-only mode (it blocks the server on large databases) — use redis_scan".into()),
        _ => None,
    }
}

/// 실행 전에 확인을 받을 위험한 문장이면 이유: WHERE 없는 UPDATE/DELETE (CTE 안 포함), DROP, TRUNCATE.
/// EXPLAIN ANALYZE 는 실제로 실행하므로 그 뒤 문장을 본다 (그냥 EXPLAIN 은 실행하지 않는다)
pub fn unsafe_statement(sql: &str, kind: DbKind) -> Option<String> {
    if kind == DbKind::Redis {
        let cmd = sql.split_whitespace().next().unwrap_or("").to_uppercase();
        return ["FLUSHALL", "FLUSHDB", "DEL", "UNLINK", "SHUTDOWN"].contains(&cmd.as_str()).then(|| format!("{cmd} deletes data"));
    }
    let sc = scan(sql, kind);
    let mut toks: &[Tok] = &sc.toks;
    if toks.first().is_some_and(|t| t.word && t.text == "explain") {
        // 옵션 (ANALYZE, (ANALYZE, BUFFERS), FORMAT …) 을 지나 첫 동사까지
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
        // 트리거·권한 정의 안의 UPDATE/DELETE 는 쓰기가 아니다
        "create" | "alter" | "grant" | "revoke" | "comment" => return None,
        _ => {}
    }
    // UPDATE/DELETE 마다: 같은 괄호 깊이 안에 WHERE 가 있는지 (최상위는 문장 끝까지, CTE 는 그 괄호 끝까지)
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

/// 데이터를 바꾸지 않는 문장인지 (스크립트 실패 안내에서 "이미 커밋된 변경" 을 셀 때)
pub fn is_read(sql: &str, kind: DbKind) -> bool {
    let sc = scan(sql, kind);
    let first = sc.toks.iter().find(|t| t.word).map(|t| t.text.as_str()).unwrap_or("");
    READ_WORDS.contains(&first) && !sc.toks.iter().any(|t| t.word && ["insert", "update", "delete", "merge"].contains(&t.text.as_str()))
}

/// 구조를 바꾸는 문장인지 (실행한 뒤 탐색기 트리를 다시 읽는다). 앞 주석은 건너뛴다
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

    /// PROD 안전장치: 바꿀 행 수를 세는 쿼리. 단순한 UPDATE / DELETE 만 (나머지는 None)
    #[test]
    fn impact_count_queries() {
        let pg = |q: &str| impact_count_sql(q, DbKind::Postgres);
        assert_eq!(pg("UPDATE orders SET status = 'paid' WHERE id = 42;").as_deref(), Some("SELECT count(*) FROM orders WHERE id = 42"));
        assert_eq!(pg("update public.\"Order Items\" o set qty = qty + 1 where o.qty < 0 returning *").as_deref(), Some("SELECT count(*) FROM public.\"Order Items\" o WHERE o.qty < 0"));
        assert_eq!(pg("DELETE FROM logs.user_activity_log WHERE ts < now() - interval '30 days'").as_deref(), Some("SELECT count(*) FROM logs.user_activity_log WHERE ts < now() - interval '30 days'"));
        assert_eq!(pg("delete from t").as_deref(), Some("SELECT count(*) FROM t"));
        assert_eq!(pg("delete from t returning id").as_deref(), Some("SELECT count(*) FROM t"));
        // 조건 안의 괄호·문자열 속 키워드는 그대로
        assert_eq!(pg("update t set a = 'where' where b in (select x from y where z) -- c").as_deref(), Some("SELECT count(*) FROM t WHERE b in (select x from y where z)"));
        // 셀 수 없는 것
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

    /// 되돌리기 SQL 에 쓰는 모양: 테이블 이름 (별칭 빼고) · 바꾸는 컬럼
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
        // 실제로 뚫렸던 MySQL 형태
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
        // 문자열·주석 안의 단어는 상관없다
        assert!(!bad("select 'pg_terminate_backend(1); delete'", Postgres));
        assert!(!bad("select $$; delete from t; $$", Postgres));
        assert!(!bad("select 1 -- x; delete", Mysql));
        assert!(!bad("with a as (select 1) select * from a", Postgres));
        assert!(!bad("explain select 1", Postgres));
        assert!(!bad("show tables", Mysql));
    }

    /// 보안 점검에서 나온 우회 (2026-10)
    #[test]
    fn read_only_blocks_review_findings() {
        let bad = |s: &str, k| read_only_violation(s, k, true).is_some();
        // MariaDB 실행 주석 /*M! … */ 도 /*! … */ 처럼
        assert!(bad("select 1 /*M!100000 into outfile '/tmp/x' */", Mysql));
        assert!(bad("/*M! commit; delete from t */ select 1", Mysql));
        // MySQL 백틱 안의 \ 는 이스케이프가 아니다: 서버는 문장 둘로 본다
        assert!(bad("select 1 as `a\\`, 1 as `b`; delete from t; -- `", Mysql));
        // CTE 안의 쓰기 (첫 단어만 보면 with)
        assert!(bad("with d as (delete from t returning *) select * from d", Postgres));
        // 읽기 전용 Data Source(엄격하지 않음)도 set_config 로 읽기 전용을 끄는 것은 막는다
        let ds = |s: &str| read_only_violation(s, Postgres, false).is_some();
        assert!(ds("select set_config('default_transaction_read_only','off',false)"));
        assert!(ds("select pg_catalog.set_config('transaction_read_only', 'off', true)"));
        assert!(ds("with d as (update t set a = 1 returning *) select * from d"));
        // 읽기 전용 트랜잭션에서도 되는 관리 함수 (MCP)
        assert!(bad("select pg_stat_reset()", Postgres));
        assert!(bad("select pg_drop_replication_slot('s')", Postgres));
        assert!(bad("select pg_logical_emit_message(true, 'p', 'x')", Postgres));
        // 읽기는 그대로
        assert!(!ds("select * from t for update"));
        assert!(!ds("select * from t for no key update"));
        assert!(!ds("select \"delete\" from t"));
        assert!(!bad("select `update` from t", Mysql));
    }

    /// 드라이버 DB 와 렉서 경계 (2026-10 점검 2차)
    #[test]
    fn read_only_plugin_and_lexer_edges() {
        use DbKind::{Clickhouse, Duckdb, Mssql};
        let bad = |s: &str, k| read_only_violation(s, k, true).is_some();
        let ds = |s: &str, k| read_only_violation(s, k, false).is_some();
        // SQL Server: ; 없이 이어 쓴 문장 · SELECT INTO · 외부 읽기
        assert!(bad("SELECT 1 DROP TABLE dbo.orders", Mssql));
        assert!(ds("SELECT 1 EXEC xp_cmdshell 'whoami'", Mssql));
        assert!(ds("select 1\nexec('drop table t')", Mssql));
        assert!(ds("SELECT * INTO dbo.x FROM dbo.y", Mssql));
        assert!(ds("SELECT * FROM OPENROWSET('SQLNCLI', 'Server=x;', 'select 1')", Mssql));
        assert!(!ds("SELECT name, create_date FROM sys.tables WHERE name <> 'drop'", Mssql));
        assert!(!ds("SELECT [drop], [use] FROM dbo.t", Mssql));
        assert!(!ds("SELECT TOP 10 * FROM dbo.t ORDER BY id", Mssql));
        // REST: Milvus 는 메서드와 상관없이 POST, 경로에 # · % · ..
        assert!(bad("GET /collections/drop\n{\"collectionName\": \"prod\"}", Milvus));
        assert!(bad("GET /entities/delete\n{}", Milvus));
        assert!(!bad("POST /collections/list\n{}", Milvus));
        assert!(!bad("POST /entities/search\n{}", Milvus));
        assert!(bad("POST /collections/prod/points/delete#/search\n{}", Qdrant));
        assert!(bad("POST /api/v1/collections/x/delete#/query\n{}", Chroma));
        assert!(bad("POST /collections/x/points/delete%2F..%2Fsearch\n{}", Qdrant));
        assert!(bad("POST /collections/x/points/delete/../search\n{}", Qdrant));
        assert!(!bad("POST /collections/x/points/search?timeout=5 {\"limit\": 3}", Qdrant));
        // MongoDB: JSON5 이스케이프로 숨긴 $out / $merge
        assert!(bad("db.users.aggregate([{\"\\u0024out\": \"users\"}])", Mongodb));
        assert!(bad("db.users.aggregate([{'\\x24merge': {into: 'x'}}])", Mongodb));
        assert!(bad("db.users.aggregate([{$o\\u0075t: 'users'}])", Mongodb));
        assert!(bad("db.users.aggregate([{$facet: {a: [{$merge: 'x'}]}}])", Mongodb));
        assert!(bad("db.users.find({$expr: {$function: {body: 'x', args: [], lang: 'js'}}})", Mongodb));
        assert!(!bad("db.users.find({name: {$regex: '\\\\d+'}})", Mongodb));
        // PostgreSQL: 낱말 끝의 e' 는 E-string 이 아니다 (뒤 토큰이 숨지 않게)
        assert!(bad("select name'\\', pg_terminate_backend(pid) from pg_stat_activity --'", Postgres));
        assert!(ds("select name'\\', set_config('default_transaction_read_only','off',false) --'", Postgres));
        assert!(bad("select é'\\'; delete from t; --'", Postgres));
        assert!(!bad("select E'it\\'s', 1", Postgres));
        // PostgreSQL: U& 이름 · 문자열 해석 바꾸기 · SELECT INTO
        assert!(ds("SET U&\"default_transaction_read\\005fonly\" = off", Postgres));
        assert!(ds("set standard_conforming_strings = off", Postgres));
        assert!(ds("select * into t2 from t", Postgres));
        assert!(!ds("select 'into', \"into\" from t", Postgres));
        // DuckDB · ClickHouse (MCP): 파일 · 다른 서버 읽기
        assert!(bad("SELECT * FROM read_text('/Users/x/.ssh/id_rsa')", Duckdb));
        assert!(bad("select getenv('HOME')", Duckdb));
        assert!(bad("SELECT * FROM url('http://169.254.169.254/', 'RawBLOB', 'x String')", Clickhouse));
        assert!(bad("select * from file('/etc/passwd', 'LineAsString')", Clickhouse));
        assert!(!bad("SELECT count() FROM system.tables", Clickhouse));
        assert!(!ds("SELECT * FROM read_csv('local.csv')", Duckdb), "the user's own read-only data source may read local files");
    }

    #[test]
    fn read_only_data_source_allows_session_statements() {
        let ok = |s: &str| read_only_violation(s, Postgres, false).is_none();
        assert!(ok("begin") && ok("rollback") && ok("set search_path = app"));
        assert!(!ok("set default_transaction_read_only = off"));
        assert!(!ok("start transaction read write"));
        assert!(!ok("update t set a = 1"));
        // 데이터 소스 읽기 전용은 pg_sleep 같은 건 막지 않는다 (MCP 만)
        assert!(ok("select pg_sleep(0)"));
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
        // EXPLAIN ANALYZE 는 실제로 실행한다. 그냥 EXPLAIN 은 실행하지 않는다
        assert!(u("explain analyze delete from t").is_some());
        assert!(u("explain (analyze, buffers) update t set a = 1").is_some());
        assert!(u("explain delete from t").is_none());
        assert!(unsafe_statement("explain analyze delete from t", Mysql).is_some());
        // CTE 안의 WHERE 없는 쓰기
        assert!(u("with d as (delete from t returning *) select * from d").is_some());
        assert!(u("with d as (delete from t where id = 1 returning *) select * from d").is_none());
        // 쓰기가 아닌 update 단어
        assert!(u("insert into t values (1) on conflict (id) do update set a = 1").is_none());
        assert!(u("create trigger tr before update on t for each row execute function f()").is_none());
        assert!(u("grant update on t to u").is_none());
        assert!(u("select * from t for update").is_none());
        assert!(unsafe_statement("FLUSHALL", DbKind::Redis).is_some());
        assert!(unsafe_statement("GET a", DbKind::Redis).is_none());
    }
}
