//! SQL utilities per DB kind: quoting, the lexer rules the read-only check shares, statement splitting.
//! Same rules as Dialect in the frontend's lib/api.ts. (Data source helpers such as target_id stay in qurry_core::sql.)
use crate::kind::DbKind;

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
        // ClickHouse reads \ as an escape inside "names" too (escapes_in), so it is doubled
        DbKind::Clickhouse => if simple(true) { s.into() } else { format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\"\"")) },
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

/// Query text with each string and number literal replaced by `?` (MCP's Database Admin reports: session and statement text can hold
/// values from the rows). Read with scope::lex, so names, quoted names, comments, PostgreSQL `$1` and MySQL digest `?` stay as written.
/// A prefix glued to a string goes with it (E'…', X'…', B'…', N'…', MySQL _charset'…'); an unclosed literal at the end (text cut by
/// left(query, 4000)) runs to the end and is masked whole. MySQL `/*! … */` · `/*M! … */` are code the server runs, so their inside is masked too
pub fn mask_literals(text: &str, kind: DbKind) -> String {
    use crate::scope::{K, Tok};
    let toks = crate::scope::lex(text, kind == DbKind::Mysql);
    let (mut out, mut at) = (String::with_capacity(text.len()), 0);
    let src = |t: &Tok| &text[t.s..t.e];
    for (i, t) in toks.iter().enumerate() {
        let prev = i.checked_sub(1).map(|p| &toks[p]).filter(|p| p.e == t.s);
        let start = match t.k {
            K::Str => match prev {
                Some(p) if p.k == K::Word && ((src(p).len() == 1 && "eEbBxXnN".contains(src(p))) || src(p).starts_with('_')) => p.s,
                _ => t.s,
            },
            // $1 is a placeholder
            K::Num if prev.is_none_or(|p| src(p) != "$") => t.s,
            _ => continue,
        };
        mask_gap(&mut out, &text[at..start], kind);
        out.push('?');
        at = t.e;
    }
    mask_gap(&mut out, &text[at..], kind);
    out
}

/// Text between tokens (whitespace and comments) as is, except the inside of MySQL executable comments
fn mask_gap(out: &mut String, gap: &str, kind: DbKind) {
    let mut rest = gap;
    while kind == DbKind::Mysql
        && let Some(i) = rest.find("/*")
    {
        let end = rest[i + 2..].find("*/").map_or(rest.len(), |j| i + 2 + j);
        let marker = [("/*!", 3), ("/*M!", 4)].into_iter().find(|(m, _)| rest[i..].starts_with(m)).map(|(_, n)| n);
        // The version number after the marker isn't a literal
        let Some(m) = marker.map(|m| m + rest[i + m..end].bytes().take_while(u8::is_ascii_digit).count()) else {
            let past = (end + 2).min(rest.len());
            out.push_str(&rest[..past]);
            rest = &rest[past..];
            continue;
        };
        out.push_str(&rest[..i + m]);
        out.push_str(&mask_literals(&rest[i + m..end], kind));
        rest = &rest[end..];
    }
    out.push_str(rest);
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
    split_statements_with(text, kind, SplitOptions::default())
}

/// Editor rules the GUI keeps from its old TypeScript splitter, where it splits differently from the server and the TUI
/// (D2 in the 0.3.0 architecture batch; the inputs are the splitStatements · splitScript rows of tests/DIVERGENCES.md).
/// The default is the server's rules. Only for picking what the editor runs: the backend splits again with the default before any check.
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SplitOptions {
    /// `$tag$` opens a dollar quote right after a name character too (`foo$x$`)
    pub dollar_after_name: bool,
}

/// `--` at b[i] starts a line comment (MySQL needs a space or control char after it: `select 1--1` is arithmetic)
pub(crate) fn dash_comment(b: &[u8], i: usize, e: usize, kind: DbKind) -> bool {
    b[i] == b'-' && i + 1 < e && b[i + 1] == b'-' && (kind != DbKind::Mysql || i + 2 >= e || b[i + 2].is_ascii_whitespace() || b[i + 2].is_ascii_control())
}

