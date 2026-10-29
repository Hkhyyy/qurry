//! DB 종류별 SQL 유틸 (TUI·MCP 공용). 프론트엔드 lib/api.ts 의 Dialect 와 같은 규칙.
use crate::store::{ConnectionConfig, DbKind};

/// 접속 대상 id: "<data source id>::<database>"
pub fn target_id(conn: &str, db: Option<&str>) -> String {
    match db.filter(|d| !d.is_empty()) {
        Some(d) => format!("{conn}::{d}"),
        None => conn.to_string(),
    }
}

/// 트리·콘솔의 기본 DB 이름
pub fn default_db(cfg: &ConnectionConfig) -> String {
    match cfg.kind {
        DbKind::Postgres => if cfg.database.trim().is_empty() { "postgres".into() } else { cfg.database.trim().into() },
        DbKind::Mysql => cfg.database.trim().into(),
        DbKind::Redis => format!("db{}", crate::redisdb::db_index(&cfg.database)),
        // 파일 DB 는 main, 나머지 드라이버는 정한 DB 이름 그대로
        k if k.is_file() => "main".into(),
        // 벡터 DB: 비면 서버의 기본 이름
        DbKind::Qdrant if cfg.database.trim().is_empty() => "qdrant".into(),
        DbKind::Chroma if cfg.database.trim().is_empty() => "default_database".into(),
        DbKind::Milvus if cfg.database.trim().is_empty() => "default".into(),
        _ => cfg.database.trim().into(),
    }
}

/// 따옴표 없이 쓰면 문법이 깨지는 예약어 (PG·MySQL 합집합. 불필요하게 따옴표를 쳐도 해는 없다)
const RESERVED: &[&str] = &[
    "all", "analyse", "analyze", "and", "any", "array", "as", "asc", "asymmetric", "between", "both", "by", "case", "cast", "change", "check",
    "collate", "column", "condition", "constraint", "create", "cross", "current_date", "current_role", "current_time", "current_timestamp",
    "current_user", "database", "default", "deferrable", "delete", "desc", "distinct", "do", "drop", "else", "end", "except", "exists", "false",
    "fetch", "for", "foreign", "from", "full", "grant", "group", "having", "in", "index", "initially", "inner", "insert", "interval", "intersect",
    "into", "is", "join", "key", "keys", "lateral", "leading", "left", "like", "limit", "localtime", "localtimestamp", "match", "natural", "not",
    "null", "offset", "on", "only", "or", "order", "outer", "placing", "primary", "range", "rank", "read", "references", "returning", "right",
    "row", "rows", "select", "session_user", "set", "some", "symmetric", "table", "then", "to", "trailing", "true", "union", "unique", "update",
    "usage", "user", "using", "values", "variadic", "when", "where", "window", "with", "write",
];

pub fn quote_ident(kind: DbKind, s: &str) -> String {
    let reserved = RESERVED.contains(&s.to_lowercase().as_str());
    let simple = |lower_only: bool| {
        let mut ch = s.chars();
        !reserved
            && matches!(ch.next(), Some(c) if c == '_' || if lower_only { c.is_ascii_lowercase() } else { c.is_ascii_alphabetic() })
            && s.chars().all(|c| c == '_' || c.is_ascii_digit() || if lower_only { c.is_ascii_lowercase() } else { c.is_ascii_alphabetic() })
    };
    match kind {
        DbKind::Mysql => if simple(false) { s.into() } else { format!("`{}`", s.replace('`', "``")) },
        DbKind::Redis => s.into(),
        DbKind::Postgres => if simple(true) { s.into() } else { format!("\"{}\"", s.replace('"', "\"\"")) },
        // 드라이버: 표준 따옴표 (SQL Server·Oracle·ClickHouse·SQLite·DuckDB 모두 "…" 를 받는다). 대소문자를 바꾸지 않게 소문자만 그대로
        DbKind::Mongodb | DbKind::Qdrant | DbKind::Chroma | DbKind::Milvus => s.into(),
        _ => if simple(true) { s.into() } else { format!("\"{}\"", s.replace('"', "\"\"")) },
    }
}

