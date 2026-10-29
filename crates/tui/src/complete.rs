//! 콘솔 자동완성 (GUI src/lib/completion.ts 의 TUI 판)
//! - `alias.` / `table.` → 그 테이블의 컬럼, `schema.` → 그 스키마의 테이블
//! - 그 밖: 문장에 나온 테이블의 컬럼 → 테이블 → 스키마 → 키워드
use qurry_core::meta::CompletionTable;
use std::collections::HashMap;

/// 자동완성 메타: 불러올 때 한 번 소문자 이름과 이름별 색인을 만들어 둔다 (키마다 다시 만들지 않게)
pub struct Meta {
    pub tables: Vec<CompletionTable>,
    /// (스키마, 테이블) 소문자
    lower: Vec<(String, String)>,
    /// 소문자 테이블 이름 → tables 위치들
    by_name: HashMap<String, Vec<usize>>,
}

impl Meta {
    pub fn new(tables: Vec<CompletionTable>) -> Self {
        let lower: Vec<(String, String)> = tables.iter().map(|t| (t.schema.to_lowercase(), t.name.to_lowercase())).collect();
        let mut by_name: HashMap<String, Vec<usize>> = HashMap::new();
        for (i, (_, n)) in lower.iter().enumerate() {
            by_name.entry(n.clone()).or_default().push(i);
        }
        Meta { tables, lower, by_name }
    }
}

const KEYWORDS: &str = "select from where and or not in is null like ilike between exists group by order having limit offset join left right inner outer full cross on using as \
distinct union all intersect except insert into values update set delete returning create table view index alter drop truncate with case when then else end \
asc desc nulls first last count sum avg min max coalesce now current_date current_timestamp interval true false begin commit rollback explain analyze";

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum ItemKind {
    Column,
    Table,
    Schema,
    Keyword,
    /// 컬럼 값 · json 키 (DB 에서 읽은 것)
    Value,
}

#[derive(Clone, Debug)]
pub struct Item {
    pub label: String,
    pub detail: String,
    pub insert: String,
    pub kind: ItemKind,
}

fn keywords() -> impl Iterator<Item = &'static str> {
    KEYWORDS.split_whitespace()
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// `from|join|update|into <ref> [as] [alias]` 를 찾는다
fn tables_in_statement(text: &str) -> Vec<(String, Option<String>)> {
    let toks: Vec<String> = tokenize(text);
    let mut out = vec![];
    for (i, t) in toks.iter().enumerate() {
        if !["from", "join", "update", "into"].contains(&t.to_lowercase().as_str()) {
            continue;
        }
        let Some(r) = toks.get(i + 1).filter(|r| r.chars().next().is_some_and(|c| is_word(c) || c == '"')) else { continue };
        let mut j = i + 2;
        if toks.get(j).is_some_and(|t| t.eq_ignore_ascii_case("as")) {
            j += 1;
        }
        let alias = toks.get(j).filter(|a| a.chars().all(is_word) && !keywords().any(|k| k.eq_ignore_ascii_case(a))).cloned();
        out.push((r.replace('"', ""), alias));
    }
    out
}

/// 단어(`schema.table`, `"quoted"` 포함) 와 기호로 나눈다
fn tokenize(text: &str) -> Vec<String> {
    let cs: Vec<char> = text.chars().collect();
    let mut out = vec![];
    let mut i = 0;
    while i < cs.len() {
        let c = cs[i];
        if is_word(c) || c == '"' {
            let start = i;
            loop {
                if i < cs.len() && cs[i] == '"' {
                    i += 1;
                    while i < cs.len() && cs[i] != '"' {
                        i += 1;
                    }
                    i = (i + 1).min(cs.len());
                } else {
                    while i < cs.len() && is_word(cs[i]) {
                        i += 1;
                    }
                }
                // schema.table 은 한 토큰
                if i + 1 < cs.len() && cs[i] == '.' && (is_word(cs[i + 1]) || cs[i + 1] == '"') {
                    i += 1;
                    continue;
                }
                break;
            }
            out.push(cs[start..i].iter().collect());
        } else {
            if !c.is_whitespace() {
                out.push(c.to_string());
            }
            i += 1;
        }
    }
    out
}