/// Where the scan goes on when b[i] starts what isn't code — a comment, a string, a quoted name, a dollar quote — past its end
/// (the text's end if unclosed). MySQL `/*!` · `/*M!` give the position after the marker: their inside is code. None: b[i] is code
fn skip_non_code(text: &str, i: usize, kind: DbKind, opts: SplitOptions) -> Option<usize> {
    let b = text.as_bytes();
    let n = b.len();
    let c = b[i];
    let d = if i + 1 < n { b[i + 1] } else { 0 };
    if dash_comment(b, i, n, kind) || (c == b'#' && hash_comment(kind, d)) {
        return Some(text[i..].find('\n').map(|j| i + j + 1).unwrap_or(n));
    }
    if c == b'/' && d == b'*' {
        // MySQL /*! … */ is executed code, not a comment: read its inside as normal
        if kind == DbKind::Mysql && i + 2 < n && b[i + 2] == b'!' {
            return Some(i + 3);
        }
        // MariaDB /*M! … */ is executed code too
        if kind == DbKind::Mysql && i + 3 < n && b[i + 2] == b'M' && b[i + 3] == b'!' {
            return Some(i + 4);
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
        return Some(j);
    }
    if c == b'\'' || c == b'"' || c == b'`' {
        // In PG E'…' strings \ is an escape
        let e_string = e_strings(kind) && c == b'\'' && is_e_string(b, i);
        let mut j = i + 1;
        while j < n {
            // \ inside MySQL backtick names is not an escape
            if ((backslash_escapes(kind) && escapes_in(kind, c)) || e_string) && b[j] == b'\\' {
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
        return Some((j + 1).min(n));
    }
    // [name] (]] is one ])
    if c == b'[' && bracket_names(kind) {
        let mut j = i + 1;
        while j < n && !(b[j] == b']' && !(j + 1 < n && b[j + 1] == b']')) {
            j += if b[j] == b']' { 2 } else { 1 };
        }
        return Some((j + 1).min(n));
    }
    if c == b'$' && dollar_quotes(kind) && (dollar_opens(b, i) || opts.dollar_after_name) {
        let rest = &text[i..];
        if let Some(te) = rest[1..].find('$').map(|j| j + 2) {
            let tag = &rest[..te];
            if is_dollar_tag(tag) {
                return Some(text[i + te..].find(tag).map(|j| i + te + j + te).unwrap_or(n));
            }
        }
    }
    None
}

/// split_statements with the given editor rules
pub fn split_statements_with(text: &str, kind: DbKind, opts: SplitOptions) -> Vec<(usize, usize)> {
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
    let n = text.len();
    let (mut out, mut start, mut i) = (Vec::new(), 0usize, 0usize);
    while i < n {
        if let Some(j) = skip_non_code(text, i, kind, opts) {
            i = j;
            continue;
        }
        let c = text.as_bytes()[i];
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
        let w = l.split_whitespace().next().unwrap_or("").to_uppercase();
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
    split_script_with(text, kind, SplitOptions::default())
}

/// split_script with the given editor rules
pub fn split_script_with(text: &str, kind: DbKind, opts: SplitOptions) -> Vec<(usize, usize)> {
    let segs = split_statements_with(text, kind, opts);
    if kind == DbKind::Redis || kind.is_vector() {
        return segs;
    }
    let mut out = Vec::new();
    for (s, e) in segs {
        let mut start = s;
        for cut in script_cuts(text, s, e, kind, opts) {
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
fn script_cuts(text: &str, s: usize, e: usize, kind: DbKind, opts: SplitOptions) -> Vec<usize> {
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
        if dash_comment(b, i, e, kind) || (c == b'#' && hash_comment(kind, d)) {
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
        if c == b'$' && dollar_quotes(kind) && (dollar_opens(b, i) || opts.dollar_after_name) {
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
    statement_span(text, offset, kind).map(|(s, e)| text[s..e].trim().to_string())
}

/// The statement at the caret (as [`statement_at`] picks it) from its start up to the caret; "" before its start
pub fn statement_head(text: &str, offset: usize, kind: DbKind) -> &str {
    statement_span(text, offset, kind).and_then(|(s, _)| text.get(s..offset)).unwrap_or("")
}

/// The byte range of the statement at the caret (as [`statement_at`] picks it, untrimmed)
pub fn statement_span(text: &str, offset: usize, kind: DbKind) -> Option<(usize, usize)> {
    let segs = split_script(text, kind);
    segs.iter().find(|(s, e)| offset >= *s && offset <= e + 1).or_else(|| segs.iter().rev().find(|(_, e)| *e < offset)).or(segs.first()).copied()
}

/// Words a parenthesised query starts with (after any further `(`): what the Execute chooser offers to run on its own
const QUERY_HEADS: &[&str] = &["SELECT", "WITH", "VALUES", "TABLE"];

/// The parenthesised queries in [start, end) of `text` that enclose the caret (`offset`, bytes), innermost first, as trimmed byte
/// ranges of their inside: `(select …)` subqueries, CTE bodies, `IN (select …)`. Parentheses that hold no query (`IN (1, 2)`,
/// `count(*)`) and those in strings or comments are not offered. Like DataGrip's Execute chooser, after the whole statement
pub fn enclosing_subqueries(text: &str, start: usize, end: usize, offset: usize, kind: DbKind) -> Vec<(usize, usize)> {
    if kind == DbKind::Redis || kind.is_vector() {
        return vec![];
    }
    let b = text.as_bytes();
    let (mut open, mut out, mut i) = (Vec::new(), Vec::new(), start);
    while i < end {
        if let Some(j) = skip_non_code(text, i, kind, SplitOptions::default()) {
            i = j;
            continue;
        }
        match b[i] {
            b'(' => open.push(i),
            b')' => {
                if let Some(o) = open.pop()
                    && o < offset
                    && offset <= i
                {
                    let (s, e) = trimmed(text, o + 1, i);
                    let head: String = text[s..e].trim_start_matches(|c: char| c == '(' || c.is_whitespace()).chars().take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '$').collect();
                    // `((select 1))` runs the same as the inner one: offered once
                    let same = e - s >= 2 && out.last() == Some(&trimmed(text, s + 1, e - 1));
                    if QUERY_HEADS.contains(&head.to_ascii_uppercase().as_str()) && !same {
                        out.push((s, e));
                    }
                }
            }
            _ => {}
        }
        i += 1;
    }
    // Closed in order: inner ones close first
    out
}

/// [s, e) without the whitespace at its ends
fn trimmed(text: &str, s: usize, e: usize) -> (usize, usize) {
    let raw = &text[s..e];
    (s + (raw.len() - raw.trim_start().len()), s + raw.trim_end().len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subqueries_around_the_caret() {
        let subs = |t: &str, at: &str, kind| {
            let off = t.find(at).unwrap();
            enclosing_subqueries(t, 0, t.len(), off, kind).into_iter().map(|(a, b)| t[a..b].to_string()).collect::<Vec<_>>()
        };
        let pg = DbKind::Postgres;
        // Nested: innermost first
        let t = "select * from a where id in (select a_id from b where x = (select max(x) from c))";
        assert_eq!(subs(t, "max", pg), ["select max(x) from c", "select a_id from b where x = (select max(x) from c)"]);
        assert_eq!(subs(t, "a_id", pg), ["select a_id from b where x = (select max(x) from c)"]);
        assert!(subs(t, "from a", pg).is_empty());
        // A list or a call is not a query (unless it holds one)
        assert!(subs("select * from a where id in (1, 2, 3)", "2", pg).is_empty());
        assert!(subs("select count(*) from a", "*", pg).is_empty());
        assert_eq!(subs("select * from a where id in ((select 1))", "1", pg), ["select 1"]);
        // A word only starts a query when it is the whole identifier: `table_name`, `with_tax`, `select_count` are columns
        assert!(subs("select * from information_schema.columns where (table_name = 'x')", "name", pg).is_empty());
        assert!(subs("select count(table_name) from information_schema.tables", "name", pg).is_empty());
        assert!(subs("select coalesce(with_tax, 0) from t", "tax", pg).is_empty());
        assert!(subs("insert into t (select_count, b) values (1, 2)", "count", pg).is_empty());
        assert!(subs("select (values_json) from t", "json", pg).is_empty());
        // CTE bodies
        let t = "with x as (\n  select 1 as n\n), y as (values (2)) select * from x, y";
        assert_eq!(subs(t, "as n", pg), ["select 1 as n"]);
        assert_eq!(subs(t, "(2)", pg), ["values (2)"]);
        // Parentheses in strings and comments are not counted
        let t = "select * from (select ')' as p, 1 /* ( */ -- (\n from t) s";
        assert_eq!(subs(t, "as p", pg), ["select ')' as p, 1 /* ( */ -- (\n from t"]);
        assert_eq!(subs("select '(select 1)'", "1", pg), Vec::<String>::new());
        assert_eq!(subs("select $$(select 1)$$", "1", pg), Vec::<String>::new());
        // MySQL: # comments, backslash escapes
        assert_eq!(subs("select * from (select 'a\\')' x # )\n from t) s", "x #", DbKind::Mysql), ["select 'a\\')' x # )\n from t"]);
        // Caret right after `(` or right before `)` is inside
        let t = "select (select 1)";
        assert_eq!(enclosing_subqueries(t, 0, t.len(), t.find('(').unwrap() + 1, pg).len(), 1);
        assert_eq!(enclosing_subqueries(t, 0, t.len(), t.len() - 1, pg).len(), 1);
        assert!(enclosing_subqueries(t, 0, t.len(), t.len(), pg).is_empty());
        // Only within the statement's range
        let t = "select 1; select (select 2)";
        assert!(enclosing_subqueries(t, 0, 8, t.find('2').unwrap(), pg).is_empty());
        assert!(subs("GET (select 1)", "1", DbKind::Redis).is_empty());
    }

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

    /// Each editor option changes only its own rule (the GUI's rows in tests/DIVERGENCES.md); the default stays the server's
    #[test]
    fn split_options_keep_the_gui_rules() {
        let parts = |t: &str, k, o| split_script_with(t, k, o).into_iter().map(|(a, b)| t[a..b].to_string()).collect::<Vec<_>>();
        // MySQL `--` with no space after it is no editor rule: `select 1--1; delete …` runs the DELETE, so the console splits it off
        assert_eq!(parts("select 1--1;\nselect 2", DbKind::Mysql, SplitOptions::default()), ["select 1--1", "\nselect 2"]);
        let dollar = SplitOptions { dollar_after_name: true };
        assert_eq!(split_statements_with("select foo$x$ from t; select 1", DbKind::Postgres, dollar).len(), 1);
        assert_eq!(split_statements("select foo$x$ from t; select 1", DbKind::Postgres).len(), 2);
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

    #[test]
    fn mask_literals_postgres() {
        let m = |s: &str| mask_literals(s, DbKind::Postgres);
        assert_eq!(m("select 'abc', 'it''s', 42, 3.14, -7, .5"), "select ?, ?, ?, ?, -?, .?");
        assert_eq!(m(r"select E'it\'s', e'a\\', X'1F', B'0101', N'name', U&'d\0061'"), "select ?, ?, ?, ?, ?, U&?");
        assert_eq!(m("select $$se'cret$$, $t$a $$ b$t$ from t"), "select ?, ? from t");
        assert_eq!(m("insert into t1 (col2) values (1e10, '2024-01-01'::date)"), "insert into t1 (col2) values (?, ?::date)");
        // \ is a plain character in '…' (standard_conforming_strings): the string ends at the next '
        assert_eq!(m(r"select 'a\', 'b'"), "select ?, ?");
        // Left alone: names with digits, quoted names (quotes inside too), placeholders, comments
        let kept = "select \"t1\".col2, \"it's 5\" from s3.t1 where id = $1 and x = $12 -- 'note' 5\n/* 'c' 7 */";
        assert_eq!(m(kept), kept);
        assert_eq!(m(r#"select "Weird""Name", 'v'"#), r#"select "Weird""Name", ?"#);
    }

    #[test]
    fn mask_literals_mysql() {
        let m = |s: &str| mask_literals(s, DbKind::Mysql);
        assert_eq!(m(r#"select 'a\'b', "dq", "d\"q", _utf8mb4'x', _binary 'y', x'1f', b'01', N'n', 0x1F"#), "select ?, ?, ?, ?, _binary ?, ?, ?, ?, ?");
        // Digest text keeps ?, backtick names and comments; executable comments are code
        let kept = "SELECT `t1`.`col2` FROM `db1`.`t1` WHERE `a` = ? AND `b` IN (...) # 'c' 9\n-- 'd'\n/* 'e' */";
        assert_eq!(m(kept), kept);
        assert_eq!(m("select /*!40101 'x', 5 */ 1 /*M!100108 'y' */"), "select /*!40101 ?, ? */ ? /*M!100108 ? */");
        // The other quoting: \ escapes in MySQL strings (one string, then an unclosed b'…'); "…" is a string in MySQL, a name in PostgreSQL
        assert_eq!(m(r"select 'a\', 'b'"), "select ??");
        assert_eq!(mask_literals(r#"select "dq""#, DbKind::Postgres), r#"select "dq""#);
    }

    /// PostgreSQL comments nest, MySQL's don't: the literal after the inner */ is code in MySQL, still comment in PostgreSQL
    #[test]
    fn mask_literals_nested_comments() {
        let pg = |s: &str| mask_literals(s, DbKind::Postgres);
        assert_eq!(pg("/* a /* b */ it's */ select 'secret-1'"), "/* a /* b */ it's */ select ?");
        assert_eq!(pg("/* a /* b */ 'c' */ select 2"), "/* a /* b */ 'c' */ select ?");
        assert_eq!(pg("select 1 /* a /* b */ 'c'"), "select ? /* a /* b */ 'c'");
        assert_eq!(mask_literals("/* a /* b */ 'c' */ select 2", DbKind::Mysql), "/* a /* b */ ? */ select ?");
    }

    /// Report SQL cuts query text with left(query, 4000): the cut literal runs to the end
    #[test]
    fn mask_literals_cut_at_the_end() {
        let cut = |open: &str| format!("select * from t where token = {open}{}", "s".repeat(5000)).chars().take(4000).collect::<String>();
        for (open, kind) in [("'", DbKind::Postgres), (r"E'\'", DbKind::Postgres), ("$q$", DbKind::Postgres), ("'", DbKind::Mysql), ("\"", DbKind::Mysql), ("_latin1'", DbKind::Mysql)] {
            assert_eq!(mask_literals(&cut(open), kind), "select * from t where token = ?", "{open}");
        }
        assert_eq!(mask_literals("select 1 /*!40101 'x", DbKind::Mysql), "select ? /*!40101 ?");
    }
}