/// pgvector 열인지 (vector(3) · halfvec · sparsevec)
pub fn is_vector_type(t: &str) -> bool {
    let t = t.trim().to_lowercase();
    ["vector", "halfvec", "sparsevec"].iter().any(|v| t == *v || t.starts_with(&format!("{v}(")) || t.ends_with(&format!(".{v}")))
}

/// 이 열의 벡터 인덱스가 쓰는 거리 연산자 (인덱스를 타게). 없으면 L2 (<->)
pub fn vector_operator(column: &str, index_defs: &[&str]) -> &'static str {
    let col = column.to_lowercase();
    for d in index_defs {
        let d = d.to_lowercase();
        if !(d.contains("using hnsw") || d.contains("using ivfflat")) || !(d.contains(&format!("({col} ")) || d.contains(&format!("(\"{col}\" "))) {
            continue;
        }
        for (ops, op) in [("_cosine_ops", "<=>"), ("_ip_ops", "<#>"), ("_l1_ops", "<+>"), ("_l2_ops", "<->")] {
            if d.contains(ops) {
                return op;
            }
        }
    }
    "<->"
}

/// pgvector: 이 행과 가까운 행 (자신은 빼고 거리 순). pk = (열, 값)
pub fn similar_rows_sql(schema: &str, table: &str, column: &str, pk: &[(String, Option<String>)], op: &str, limit: usize) -> String {
    let q = |s: &str| quote_ident(DbKind::Postgres, s);
    let t = format!("{}.{}", q(schema), q(table));
    let key = pk.iter().map(|(c, v)| format!("{} = {}", q(c), literal(DbKind::Postgres, v.as_deref()))).collect::<Vec<_>>().join(" AND ");
    let col = q(column);
    format!("SELECT {col} {op} (SELECT {col} FROM {t} WHERE {key}) AS distance, *\nFROM {t}\nWHERE NOT ({key})\nORDER BY distance\nLIMIT {limit}")
}

pub fn literal(kind: DbKind, v: Option<&str>) -> String {
    match v {
        None => "NULL".into(),
        Some(v) if kind == DbKind::Mysql => format!("'{}'", v.replace('\\', "\\\\").replace('\'', "''")),
        Some(v) => format!("'{}'", v.replace('\'', "''")),
    }
}

/// 세미콜론으로 문장을 나눈다 (문자열·주석·PG 달러 인용 건너뜀). Redis 는 한 줄 = 명령. (start, end) 바이트 범위.
/// 서버와 같은 규칙으로 읽어야 한다 (읽기 전용 검사가 이걸로 문장 수를 센다):
/// MySQL 은 `--` 뒤에 공백이 있어야 주석이고 `/*! … */` 는 실행되는 코드, PG 는 주석이 중첩되고 E'…' 안에서 \ 가 이스케이프
/// 문자열 안의 \ 가 이스케이프인 DB (MySQL · ClickHouse). guard::scan 도 같은 규칙
pub(crate) fn backslash_escapes(kind: DbKind) -> bool {
    matches!(kind, DbKind::Mysql | DbKind::Clickhouse)
}

/// # 로 시작하는 줄 주석인지: MySQL 은 늘, ClickHouse 는 "# " · "#!" (next = # 다음 바이트, 끝이면 0)
pub(crate) fn hash_comment(kind: DbKind, next: u8) -> bool {
    kind == DbKind::Mysql || (kind == DbKind::Clickhouse && matches!(next, b' ' | b'!' | b'\n' | b'\t' | 0))
}

/// PostgreSQL 달러 인용 태그 ($$ · $tag$): 태그는 이름 글자 (비ASCII 포함), 숫자로 시작하지 않는다
pub(crate) fn is_dollar_tag(tag: &str) -> bool {
    let inner = &tag[1..tag.len() - 1];
    inner.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || !ch.is_ascii()) && !inner.starts_with(|ch: char| ch.is_ascii_digit())
}

/// PG E'…' 인지: ' 바로 앞이 e/E 이고 그 앞은 이름 글자가 아니다 (name' 은 아니다)
pub(crate) fn is_e_string(b: &[u8], i: usize) -> bool {
    i > 0 && (b[i - 1] == b'e' || b[i - 1] == b'E') && (i < 2 || !(b[i - 2].is_ascii_alphanumeric() || b[i - 2] == b'_' || b[i - 2] >= 0x80))
}