fn find_table<'a>(m: &'a Meta, r: &str, db: &str) -> Option<&'a CompletionTable> {
    let r = r.to_lowercase();
    let (schema, name) = match r.split_once('.') {
        Some((s, n)) => (Some(s), n),
        None => (None, r.as_str()),
    };
    let hits = m.by_name.get(name)?;
    let t = |i: &usize| &m.tables[*i];
    match schema {
        Some(s) => hits.iter().find(|&&i| m.lower[i].0 == s).map(t),
        None => hits.iter().find(|&&i| m.tables[i].schema == "public" || m.tables[i].schema == db).or(hits.first()).map(t),
    }
}

/// 커서 앞 텍스트로 후보를 만든다. 반환: (바꿀 접두어의 글자 수, 후보)
/// `text` 는 현재 문장 전체, `before` 는 커서가 있는 줄의 커서 앞부분, `db` 는 탭의 DB (MySQL 은 스키마 = DB)
pub fn suggest(m: &Meta, text: &str, before: &str, db: &str) -> (usize, Vec<Item>) {
    let all = &m.tables;
    let prefix: String = before.chars().rev().take_while(|c| is_word(*c)).collect::<Vec<_>>().into_iter().rev().collect();
    let head = &before[..before.len() - prefix.len()];
    let p = prefix.to_lowercase();
    // 이미 다 친 단어와 똑같은 후보는 빼서 Enter 가 줄바꿈으로 가게 한다
    let matches = |label: &str| p.is_empty() || (label.to_lowercase().starts_with(&p) && label.to_lowercase() != p);
    let n = prefix.chars().count();

    if let Some(h) = head.strip_suffix('.') {
        let q: String = h.chars().rev().take_while(|c| is_word(*c) || *c == '"').collect::<Vec<_>>().into_iter().rev().collect();
        let q = q.replace('"', "").to_lowercase();
        if q.is_empty() {
            return (n, vec![]);
        }
        // alias. 또는 table. → 컬럼
        let refs = tables_in_statement(text);
        let hit = refs
            .iter()
            .find(|(_, a)| a.as_ref().is_some_and(|a| a.to_lowercase() == q))
            .or_else(|| refs.iter().find(|(r, _)| r.rsplit('.').next().unwrap_or(r).to_lowercase() == q));
        let t = hit.and_then(|(r, _)| find_table(m, r, db)).or_else(|| find_table(m, &q, db));
        if let Some(t) = t {
            let items = t.columns.iter().filter(|(c, _)| matches(c)).map(|(c, ty)| Item { label: c.clone(), detail: ty.clone(), insert: c.clone(), kind: ItemKind::Column }).collect();
            return (n, items);
        }
        // schema. → 테이블
        let items = all
            .iter()
            .zip(&m.lower)
            .filter(|(_, (ls, ln))| *ls == q && (p.is_empty() || (ln.starts_with(&p) && *ln != p)))
            .map(|(t, _)| t)
            .map(|t| Item { label: t.name.clone(), detail: format!("{} cols", t.columns.len()), insert: t.name.clone(), kind: ItemKind::Table })
            .collect();
        return (n, items);
    }

    let mut items: Vec<Item> = vec![];
    for (r, _) in tables_in_statement(text) {
        if let Some(t) = find_table(m, &r, db) {
            for (c, ty) in &t.columns {
                if matches(c) && !items.iter().any(|i| &i.label == c) {
                    items.push(Item { label: c.clone(), detail: format!("{ty} · {}", t.name), insert: c.clone(), kind: ItemKind::Column });
                }
            }
        }
    }
    let mut tables: Vec<Item> = all
        .iter()
        .zip(&m.lower)
        .filter(|(_, (_, ln))| p.is_empty() || (ln.starts_with(&p) && *ln != p))
        .map(|(t, _)| Item {
            label: t.name.clone(),
            detail: t.schema.clone(),
            insert: if t.schema == "public" || t.schema == db { t.name.clone() } else { format!("{}.{}", t.schema, t.name) },
            kind: ItemKind::Table,
        })
        .collect();
    tables.sort_by(|a, b| a.label.cmp(&b.label));
    items.extend(tables);
    let mut schemas: Vec<&str> = all.iter().map(|t| t.schema.as_str()).filter(|s| matches(s)).collect();
    schemas.sort();
    schemas.dedup();
    items.extend(schemas.into_iter().map(|s| Item { label: s.into(), detail: "schema".into(), insert: s.into(), kind: ItemKind::Schema }));
    // 키워드는 한 글자 이상 쳤을 때만 (빈 접두어에 키워드 수십 개는 방해)
    if !p.is_empty() {
        items.extend(keywords().filter(|k| matches(k)).map(|k| Item { label: k.into(), detail: "keyword".into(), insert: k.into(), kind: ItemKind::Keyword }));
    }
    (n, items)
}

