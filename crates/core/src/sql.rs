//! SQL utilities per DB kind (shared by TUI·MCP). Same rules as Dialect in the frontend's lib/api.ts.
use crate::store::{ConnectionConfig, DbKind};

/// Connection target id: "<data source id>::<database>"
pub fn target_id(conn: &str, db: Option<&str>) -> String {
    match db.filter(|d| !d.is_empty()) {
        Some(d) => format!("{conn}::{d}"),
        None => conn.to_string(),
    }
}

/// Default DB name for the tree·console
pub fn default_db(cfg: &ConnectionConfig) -> String {
    match cfg.kind {
        DbKind::Postgres => if cfg.database.trim().is_empty() { "postgres".into() } else { cfg.database.trim().into() },
        DbKind::Mysql => cfg.database.trim().into(),
        DbKind::Redis => format!("db{}", crate::redisdb::db_index(&cfg.database)),
        // File DBs use main, other drivers use the configured DB name as is
        k if k.is_file() => "main".into(),
        // Vector DB: the server's default name if empty
        DbKind::Qdrant if cfg.database.trim().is_empty() => "qdrant".into(),
        DbKind::Chroma if cfg.database.trim().is_empty() => "default_database".into(),
        DbKind::Milvus if cfg.database.trim().is_empty() => "default".into(),
        _ => cfg.database.trim().into(),
    }
}

/// Reserved words that break syntax unless quoted (union of PG·MySQL; extra quoting does no harm)
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
        // Drivers: standard quoting (SQL Server·Oracle·ClickHouse·SQLite·DuckDB all accept "…"). Only lowercase names stay bare so case isn't changed
        DbKind::Mongodb | DbKind::Qdrant | DbKind::Chroma | DbKind::Milvus => s.into(),
        _ => if simple(true) { s.into() } else { format!("\"{}\"", s.replace('"', "\"\"")) },
    }
}

/// Whether it is a pgvector column (vector(3) · halfvec · sparsevec)
pub fn is_vector_type(t: &str) -> bool {
    let t = t.trim().to_lowercase();
    ["vector", "halfvec", "sparsevec"].iter().any(|v| t == *v || t.starts_with(&format!("{v}(")) || t.ends_with(&format!(".{v}")))
}

/// Distance operator used by this column's vector index (so the index is used). L2 (<->) if none
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

/// pgvector: rows near this row (excluding itself, by distance). pk = (column, value)
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
        // MySQL and ClickHouse read \ as an escape inside strings
        Some(v) if backslash_escapes(kind) => format!("'{}'", v.replace('\\', "\\\\").replace('\'', "''")),
        // PostgreSQL: E'' reads \ the same way whatever standard_conforming_strings is set to on the server
        Some(v) if kind == DbKind::Postgres && v.contains('\\') => format!("E'{}'", v.replace('\\', "\\\\").replace('\'', "''")),
        Some(v) => format!("'{}'", v.replace('\'', "''")),
    }
}

/// Split statements on semicolons (skipping strings·comments·PG dollar quotes). Redis is one line = one command. (start, end) byte ranges.
/// Must parse with the same rules as the server (the read-only check counts statements with this):
/// in MySQL `--` needs a following space to be a comment and `/*! … */` is executed code; in PG comments nest and \ escapes inside E'…'
/// DBs where \ inside strings is an escape (MySQL · ClickHouse). Same rule as guard::scan
pub(crate) fn backslash_escapes(kind: DbKind) -> bool {
    matches!(kind, DbKind::Mysql | DbKind::Clickhouse)
}

/// Whether it is a line comment starting with #: always in MySQL, "# " · "#!" in ClickHouse (next = byte after #, 0 at end)
pub(crate) fn hash_comment(kind: DbKind, next: u8) -> bool {
    kind == DbKind::Mysql || (kind == DbKind::Clickhouse && matches!(next, b' ' | b'!' | b'\n' | b'\t' | 0))
}

/// PostgreSQL dollar-quote tag ($$ · $tag$): the tag is name chars (non-ASCII included), not starting with a digit
pub(crate) fn is_dollar_tag(tag: &str) -> bool {
    let inner = &tag[1..tag.len() - 1];
    inner.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || !ch.is_ascii()) && !inner.starts_with(|ch: char| ch.is_ascii_digit())
}

/// Whether it is PG E'…': right before ' is e/E, and before that is not a name char (name' is not)
pub(crate) fn is_e_string(b: &[u8], i: usize) -> bool {
    i > 0 && (b[i - 1] == b'e' || b[i - 1] == b'E') && (i < 2 || !is_name_byte(b[i - 2]))
}