pub fn split_statements(text: &str, kind: DbKind) -> Vec<(usize, usize)> {
    if kind == DbKind::Redis {
        let mut out = Vec::new();
        let mut pos = 0;
        for line in text.split('\n') {
            let t = line.trim();
            if !t.is_empty() && !t.starts_with('#') && !t.starts_with("//") {
                out.push((pos, pos + line.len()));
            }
            pos += line.len() + 1;
        }
        return out;
    }
    if kind.is_vector() {
        return split_rest(text);
    }
    let b = text.as_bytes();
    let n = b.len();
    let backslash = backslash_escapes(kind);
    let (mut out, mut start, mut i) = (Vec::new(), 0usize, 0usize);
    while i < n {
        let c = b[i];
        let d = if i + 1 < n { b[i + 1] } else { 0 };
        if c == b'-' && d == b'-' && (kind != DbKind::Mysql || i + 2 >= n || b[i + 2].is_ascii_whitespace() || b[i + 2].is_ascii_control()) {
            i = text[i..].find('\n').map(|j| i + j + 1).unwrap_or(n);
            continue;
        }
        if c == b'#' && hash_comment(kind, d) {
            i = text[i..].find('\n').map(|j| i + j + 1).unwrap_or(n);
            continue;
        }
        if c == b'/' && d == b'*' {
            // MySQL /*! … */ 는 주석이 아니라 실행되는 코드: 안쪽도 그대로 읽는다
            if kind == DbKind::Mysql && i + 2 < n && b[i + 2] == b'!' {
                i += 3;
                continue;
            }
            // MariaDB /*M! … */ 도 실행되는 코드
            if kind == DbKind::Mysql && i + 3 < n && b[i + 2] == b'M' && b[i + 3] == b'!' {
                i += 4;
                continue;
            }
            // PG 주석은 중첩된다
            let (mut depth, mut j) = (1, i + 2);
            while j < n && depth > 0 {
                if b[j] == b'/' && j + 1 < n && b[j + 1] == b'*' && kind == DbKind::Postgres {
                    depth += 1;
                    j += 2;
                } else if b[j] == b'*' && j + 1 < n && b[j + 1] == b'/' {
                    depth -= 1;
                    j += 2;
                } else {
                    j += 1;
                }
            }
            i = j;
            continue;
        }
        if c == b'\'' || c == b'"' || c == b'`' {
            // PG E'…' 문자열은 \ 가 이스케이프
            let e_string = kind == DbKind::Postgres && c == b'\'' && is_e_string(b, i);
            let mut j = i + 1;
            while j < n {
                // MySQL 백틱 이름 안의 \ 는 이스케이프가 아니다
                if ((backslash && c != b'`') || e_string) && b[j] == b'\\' {
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
            i = (j + 1).min(n);
            continue;
        }
        if c == b'$' && kind == DbKind::Postgres {
            let rest = &text[i..];
            let tag_end = rest[1..].find('$').map(|j| j + 2);
            if let Some(te) = tag_end {
                let tag = &rest[..te];
                if is_dollar_tag(tag) {
                    i = text[i + te..].find(tag).map(|j| i + te + j + te).unwrap_or(n);
                    continue;
                }
            }
        }
        if c == b';' {
            out.push((start, i));
            start = i + 1;
        }
        i += 1;
    }
    out.push((start, n));
    out.into_iter().filter(|(s, e)| !text[*s..*e].trim().is_empty()).collect()
}

/// REST 요청: "GET /x" 처럼 메서드로 시작하는 줄부터 다음 요청 전까지 (본문 JSON 포함). # · // 줄은 주석
fn split_rest(text: &str) -> Vec<(usize, usize)> {
    let is_head = |l: &str| {
        let w = l.trim_start().split_whitespace().next().unwrap_or("").to_uppercase();
        ["GET", "POST", "PUT", "PATCH", "DELETE", "SELECT"].contains(&w.as_str())
    };
    let mut out: Vec<(usize, usize)> = Vec::new();
    let (mut pos, mut cur): (usize, Option<(usize, usize)>) = (0, None);
    for line in text.split('\n') {
        let t = line.trim();
        if is_head(line) {
            out.extend(cur.take());
            cur = Some((pos, pos + line.len()));
        } else if !t.is_empty() && !t.starts_with('#') && !t.starts_with("//") {
            match &mut cur {
                Some(c) => c.1 = pos + line.len(),
                None => cur = Some((pos, pos + line.len())),
            }
        }
        pos += line.len() + 1;
    }
    out.extend(cur);
    out
}

/// 편집기용 문장 나누기: 세미콜론으로 나눈 뒤, `;` 없이 이어 쓴 문장도 줄 머리의 문장 키워드로 나눈다 (DataGrip 처럼).
/// `select … where x = 3` 다음 줄 `select * from users` → 두 문장. UNION·INSERT … SELECT·WITH … SELECT·GRANT SELECT 등은 한 문장.
/// 실행할 문장을 고를 때만 쓴다 (읽기 전용 검사는 서버와 같은 split_statements 로)
pub fn split_script(text: &str, kind: DbKind) -> Vec<(usize, usize)> {
    let segs = split_statements(text, kind);
    if kind == DbKind::Redis || kind.is_vector() {
        return segs;
    }
    let mut out = Vec::new();
    for (s, e) in segs {
        let mut start = s;
        for cut in script_cuts(text, s, e, kind) {
            out.push((start, cut));
            start = cut;
        }
        out.push((start, e));
    }
    out.into_iter().filter(|(s, e)| !text[*s..*e].trim().is_empty()).collect()
}

/// 줄 머리에 오면 새 문장일 수 있는 키워드 (SET·DO·VALUES 처럼 문장 안 줄 머리에 흔한 것은 뺀다)
const SCRIPT_STARTERS: &[&str] = &["SELECT", "INSERT", "UPDATE", "DELETE", "WITH", "CREATE", "ALTER", "DROP", "TRUNCATE", "GRANT", "REVOKE", "EXPLAIN", "SHOW", "MERGE", "CALL"];
/// 문장 머리로 오는 키워드 (이게 아니면 앞은 SQL 이 아닌 메모로 본다)
const SQL_HEADS: &[&str] = &[
    "SELECT", "INSERT", "UPDATE", "DELETE", "WITH", "CREATE", "ALTER", "DROP", "TRUNCATE", "GRANT", "REVOKE", "EXPLAIN", "SHOW", "MERGE", "CALL", "BEGIN", "START", "COMMIT", "ROLLBACK", "END",
    "ABORT", "SAVEPOINT", "RELEASE", "SET", "RESET", "VALUES", "TABLE", "DO", "COPY", "VACUUM", "ANALYZE", "REFRESH", "LOCK", "REINDEX", "CLUSTER", "COMMENT", "DECLARE", "FETCH", "CLOSE",
    "PREPARE", "EXECUTE", "DEALLOCATE", "LISTEN", "NOTIFY", "UNLISTEN", "USE", "DESC", "DESCRIBE", "REPLACE", "LOAD", "OPTIMIZE", "RENAME", "IMPORT", "DISCARD", "CHECKPOINT", "SECURITY",
];
/// 이 낱말 뒤의 줄은 앞 문장이 이어진다
const SCRIPT_CONT: &[&str] = &[
    "UNION", "ALL", "INTERSECT", "EXCEPT", "MINUS", "DISTINCT", "AS", "DO", "THEN", "ELSE", "WHEN", "AND", "OR", "NOT", "IN", "EXISTS", "ANY", "SOME", "BEFORE", "AFTER", "OF", "ON", "FOR", "KEY",
    "INSTEAD", "BY", "FROM", "JOIN", "WHERE", "TABLE", "INTO", "RETURNING", "GRANT", "REVOKE", "EXPLAIN", "ANALYZE", "VERBOSE", "IS", "LIKE", "BETWEEN", "WITH",
];

/// split_script: [s, e) 안에서 새 문장이 시작하는 줄의 머리 위치들
fn script_cuts(text: &str, s: usize, e: usize, kind: DbKind) -> Vec<usize> {
    let b = text.as_bytes();
    let mut cuts = Vec::new();
    let (mut i, mut depth, mut line_start, mut line_has_code) = (s, 0i32, s, false);
    // 지금 문장의 첫 낱말, 바로 앞 토큰 (낱말이면 대문자, 기호면 그 글자 + prev_punct, 문자열이면 '')
    let (mut head, mut prev, mut with_main, mut prev_punct) = (String::new(), String::new(), false, false);
    while i < e {
        let c = b[i];
        let d = if i + 1 < e { b[i + 1] } else { 0 };
        if c == b'\n' {
            line_start = i + 1;
            line_has_code = false;
            i += 1;
            continue;
        }
        if c.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        // 주석 (줄 끝 / 블록) — split_statements 와 같은 규칙
        if (c == b'-' && d == b'-' && (kind != DbKind::Mysql || i + 2 >= e || b[i + 2].is_ascii_whitespace())) || (c == b'#' && hash_comment(kind, d)) {
            i = text[i..e].find('\n').map(|j| i + j).unwrap_or(e);
            continue;
        }
        if c == b'/' && d == b'*' && !(kind == DbKind::Mysql && i + 2 < e && (b[i + 2] == b'!' || (b[i + 2] == b'M' && i + 3 < e && b[i + 3] == b'!'))) {
            let (mut dep, mut j) = (1, i + 2);
            while j < e && dep > 0 {
                if b[j] == b'/' && j + 1 < e && b[j + 1] == b'*' && kind == DbKind::Postgres {
                    dep += 1;
                    j += 2;
                } else if b[j] == b'*' && j + 1 < e && b[j + 1] == b'/' {
                    dep -= 1;
                    j += 2;
                } else {
                    j += 1;
                }
            }
            // 블록 주석이 줄을 넘었으면 그 뒤는 새 줄의 머리가 아니다
            if text[i..j].contains('\n') {
                line_has_code = true;
            }
            i = j;
            continue;
        }
        let first_on_line = !line_has_code;
        line_has_code = true;
        // 문자열 · 따옴표 이름 · 달러 인용 = 값 하나
        if c == b'\'' || c == b'"' || c == b'`' {
            let e_string = kind == DbKind::Postgres && c == b'\'' && is_e_string(b, i);
            let mut j = i + 1;
            while j < e {
                if ((backslash_escapes(kind) && c != b'`') || e_string) && b[j] == b'\\' {
                    j += 2;
                    continue;
                }
                if b[j] == c {
                    if j + 1 < e && b[j + 1] == c {
                        j += 2;
                        continue;
                    }
                    break;
                }
                j += 1;
            }
            i = (j + 1).min(e);
            prev = "''".into();
            prev_punct = false;
            continue;
        }
        if c == b'$' && kind == DbKind::Postgres {
            let rest = &text[i..e];
            if let Some(te) = rest[1..].find('$').map(|j| j + 2) {
                let tag = &rest[..te];
                if is_dollar_tag(tag) {
                    i = text[i + te..e].find(tag).map(|j| i + te + j + te).unwrap_or(e);
                    prev = "$$".into();
                    prev_punct = false;
                    continue;
                }
            }
        }
        if c.is_ascii_alphanumeric() || c == b'_' || c >= 0x80 {
            let mut j = i;
            while j < e && (b[j].is_ascii_alphanumeric() || b[j] == b'_' || b[j] == b'$' || b[j] >= 0x80) {
                j += 1;
            }
            let w = text[i..j].to_ascii_uppercase();
            if head.is_empty() {
                head = w.clone();
            } else if depth == 0 {
                let starter = SCRIPT_STARTERS.contains(&w.as_str());
                // WITH … 뒤 첫 SELECT/INSERT/UPDATE/DELETE 는 그 문장의 본문
                let with_body = head == "WITH" && !with_main && matches!(w.as_str(), "SELECT" | "INSERT" | "UPDATE" | "DELETE" | "MERGE");
                if with_body {
                    with_main = true;
                } else if starter && first_on_line && !SQL_HEADS.contains(&head.as_str()) {
                    // 앞은 SQL 이 아닌 제목·메모 줄: 여기서 문장이 시작한다 (메모는 실행할 때 따로 뺀다)
                    head = w.clone();
                } else if starter && first_on_line && !SCRIPT_CONT.contains(&prev.as_str()) && !(prev_punct && prev != ")") {
                    let split = match head.as_str() {
                        "CREATE" | "ALTER" | "EXPLAIN" => false,
                        "INSERT" | "REPLACE" => !matches!(w.as_str(), "SELECT" | "WITH"),
                        "GRANT" | "REVOKE" => matches!(w.as_str(), "GRANT" | "REVOKE"),
                        _ => true,
                    };
                    if split {
                        cuts.push(line_start);
                        head = w.clone();
                        with_main = false;
                    }
                }
            }
            prev = w;
            prev_punct = false;
            i = j;
            continue;
        }
        match c {
            b'(' => depth += 1,
            b')' => depth -= 1,
            _ => {}
        }
        prev = (c as char).to_string();
        prev_punct = true;
        i += 1;
    }
    cuts
}

/// 캐럿 위치(바이트)의 문장. 문장 사이면 바로 앞 문장
pub fn statement_at(text: &str, offset: usize, kind: DbKind) -> Option<String> {
    let segs = split_script(text, kind);
    let pick = segs.iter().find(|(s, e)| offset >= *s && offset <= e + 1).or_else(|| segs.iter().rev().find(|(_, e)| *e < offset)).or(segs.first())?;
    Some(text[pick.0..pick.1].trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn split_and_quote() {
        let t = "select ';' as a; -- x;\nselect $$a;b$$; select \"x;y\"";
        let s: Vec<_> = split_statements(t, DbKind::Postgres).into_iter().map(|(a, b)| t[a..b].trim().to_string()).collect();
        assert_eq!(s, ["select ';' as a", "-- x;\nselect $$a;b$$", "select \"x;y\""]);
        assert_eq!(statement_at(t, 3, DbKind::Postgres).unwrap(), "select ';' as a");
        let m = "select 'a\\';b'; select 2";
        assert_eq!(split_statements(m, DbKind::Mysql).len(), 2);
        assert_eq!(split_statements("GET a\n# c\n\nSET b 1", DbKind::Redis).len(), 2);
        assert_eq!(quote_ident(DbKind::Postgres, "User"), "\"User\"");
        assert_eq!(quote_ident(DbKind::Mysql, "my col"), "`my col`");
        assert_eq!(quote_ident(DbKind::Postgres, "user"), "\"user\"");
        assert_eq!(quote_ident(DbKind::Mysql, "order"), "`order`");
        assert_eq!(quote_ident(DbKind::Postgres, "users"), "users");
        assert_eq!(literal(DbKind::Mysql, Some("a\\'b")), "'a\\\\''b'");
        assert_eq!(literal(DbKind::Postgres, None), "NULL");
    }

    /// `;` 없이 줄을 바꿔 쓴 문장 (DataGrip 처럼 나눈다)
    #[test]
    fn pgvector_similar_rows() {
        assert!(is_vector_type("vector(1536)") && is_vector_type("halfvec") && is_vector_type("public.vector") && !is_vector_type("text") && !is_vector_type("vectors"));
        let defs = ["CREATE UNIQUE INDEX items_pkey ON public.items USING btree (id)", "CREATE INDEX items_embedding_idx ON public.items USING hnsw (embedding vector_cosine_ops)"];
        assert_eq!(vector_operator("embedding", &defs), "<=>");
        assert_eq!(vector_operator("other", &defs), "<->");
        let sql = similar_rows_sql("public", "items", "embedding", &[("id".into(), Some("1".into()))], "<=>", 10);
        assert_eq!(sql, "SELECT embedding <=> (SELECT embedding FROM public.items WHERE id = '1') AS distance, *\nFROM public.items\nWHERE NOT (id = '1')\nORDER BY distance\nLIMIT 10");
    }

    #[test]
    fn split_rest_requests() {
        let t = "# list\nGET /collections\n\nPOST /collections/x/points/search\n{\n  vector: [1, 2],\n\n  limit: 3\n}\nDELETE /collections/y";
        let s: Vec<_> = split_statements(t, DbKind::Qdrant).into_iter().map(|(a, b)| &t[a..b]).collect();
        assert_eq!(s, ["GET /collections", "POST /collections/x/points/search\n{\n  vector: [1, 2],\n\n  limit: 3\n}", "DELETE /collections/y"]);
        assert_eq!(statement_at(t, t.find("limit").unwrap(), DbKind::Qdrant).unwrap(), s[1]);
    }

    #[test]
    fn split_script_without_semicolons() {
        let parts = |t: &str| split_script(t, DbKind::Postgres).into_iter().map(|(a, b)| t[a..b].trim().to_string()).collect::<Vec<_>>();
        assert_eq!(parts("select * from a where x = 3\nselect * from public.users"), ["select * from a where x = 3", "select * from public.users"]);
        assert_eq!(parts("delete from t where id = 1\nselect 1;\nselect 2"), ["delete from t where id = 1", "select 1", "select 2"]);
        assert_eq!(parts("insert into t values (1)\ninsert into t values (2)").len(), 2);
        assert_eq!(parts("grant select on a to x\ngrant select on b to x").len(), 2);
        assert_eq!(parts("with x as (\n  select 1\n)\nselect * from x\nselect 2"), ["with x as (\n  select 1\n)\nselect * from x", "select 2"]);
        // 앞의 메모 줄은 첫 문장에 붙는다 (실행할 때 TUI/GUI 가 뺀다)
        assert_eq!(parts("1. 공격 탐지 (SSRF)\nselect 1\nselect 2"), ["1. 공격 탐지 (SSRF)\nselect 1", "select 2"]);
        // 한 문장인 것
        for one in [
            "select 1\nunion all\nselect 2",
            "select 1\nunion\nselect 2",
            "insert into t (a)\nselect a from u",
            "insert into t (a)\nwith x as (select 1) select * from x",
            "update t\nset a = 1\nwhere id in (\nselect id from u\n)",
            "create view v as\nselect 1",
            "create table t as\nselect 1",
            "alter table t\ndrop column a",
            "explain\nselect 1",
            "grant select,\ninsert on t to u",
            "grant\nselect on t to u",
            "select * from t for\nupdate",
            "insert into t values (1) on conflict (id) do\nupdate set a = 1",
            "select 'a\nselect b'",
            "select a -- x\n-- select y\nfrom t",
            "select a /* \nselect */ from t",
            "select *\nfrom t\nwhere a in\n(select 1)",
            "select a,\nselect_count from t",
        ] {
            assert_eq!(parts(one).len(), 1, "{one}");
        }
        // MySQL: ON DUPLICATE KEY UPDATE
        assert_eq!(split_script("insert into t values (1) on duplicate key\nupdate a = 1", DbKind::Mysql).len(), 1);
        // Redis 는 그대로 (한 줄 = 명령)
        assert_eq!(split_script("GET a\nSET b 1", DbKind::Redis).len(), 2);
    }

    #[test]
    fn split_follows_server_comment_rules() {
        let n = |t: &str, k| split_statements(t, k).len();
        // MySQL: `--x` 는 주석이 아니다 → 문장 셋 (실제로 MCP 읽기 전용이 뚫렸던 형태)
        assert_eq!(n("select 1 --1; COMMIT; DELETE FROM t", DbKind::Mysql), 3);
        assert_eq!(n("select 1 -- x; delete", DbKind::Mysql), 1);
        // MySQL /*! … */ 안은 코드
        assert_eq!(n("/*! COMMIT; DELETE FROM t; */ select 1", DbKind::Mysql), 3);
        assert_eq!(n("/* c; */ select 1", DbKind::Mysql), 1);
        // MariaDB /*M! … */ 안도 코드
        assert_eq!(n("/*M! COMMIT; DELETE FROM t; */ select 1", DbKind::Mysql), 3);
        // MySQL 백틱 안의 \ 는 이스케이프가 아니다 (서버는 문장 셋으로 본다)
        assert_eq!(n("select 1 as `a\\`, 1 as `b`; delete from t; -- `", DbKind::Mysql), 3);
        // PG: 중첩 주석, E'' 의 \'
        assert_eq!(n("/* a /* b */ ; */ select 1", DbKind::Postgres), 1);
        assert_eq!(n("select E'\\'; x'; select 2", DbKind::Postgres), 2);
        assert_eq!(n("select '\\'; select 2", DbKind::Postgres), 2);
    }
}