// ───────── 값 · json 키 추천 (GUI src/lib/valueHint.ts 와 같은 규칙) ─────────

/// 값 추천 자리: `status = '` · `in ('a', '` · `like '` → 그 컬럼의 값, `data->>'` · `data->'a'->>'` (MySQL `->>'$.a.`) → json 키
#[derive(Debug, Clone, PartialEq)]
pub struct HintAt {
    pub schema: String,
    pub table: String,
    pub column: String,
    pub type_name: String,
    pub path: Vec<String>,
    pub mode: HintMode,
    /// 따옴표 안에서 치는 중
    pub quoted: bool,
    /// 이미 친 글자
    pub typed: String,
    /// keys: 연산자부터 커서까지 (`->> '` + typed) — @> 조건으로 바꿔 넣을 때
    pub op: String,
}

/// values = 컬럼 값, keys = json 키, pairs = 많이 나온 `키: 값` 의 @> 조건 (PostgreSQL)
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HintMode {
    Values,
    Keys,
    Pairs,
}

struct Ctx {
    r#ref: Option<String>,
    column: String,
    path: Vec<String>,
    mode: HintMode,
    quoted: bool,
    typed: String,
    op: String,
}

static RES: std::sync::LazyLock<[regex::Regex; 4]> = std::sync::LazyLock::new(|| {
    let id = r#"(?:"[^"]+"|\w+)"#;
    let expr = format!(r"(?:({id})\.)?({id})((?:\s*->>?\s*(?:'[^']*'|\d+))*)");
    let op = r"\s*(=|<>|!=|@>|(?:not\s+)?i?like|(?:not\s+)?in\s*\((?:\s*(?:'[^']*'|[\w.-]+)\s*,)*)\s*";
    let my_val = format!(r#"(?:({id})\.)?({id})\s*->>?\s*'\$((?:\.(?:"[^"]*"|\w+))*)'"#);
    [
        regex::Regex::new(&format!(r#"(?:({id})\.)?({id})\s*->>?\s*'\$((?:\.(?:"[^"]*"|\w+))*)\.(\w*)$"#)).unwrap(),
        regex::Regex::new(&format!(r"(?i){my_val}{op}(?:'([^']*)|([\w.-]*))$")).unwrap(),
        regex::Regex::new(&format!(r"{expr}(\s*->>?\s*(?:'([^']*)|(\w*)))$")).unwrap(),
        regex::Regex::new(&format!(r"(?i){expr}{op}(?:'([^']*)|([\w.-]*))$")).unwrap(),
    ]
});

fn unq(s: &str) -> String {
    s.trim_matches('"').to_string()
}

fn path_of(seg: &str) -> Vec<String> {
    static RE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| regex::Regex::new(r"->>?\s*(?:'([^']*)'|(\d+))").unwrap());
    RE.captures_iter(seg).map(|c| c.get(1).or(c.get(2)).map(|m| m.as_str().to_string()).unwrap_or_default()).collect()
}

fn my_path(p: &str) -> Vec<String> {
    static RE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| regex::Regex::new(r#"\.(?:"([^"]*)"|(\w+))"#).unwrap());
    RE.captures_iter(p).map(|c| c.get(1).or(c.get(2)).map(|m| m.as_str().to_string()).unwrap_or_default()).collect()
}

const NOT_COLUMN: [&str; 13] = ["where", "and", "or", "not", "set", "on", "select", "by", "when", "then", "else", "case", "from"];

fn context(before: &str, mysql: bool) -> Option<Ctx> {
    let g = |c: &regex::Captures, i: usize| c.get(i).map(|m| m.as_str().to_string());
    let not_col = |s: &str| NOT_COLUMN.iter().any(|k| s.eq_ignore_ascii_case(k));
    let [my_keys, my_vals, keys, vals] = &*RES;
    if mysql {
        if let Some(c) = my_keys.captures(before) {
            return Some(Ctx { r#ref: g(&c, 1).map(|s| unq(&s)), column: unq(&c[2]), path: my_path(&c[3]), mode: HintMode::Keys, quoted: true, typed: c[4].to_string(), op: String::new() });
        }
        if let Some(c) = my_vals.captures(before) {
            if &c[4] == "@>" {
                return None;
            }
            return Some(Ctx { r#ref: g(&c, 1).map(|s| unq(&s)), column: unq(&c[2]), path: my_path(&c[3]), mode: HintMode::Values, quoted: c.get(5).is_some(), typed: g(&c, 5).or(g(&c, 6)).unwrap_or_default(), op: String::new() });
        }
    }
    if let Some(c) = keys.captures(before) {
        if c.get(5).is_some_and(|m| m.as_str().starts_with('$')) || not_col(&c[2]) {
            return None;
        }
        return Some(Ctx { r#ref: g(&c, 1).map(|s| unq(&s)), column: unq(&c[2]), path: path_of(&c[3]), mode: HintMode::Keys, quoted: c.get(5).is_some(), typed: g(&c, 5).or(g(&c, 6)).unwrap_or_default(), op: c[4].to_string() });
    }
    let c = vals.captures(before)?;
    if not_col(&c[2]) {
        return None;
    }
    let mode = if &c[4] == "@>" { HintMode::Pairs } else { HintMode::Values };
    if mysql && mode == HintMode::Pairs {
        return None;
    }
    Some(Ctx { r#ref: g(&c, 1).map(|s| unq(&s)), column: unq(&c[2]), path: path_of(&c[3]), mode, quoted: c.get(5).is_some(), typed: g(&c, 5).or(g(&c, 6)).unwrap_or_default(), op: String::new() })
}

/// 커서가 값 · json 키 자리면 그 테이블 · 컬럼 (추천할 만한 컬럼일 때만)
pub fn hint_at(m: &Meta, text: &str, before: &str, db: &str, mysql: bool) -> Option<HintAt> {
    let ctx = context(before, mysql)?;
    let refs = tables_in_statement(text);
    let named: Vec<&(String, Option<String>)> = match &ctx.r#ref {
        Some(q) => {
            let q = q.to_lowercase();
            refs.iter()
                .find(|(_, a)| a.as_ref().is_some_and(|a| a.to_lowercase() == q))
                .or_else(|| refs.iter().find(|(r, _)| r.rsplit('.').next().unwrap_or(r).to_lowercase() == q))
                .into_iter()
                .collect()
        }
        None => refs.iter().collect(),
    };
    let lc = ctx.column.to_lowercase();
    let (t, (col, ty)) = named.iter().filter_map(|(r, _)| find_table(m, r, db)).find_map(|t| t.columns.iter().find(|(c, _)| c.to_lowercase() == lc).map(|c| (t, c)))?;
    let ty_l = ty.to_lowercase();
    let json = ty_l.contains("json");
    let ok = if ctx.mode != HintMode::Values {
        json
    } else {
        !["bytea", "blob", "binary", "geometry", "geography", "tsvector", "[]"].iter().any(|k| ty_l.contains(k)) && !ty_l.starts_with('_') && (!json || !ctx.path.is_empty())
    };
    ok.then(|| HintAt { schema: t.schema.clone(), table: t.name.clone(), column: col.clone(), type_name: ty.clone(), path: ctx.path, mode: ctx.mode, quoted: ctx.quoted, typed: ctx.typed, op: ctx.op })
}

/// 읽어 온 값 → 후보 (친 글자로 거른다). close = 따옴표를 닫아 줄지 (커서 뒤에 ' 가 없을 때).
/// as_pairs = keys 자리인데 키가 없어서 (객체의 배열) @> 조건을 가져왔다 → 연산자째 바꿔 넣는다 (prefix = op 길이)
pub fn hint_items(h: &HintAt, hints: &[qurry_core::hints::Hint], close: bool, as_pairs: bool) -> Vec<Item> {
    let p = h.typed.to_lowercase();
    let bare = h.mode == HintMode::Values && h.path.is_empty() && ["int", "numeric", "decimal", "float", "double", "real", "serial", "bool", "bit", "money"].iter().any(|k| h.type_name.to_lowercase().contains(k));
    let rows = |n: i64| format!("{n} row{}", if n == 1 { "" } else { "s" });
    hints
        .iter()
        .filter(|x| as_pairs || p.is_empty() || (x.value.to_lowercase().starts_with(&p) && x.value.to_lowercase() != p))
        .map(|x| {
            let esc = x.value.replace('\'', "''");
            if as_pairs {
                let lead = if h.op.starts_with(char::is_whitespace) { " " } else { "" };
                return Item { label: format!("@> '{}'", x.value), detail: rows(x.count), insert: format!("{lead}@> '{esc}'"), kind: ItemKind::Value };
            }
            let insert = if h.quoted {
                if close { format!("{esc}'") } else { esc }
            } else if bare {
                x.value.clone()
            } else {
                format!("'{esc}'")
            };
            Item { label: x.value.clone(), detail: format!("{}{}", rows(x.count), if h.mode == HintMode::Keys { " · key" } else { "" }), insert, kind: ItemKind::Value }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta() -> Meta {
        let t = |s: &str, n: &str, cols: &[&str]| CompletionTable { schema: s.into(), name: n.into(), columns: cols.iter().map(|c| (c.to_string(), "text".to_string())).collect() };
        Meta::new(vec![t("public", "users", &["id", "user_name", "email"]), t("public", "orders", &["id", "user_id", "total"]), t("billing", "invoices", &["id", "amount"])])
    }

    fn labels(v: &[Item]) -> Vec<&str> {
        v.iter().map(|i| i.label.as_str()).collect()
    }

    #[test]
    fn alias_dot_gives_columns() {
        let text = "select u. from users u join orders o on o.user_id = u.id";
        let (n, v) = suggest(&meta(), text, "select u.", "app");
        assert_eq!(n, 0);
        assert_eq!(labels(&v), ["id", "user_name", "email"]);
        let (n, v) = suggest(&meta(), text, "select u.us", "app");
        assert_eq!(n, 2);
        assert_eq!(labels(&v), ["user_name"]);
    }

    #[test]
    fn table_dot_and_schema_dot() {
        let (_, v) = suggest(&meta(), "select orders. from orders", "select orders.", "app");
        assert_eq!(labels(&v), ["id", "user_id", "total"]);
        let (_, v) = suggest(&meta(), "select * from billing.", "select * from billing.", "app");
        assert_eq!(labels(&v), ["invoices"]);
    }

    #[test]
    fn plain_prefix_orders_columns_tables_schemas_keywords() {
        let (n, v) = suggest(&meta(), "select * from users where u", "select * from users where u", "app");
        assert_eq!(n, 1);
        assert_eq!(labels(&v), ["user_name", "users", "using", "union", "update"]);
        let (_, v) = suggest(&meta(), "select * from inv", "select * from inv", "app");
        assert_eq!(v[0].insert, "billing.invoices");
    }

    #[test]
    fn typed_word_is_not_suggested_again() {
        let (_, v) = suggest(&meta(), "select * from users", "select * from users", "app");
        assert!(v.is_empty(), "{:?}", labels(&v));
        let (_, v) = suggest(&meta(), "sel", "sel", "app");
        assert_eq!(labels(&v), ["select"]);
    }

    fn hmeta() -> Meta {
        let t = |s: &str, n: &str, cols: &[(&str, &str)]| CompletionTable { schema: s.into(), name: n.into(), columns: cols.iter().map(|(c, ty)| (c.to_string(), ty.to_string())).collect() };
        Meta::new(vec![t("public", "orders", &[("id", "int8"), ("status", "varchar"), ("data", "jsonb"), ("raw", "bytea")]), t("public", "users", &[("id", "int8"), ("name", "text")])])
    }

    #[test]
    fn hint_places() {
        let m = hmeta();
        let at = |before: &str| hint_at(&m, before, before, "app", false);
        let h = at("select * from orders o where o.status = 'pa").unwrap();
        assert_eq!((h.table.as_str(), h.column.as_str(), h.mode, h.quoted, h.typed.as_str()), ("orders", "status", HintMode::Values, true, "pa"));
        let h = at("select * from orders where status in ('paid', '").unwrap();
        assert_eq!((h.column.as_str(), h.quoted), ("status", true));
        let h = at("select * from orders where data -> 'meta' ->> 'sr").unwrap();
        assert_eq!((h.path.clone(), h.mode, h.typed.as_str()), (vec!["meta".to_string()], HintMode::Keys, "sr"));
        let h = at("select * from orders where data->>'kind' = '").unwrap();
        assert_eq!((h.path.clone(), h.mode), (vec!["kind".to_string()], HintMode::Values));
        assert!(!at("select * from orders where id = ").unwrap().quoted);
        assert!(at("select * from orders where raw = '").is_none(), "bytea");
        assert!(at("select * from orders where data = '").is_none(), "whole json");
        assert!(at("select * from orders where ").is_none());
        assert!(at("select * from orders where status = 'paid' and ").is_none());
        assert!(at("select * from users where status = '").is_none(), "not a column of users");
        let h = hint_at(&m, "select * from orders where data->>'$.meta.", "select * from orders where data->>'$.meta.", "app", true).unwrap();
        assert_eq!((h.path.clone(), h.mode), (vec!["meta".to_string()], HintMode::Keys));
        let h = at("select * from orders where data ->> ").unwrap();
        assert_eq!((h.mode, h.quoted, h.op.as_str()), (HintMode::Keys, false, " ->> "));
        let h = at("select * from orders where data @> '").unwrap();
        assert_eq!((h.mode, h.quoted), (HintMode::Pairs, true));
        assert!(hint_at(&m, "select * from orders where data @> '", "select * from orders where data @> '", "app", true).is_none(), "no @> on MySQL");
    }

    #[test]
    fn hint_inserts() {
        let m = hmeta();
        let hs = |v: &[&str]| v.iter().map(|x| qurry_core::hints::Hint { value: x.to_string(), count: 2 }).collect::<Vec<_>>();
        let h = hint_at(&m, "select * from orders where status = 'p", "select * from orders where status = 'p", "app", false).unwrap();
        let v = hint_items(&h, &hs(&["paid", "pa'y", "ready"]), true, false);
        assert_eq!(v.iter().map(|i| i.insert.as_str()).collect::<Vec<_>>(), ["paid'", "pa''y'"]);
        assert_eq!(hint_items(&h, &hs(&["paid"]), false, false)[0].insert, "paid", "closing quote already there");
        let h = hint_at(&m, "select * from orders where status = ", "select * from orders where status = ", "app", false).unwrap();
        assert_eq!(hint_items(&h, &hs(&["paid"]), false, false)[0].insert, "'paid'");
        let h = hint_at(&m, "select * from orders where id = ", "select * from orders where id = ", "app", false).unwrap();
        assert_eq!(hint_items(&h, &hs(&["7"]), false, false)[0].insert, "7");
        let h = hint_at(&m, "select * from orders where data ->> ", "select * from orders where data ->> ", "app", false).unwrap();
        assert_eq!(hint_items(&h, &hs(&["kind"]), false, false)[0].insert, "'kind'");
        let v = hint_items(&h, &hs(&[r#"[{"code": "x"}]"#]), false, true);
        assert_eq!((v[0].insert.as_str(), v[0].label.as_str()), (r#" @> '[{"code": "x"}]'"#, r#"@> '[{"code": "x"}]'"#));
    }

    #[test]
    fn mysql_schema_is_db() {
        let all = Meta::new(vec![CompletionTable { schema: "shop".into(), name: "items".into(), columns: vec![] }]);
        let (_, v) = suggest(&all, "select * from it", "select * from it", "shop");
        assert_eq!(v[0].insert, "items");
    }
}