/// A byte that can continue a name (letters, digits, _, $, non-ASCII)
pub(crate) fn is_name_byte(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_' || c == b'$' || c >= 0x80
}

/// Databases with E'…' strings (\ escapes inside)
pub(crate) fn e_strings(kind: DbKind) -> bool {
    matches!(kind, DbKind::Postgres | DbKind::Duckdb)
}

/// Databases with dollar-quoted strings ($$…$$, $tag$…$tag$; ClickHouse calls them heredocs)
pub(crate) fn dollar_quotes(kind: DbKind) -> bool {
    matches!(kind, DbKind::Postgres | DbKind::Duckdb | DbKind::Clickhouse)
}

/// A $ at i can open a dollar quote: not in the middle of a name (x$q$ is one identifier)
pub(crate) fn dollar_opens(b: &[u8], i: usize) -> bool {
    i == 0 || !is_name_byte(b[i - 1])
}

/// Databases where [name] is a quoted identifier
pub(crate) fn bracket_names(kind: DbKind) -> bool {
    matches!(kind, DbKind::Mssql | DbKind::Sqlite)
}

/// Whether \ escapes inside this quote character (ClickHouse also escapes inside `names` and "names", MySQL only in strings)
pub(crate) fn escapes_in(kind: DbKind, quote: u8) -> bool {
    match kind {
        DbKind::Clickhouse => true,
        DbKind::Mysql => quote != b'`',
        _ => false,
    }
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
            // MySQL /*! … */ is executed code, not a comment: read its inside as normal
            if kind == DbKind::Mysql && i + 2 < n && b[i + 2] == b'!' {
                i += 3;
                continue;
            }
            // MariaDB /*M! … */ is executed code too
            if kind == DbKind::Mysql && i + 3 < n && b[i + 2] == b'M' && b[i + 3] == b'!' {
                i += 4;
                continue;
            }
            // PG comments nest
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
            // In PG E'…' strings \ is an escape
            let e_string = e_strings(kind) && c == b'\'' && is_e_string(b, i);
            let mut j = i + 1;
            while j < n {
                // \ inside MySQL backtick names is not an escape
                if ((backslash && escapes_in(kind, c)) || e_string) && b[j] == b'\\' {
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
        // [name] (]] is one ])
        if c == b'[' && bracket_names(kind) {
            let mut j = i + 1;
            while j < n && !(b[j] == b']' && !(j + 1 < n && b[j + 1] == b']')) {
                j += if b[j] == b']' { 2 } else { 1 };
            }
            i = (j + 1).min(n);
            continue;
        }
        if c == b'$' && dollar_quotes(kind) && dollar_opens(b, i) {
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

/// REST request: from a line starting with a method like "GET /x" up to the next request (including JSON body). # · // lines are comments
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

/// Statement splitting for the editor: after splitting on semicolons, also split statements written without `;` by statement keywords at line starts (like DataGrip).
/// `select … where x = 3` followed by a line `select * from users` → two statements. UNION·INSERT … SELECT·WITH … SELECT·GRANT SELECT etc. are one statement.
/// Used only to pick the statement to run (the read-only check uses split_statements, same as the server)
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

/// Keywords that may start a new statement at a line start (excluding ones common at line starts inside a statement, like SET·DO·VALUES)
const SCRIPT_STARTERS: &[&str] = &["SELECT", "INSERT", "UPDATE", "DELETE", "WITH", "CREATE", "ALTER", "DROP", "TRUNCATE", "GRANT", "REVOKE", "EXPLAIN", "SHOW", "MERGE", "CALL"];
/// Keywords that start a statement (otherwise the preceding text is treated as a note, not SQL)
const SQL_HEADS: &[&str] = &[
    "SELECT", "INSERT", "UPDATE", "DELETE", "WITH", "CREATE", "ALTER", "DROP", "TRUNCATE", "GRANT", "REVOKE", "EXPLAIN", "SHOW", "MERGE", "CALL", "BEGIN", "START", "COMMIT", "ROLLBACK", "END",
    "ABORT", "SAVEPOINT", "RELEASE", "SET", "RESET", "VALUES", "TABLE", "DO", "COPY", "VACUUM", "ANALYZE", "REFRESH", "LOCK", "REINDEX", "CLUSTER", "COMMENT", "DECLARE", "FETCH", "CLOSE",
    "PREPARE", "EXECUTE", "DEALLOCATE", "LISTEN", "NOTIFY", "UNLISTEN", "USE", "DESC", "DESCRIBE", "REPLACE", "LOAD", "OPTIMIZE", "RENAME", "IMPORT", "DISCARD", "CHECKPOINT", "SECURITY",
];
/// Lines after this word continue the previous statement
const SCRIPT_CONT: &[&str] = &[
    "UNION", "ALL", "INTERSECT", "EXCEPT", "MINUS", "DISTINCT", "AS", "DO", "THEN", "ELSE", "WHEN", "AND", "OR", "NOT", "IN", "EXISTS", "ANY", "SOME", "BEFORE", "AFTER", "OF", "ON", "FOR", "KEY",
    "INSTEAD", "BY", "FROM", "JOIN", "WHERE", "TABLE", "INTO", "RETURNING", "GRANT", "REVOKE", "EXPLAIN", "ANALYZE", "VERBOSE", "IS", "LIKE", "BETWEEN", "WITH",
];

/// split_script: line-start positions within [s, e) where a new statement begins
fn script_cuts(text: &str, s: usize, e: usize, kind: DbKind) -> Vec<usize> {
    let b = text.as_bytes();
    let mut cuts = Vec::new();
    let (mut i, mut depth, mut line_start, mut line_has_code) = (s, 0i32, s, false);
    // First word of the current statement, previous token (uppercase if a word, the char + prev_punct if a symbol, '' if a string)
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
        // Comments (line end / block) — same rules as split_statements
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
            // If a block comment spanned lines, what follows is not a line start
            if text[i..j].contains('\n') {
                line_has_code = true;
            }
            i = j;
            continue;
        }
        let first_on_line = !line_has_code;
        line_has_code = true;
        // String · quoted name · dollar quote = one value
        if c == b'\'' || c == b'"' || c == b'`' {
            let e_string = e_strings(kind) && c == b'\'' && is_e_string(b, i);
            let mut j = i + 1;
            while j < e {
                if (escapes_in(kind, c) || e_string) && b[j] == b'\\' {
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
        if c == b'$' && dollar_quotes(kind) && dollar_opens(b, i) {
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
                // The first SELECT/INSERT/UPDATE/DELETE after WITH … is that statement's body
                let with_body = head == "WITH" && !with_main && matches!(w.as_str(), "SELECT" | "INSERT" | "UPDATE" | "DELETE" | "MERGE");
                if with_body {
                    with_main = true;
                } else if starter && first_on_line && !SQL_HEADS.contains(&head.as_str()) {
                    // Preceding lines are a title·note, not SQL: the statement starts here (the note is stripped when running)
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

/// Statement at the caret position (bytes). Between statements, the one just before
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
        assert_eq!(literal(DbKind::Postgres, Some("a\\' OR 1=1")), "E'a\\\\'' OR 1=1'");
        assert_eq!(literal(DbKind::Postgres, Some("it's")), "'it''s'");
    }

    /// Statements on new lines without `;` (split like DataGrip)
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
        // Leading note lines attach to the first statement (TUI/GUI strip them when running)
        assert_eq!(parts("1. 공격 탐지 (SSRF)\nselect 1\nselect 2"), ["1. 공격 탐지 (SSRF)\nselect 1", "select 2"]);
        // Things that are a single statement
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
        // Redis as is (one line = command)
        assert_eq!(split_script("GET a\nSET b 1", DbKind::Redis).len(), 2);
    }

    #[test]
    fn split_follows_server_comment_rules() {
        let n = |t: &str, k| split_statements(t, k).len();
        // MySQL: `--x` is not a comment → three statements (the form that actually broke MCP read-only)
        assert_eq!(n("select 1 --1; COMMIT; DELETE FROM t", DbKind::Mysql), 3);
        assert_eq!(n("select 1 -- x; delete", DbKind::Mysql), 1);
        // Inside MySQL /*! … */ is code
        assert_eq!(n("/*! COMMIT; DELETE FROM t; */ select 1", DbKind::Mysql), 3);
        assert_eq!(n("/* c; */ select 1", DbKind::Mysql), 1);
        // Inside MariaDB /*M! … */ is code too
        assert_eq!(n("/*M! COMMIT; DELETE FROM t; */ select 1", DbKind::Mysql), 3);
        // \ inside MySQL backticks is not an escape (the server sees three statements)
        assert_eq!(n("select 1 as `a\\`, 1 as `b`; delete from t; -- `", DbKind::Mysql), 3);
        // PG: nested comments, \' in E''
        assert_eq!(n("/* a /* b */ ; */ select 1", DbKind::Postgres), 1);
        assert_eq!(n("select E'\\'; x'; select 2", DbKind::Postgres), 2);
        assert_eq!(n("select '\\'; select 2", DbKind::Postgres), 2);
    }
}
