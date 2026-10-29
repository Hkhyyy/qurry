//! 테스트 데이터 만들기: 테이블 구조(타입·길이·NOT NULL·PK·UNIQUE·FK·enum·CHECK IN)와 컬럼 이름을 보고 그럴듯한 행을 채운다.
//! FK 는 가리키는 테이블에 이미 있는 값에서 고른다. 자동 번호 · 계산 컬럼은 비워 둔다 (서버가 채운다).
//! 넣기는 한 트랜잭션 (execute_batch): 하나라도 실패하면 아무것도 안 들어간다
use crate::app::{AppState, ensure_pool};
use crate::db::Pool;
use crate::error::{AppError, AppResult};
use crate::store::DbKind;
use rand::Rng;
use serde_json::Value;
use std::collections::HashSet;

/// 한 번에 만들 수 있는 행 수
pub const MAX_ROWS: usize = 100_000;
/// INSERT 한 문장에 넣는 행 수
const BATCH: usize = 200;
/// FK 로 고를 부모 행 (무작위로 이만큼)
const FK_SAMPLE: usize = 1000;

/// 컬럼 구조 (DB 에서 읽은 것)
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Col {
    pub name: String,
    /// 보이는 타입 (varchar(20), numeric(10,2) …)
    pub data_type: String,
    /// 기본 타입 이름 (소문자: int4, varchar, numeric, timestamptz / MySQL: int, varchar, decimal …)
    pub base: String,
    /// 글자 수 · 정밀도
    pub len: Option<u32>,
    pub scale: Option<u32>,
    pub nullable: bool,
    /// 자동 번호 · identity · 계산 컬럼 (넣지 않는다)
    pub auto: bool,
    pub pk: bool,
    /// 혼자서 UNIQUE
    pub unique: bool,
    /// enum · CHECK (col IN (…)) 의 값
    pub choices: Vec<String>,
    pub unsigned: bool,
}

/// 외래 키: 이 컬럼들이 ref_table 의 ref_cols 를 가리킨다
#[derive(Debug, Clone, PartialEq)]
pub struct Fk {
    pub cols: Vec<String>,
    /// SQL 에 쓸 이름 (따옴표 포함)
    pub ref_table: String,
    pub ref_cols: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct TableShape {
    pub cols: Vec<Col>,
    pub fks: Vec<Fk>,
}

/// 칸 하나 (SQL 로 쓸 때 · 미리보기로 보일 때)
#[derive(Debug, Clone, PartialEq)]
enum Cell {
    Null,
    Text(String),
    Num(String),
    Bool(bool),
    /// 이미 SQL 글 (바이너리 등)
    Raw(String, String),
    /// FK 로 고른 값 (DB 에서 읽은 그대로)
    Val(Value),
}

impl Cell {
    fn sql(&self, kind: DbKind) -> String {
        match self {
            Cell::Null => "NULL".into(),
            Cell::Num(n) => n.clone(),
            Cell::Bool(b) => if kind == DbKind::Mysql { if *b { "1" } else { "0" } } else if *b { "TRUE" } else { "FALSE" }.into(),
            Cell::Text(s) => crate::transfer::sql_literal(kind, &Value::String(s.clone())),
            Cell::Raw(sql, _) => sql.clone(),
            Cell::Val(v) => crate::transfer::sql_literal(kind, v),
        }
    }
    fn show(&self) -> Value {
        match self {
            Cell::Null => Value::Null,
            Cell::Num(n) => serde_json::from_str(n).unwrap_or_else(|_| Value::String(n.clone())),
            Cell::Bool(b) => Value::Bool(*b),
            Cell::Text(s) => Value::String(s.clone()),
            Cell::Raw(_, shown) => Value::String(shown.clone()),
            Cell::Val(v) => v.clone(),
        }
    }
}

/// 컬럼을 어떻게 채우는지 (화면에 보인다)
#[derive(Debug, Clone, serde::Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ColPlan {
    pub name: String,
    pub data_type: String,
    pub how: String,
}

/// 미리보기: 컬럼별 채우는 법 + 몇 행 + 경고
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FakePreview {
    pub plan: Vec<ColPlan>,
    pub result: crate::db::QueryResult,
    pub warnings: Vec<String>,
}

// ───────── 값 만들기 ─────────

const FIRST: &[&str] = &["Minjun", "Seoyeon", "Jiho", "Hayoon", "Doyun", "Jiwoo", "Emma", "Liam", "Olivia", "Noah", "Ava", "Lucas", "Mia", "Leo", "Sofia", "Ethan"];
const LAST: &[&str] = &["Kim", "Lee", "Park", "Choi", "Jung", "Kang", "Cho", "Yoon", "Smith", "Brown", "Garcia", "Miller", "Davis", "Wilson", "Tanaka", "Silva"];
const CITY: &[&str] = &["Seoul", "Busan", "Incheon", "Daegu", "Tokyo", "London", "Berlin", "New York", "Toronto", "Sydney"];
const COUNTRY: &[&str] = &["KR", "JP", "US", "GB", "DE", "CA", "AU", "FR"];
const STREET: &[&str] = &["Main St", "Oak Ave", "Teheran-ro", "Gangnam-daero", "Maple Rd", "Park Ln", "Hill St"];
const WORDS: &[&str] = &[
    "alpha", "beta", "order", "sample", "quick", "data", "report", "update", "review", "note", "check", "item", "value", "service", "account", "change", "test", "daily", "batch", "event",
];
const STATUS: &[&str] = &["active", "inactive", "pending"];

/// 이름으로 뜻을 짐작 (소문자 컬럼 이름에 들어 있는 것)
fn has(name: &str, parts: &[&str]) -> bool {
    parts.iter().any(|p| name.contains(p))
}

/// 짧은 낱말은 _ 로 나눈 조각이 같을 때만 (description 의 ip, hotel 의 tel 을 피한다)
fn seg(name: &str, words: &[&str]) -> bool {
    name.split('_').any(|x| words.contains(&x))
}

fn words<R: Rng>(r: &mut R, n: usize) -> String {
    (0..n).map(|_| WORDS[r.gen_range(0..WORDS.len())]).collect::<Vec<_>>().join(" ")
}

fn pick<'a, R: Rng>(r: &mut R, xs: &'a [&'a str]) -> &'a str {
    xs[r.gen_range(0..xs.len())]
}

/// 길이에 맞춰 자른다 (글자 단위)
fn fit(s: String, len: Option<u32>) -> String {
    match len {
        Some(n) if s.chars().count() > n as usize => s.chars().take(n as usize).collect(),
        _ => s,
    }
}

fn base36(mut n: u64) -> String {
    let d = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut out = vec![];
    loop {
        out.push(d[(n % 36) as usize]);
        n /= 36;
        if n == 0 {
            break;
        }
    }
    out.reverse();
    String::from_utf8(out).unwrap()
}

fn is_int(b: &str) -> bool {
    matches!(b, "int2" | "int4" | "int8" | "smallint" | "integer" | "int" | "bigint" | "tinyint" | "mediumint" | "serial" | "bigserial" | "year")
}
fn is_decimal(b: &str) -> bool {
    matches!(b, "numeric" | "decimal" | "money")
}
fn is_float(b: &str) -> bool {
    matches!(b, "float4" | "float8" | "real" | "double precision" | "double" | "float")
}
fn is_text(b: &str) -> bool {
    matches!(b, "varchar" | "bpchar" | "char" | "text" | "citext" | "name" | "tinytext" | "mediumtext" | "longtext" | "character varying" | "character")
}

/// 수의 범위 (타입 크기 · 이름)
fn int_range(c: &Col, n: &str) -> (i64, i64) {
    let max = match c.base.as_str() {
        "int2" | "smallint" => 30_000,
        "tinyint" if c.unsigned => 250,
        "tinyint" => 120,
        "mediumint" => 8_000_000,
        "year" => return (1990, 2030),
        _ => 1_000_000,
    };
    if seg(n, &["age"]) {
        (18, 80)
    } else if seg(n, &["qty", "quantity", "count", "stock", "cnt"]) {
        (0, 100.min(max))
    } else if has(n, &["score", "rating", "rank", "level"]) {
        (0, 100.min(max))
    } else if has(n, &["price", "amount", "total", "cost", "fee", "balance", "salary"]) {
        (100, 100_000.min(max))
    } else {
        (1, max)
    }
}

/// 날짜·시각: 최근 1년 안
fn when<R: Rng>(r: &mut R) -> chrono::NaiveDateTime {
    let now = chrono::Local::now().naive_local();
    now - chrono::Duration::seconds(r.gen_range(0..365 * 86_400))
}

/// unique: 이 번호를 넣어 겹치지 않게 (n = 몇 번째 행 + 시작 번호)
fn value<R: Rng>(r: &mut R, c: &Col, n: u64, kind: DbKind) -> Cell {
    let name = c.name.to_lowercase();
    let b = c.base.as_str();
    let uniq = c.unique || c.pk;
    if !c.choices.is_empty() {
        return Cell::Text(c.choices[r.gen_range(0..c.choices.len())].clone());
    }
    // 참/거짓 (MySQL tinyint(1) 포함)
    if b == "bool" || b == "boolean" || (b == "tinyint" && c.len == Some(1)) || (b == "bit" && c.len.unwrap_or(1) == 1) {
        return Cell::Bool(r.gen_bool(if has(&name, &["deleted", "removed", "banned"]) { 0.1 } else { 0.7 }));
    }
    if is_int(b) {
        if uniq {
            return Cell::Num(n.to_string());
        }
        let (lo, hi) = int_range(c, &name);
        return Cell::Num(r.gen_range(lo..=hi).to_string());
    }
    if is_decimal(b) || is_float(b) {
        let scale = c.scale.unwrap_or(if is_float(b) { 2 } else { 0 }) as i32;
        let whole = c.len.map(|p| p as i32 - scale).unwrap_or(6).clamp(0, 9);
        let (lo, hi) = if seg(&name, &["rate", "ratio", "percent", "pct"]) { (0.0, 1.0) } else { (1.0, 10f64.powi(whole).min(100_000.0) - 1.0) };
        let v = if hi <= lo { 0.0 } else { r.gen_range(lo..hi) };
        return Cell::Num(format!("{v:.prec$}", prec = scale.max(0) as usize));
    }
    match b {
        "uuid" => return Cell::Text(uuid::Uuid::new_v4().to_string()),
        "date" => return Cell::Text(when(r).date().to_string()),
        "timestamp" | "timestamptz" | "datetime" => return Cell::Text(when(r).format("%Y-%m-%d %H:%M:%S").to_string()),
        "time" | "timetz" => return Cell::Text(format!("{:02}:{:02}:{:02}", r.gen_range(0..24), r.gen_range(0..60), r.gen_range(0..60))),
        "interval" => return Cell::Text(format!("{} minutes", r.gen_range(1..600))),
        "json" | "jsonb" => return Cell::Text(format!(r#"{{"key": "{}", "n": {}}}"#, pick(r, WORDS), r.gen_range(0..1000))),
        "inet" | "cidr" => return Cell::Text(format!("10.{}.{}.{}", r.gen_range(0..256), r.gen_range(0..256), r.gen_range(1..255))),
        "bytea" | "blob" | "tinyblob" | "mediumblob" | "longblob" | "binary" | "varbinary" => {
            let hex: String = (0..8).map(|_| format!("{:02X}", r.r#gen::<u8>())).collect();
            return match kind {
                DbKind::Mysql => Cell::Raw(format!("X'{hex}'"), format!("0x{hex}")),
                _ => Cell::Raw(format!("'\\x{hex}'::bytea"), format!("0x{hex}")),
            };
        }
        _ => {}
    }
    if !is_text(b) && !matches!(b, "enum" | "set") {
        // 모르는 타입 (배열 · 공간 · 벡터 …)
        return Cell::Null;
    }
    // 글: 이름으로 짐작
    let s = if has(&name, &["email", "mail"]) {
        format!("{}.{}{}@example.com", pick(r, FIRST).to_lowercase(), pick(r, LAST).to_lowercase(), n)
    } else if has(&name, &["phone", "mobile"]) || seg(&name, &["tel", "fax"]) {
        format!("010-{:04}-{:04}", r.gen_range(0..10_000), if uniq { n % 10_000 } else { r.gen_range(0..10_000) })
    } else if has(&name, &["first_name", "firstname", "given"]) {
        pick(r, FIRST).to_string()
    } else if has(&name, &["last_name", "lastname", "surname", "family"]) {
        pick(r, LAST).to_string()
    } else if has(&name, &["username", "user_name", "login", "nickname", "handle"]) {
        format!("{}{}", pick(r, FIRST).to_lowercase(), n)
    } else if has(&name, &["name"]) && !has(&name, &["file", "host", "table", "column"]) {
        format!("{} {}", pick(r, FIRST), pick(r, LAST))
    } else if has(&name, &["address", "addr", "street"]) {
        format!("{} {}", r.gen_range(1..999), pick(r, STREET))
    } else if has(&name, &["city"]) {
        pick(r, CITY).to_string()
    } else if has(&name, &["country"]) {
        pick(r, COUNTRY).to_string()
    } else if seg(&name, &["zip"]) || has(&name, &["postal", "postcode"]) {
        format!("{:05}", r.gen_range(0..100_000))
    } else if seg(&name, &["url", "link"]) || has(&name, &["website", "homepage"]) {
        format!("https://example.com/{}", base36(n))
    } else if seg(&name, &["ip"]) || has(&name, &["ip_addr", "ipaddr"]) {
        format!("10.{}.{}.{}", r.gen_range(0..256), r.gen_range(0..256), r.gen_range(1..255))
    } else if has(&name, &["status", "state"]) {
        pick(r, STATUS).to_string()
    } else if seg(&name, &["code", "sku", "token", "key", "slug"]) {
        format!("{}-{}", name.split('_').next().unwrap_or("x").chars().take(3).collect::<String>().to_uppercase(), base36(n + 36_u64.pow(3)))
    } else if has(&name, &["title", "subject", "label", "headline"]) {
        let mut w = words(r, 3);
        w[..1].make_ascii_uppercase();
        w
    } else if has(&name, &["desc", "memo", "note", "content", "body", "comment", "message", "text", "bio", "summary"]) {
        let mut w = words(r, 8);
        w[..1].make_ascii_uppercase();
        w + "."
    } else {
        words(r, 2)
    };
    let s = if uniq && !has(&name, &["email", "mail", "username", "user_name", "login", "nickname", "handle", "url", "website", "homepage", "link", "code", "sku", "token", "key", "slug", "phone", "mobile"]) {
        format!("{s}_{}", base36(n))
    } else {
        s
    };
    // 짧은 칸 (char(2) 등) 에 unique 면 끝을 남긴다
    match c.len {
        Some(l) if uniq && s.chars().count() > l as usize => Cell::Text(fit(base36(n), c.len)),
        _ => Cell::Text(fit(s, c.len)),
    }
}

/// 컬럼을 어떻게 채우는지 (미리보기 표의 설명)
fn how(c: &Col, fk: Option<&Fk>, kind: DbKind) -> String {
    if c.auto {
        return "left to the database (auto / generated)".into();
    }
    if let Some(f) = fk {
        return format!("picked from {} ({})", f.ref_table, f.ref_cols.join(", "));
    }
    if !c.choices.is_empty() {
        let shown: Vec<&str> = c.choices.iter().take(5).map(String::as_str).collect();
        return format!("one of: {}{}", shown.join(", "), if c.choices.len() > 5 { ", …" } else { "" });
    }
    let mut r = rand::thread_rng();
    let sample = value(&mut r, c, 1, kind);
    let mut s: String = match sample {
        Cell::Null => if c.nullable { "NULL (type not supported)".into() } else { "can't fill this type".into() },
        Cell::Bool(_) => "true / false".into(),
        _ if c.pk || c.unique => "unique values".into(),
        _ => "random values".into(),
    };
    if c.nullable && !c.pk && !c.unique && sample != Cell::Null {
        s.push_str(" · some NULL");
    }
    s
}

/// 행들을 만든다. fk_rows[i] = fks[i] 가 가리킬 수 있는 값들 (부모 테이블에서 읽은 것). start = unique 번호의 시작.
/// 같은 PK · UNIQUE 가 나오면 다시 만든다 (100 번 해도 안 되면 그만큼만)
fn make_rows<R: Rng>(r: &mut R, shape: &TableShape, fk_rows: &[Vec<Vec<Value>>], n: usize, start: u64, kind: DbKind) -> Vec<Vec<Cell>> {
    let cols: Vec<&Col> = shape.cols.iter().filter(|c| !c.auto).collect();
    let fk_of = |name: &str| shape.fks.iter().enumerate().find_map(|(i, f)| f.cols.iter().position(|c| c == name).map(|j| (i, j)));
    let key_sets: Vec<Vec<usize>> = {
        let pk: Vec<usize> = (0..cols.len()).filter(|&i| cols[i].pk).collect();
        let mut v: Vec<Vec<usize>> = (0..cols.len()).filter(|&i| cols[i].unique && !cols[i].pk).map(|i| vec![i]).collect();
        if !pk.is_empty() {
            v.push(pk);
        }
        v
    };
    let mut seen: Vec<HashSet<String>> = vec![HashSet::new(); key_sets.len()];
    let mut out = Vec::with_capacity(n);
    let mut serial = start;
    'rows: for _ in 0..n {
        for _try in 0..100 {
            serial += 1;
            // FK 는 한 묶음씩 같은 부모 행에서
            let picks: Vec<Option<&Vec<Value>>> = fk_rows.iter().map(|rows| if rows.is_empty() { None } else { Some(&rows[r.gen_range(0..rows.len())]) }).collect();
            let row: Vec<Cell> = cols
                .iter()
                .map(|c| match fk_of(&c.name) {
                    Some((i, j)) => match picks[i] {
                        Some(p) if !(c.nullable && r.gen_bool(0.1)) => Cell::Val(p[j].clone()),
                        _ => Cell::Null,
                    },
                    None if c.nullable && !c.pk && !c.unique && r.gen_bool(0.1) => Cell::Null,
                    None => value(r, c, serial, kind),
                })
                .collect();
            let keys: Vec<String> = key_sets.iter().map(|ks| ks.iter().map(|&i| format!("{:?}", row[i].show())).collect::<Vec<_>>().join("|")).collect();
            // NULL 이 낀 UNIQUE 는 겹쳐도 된다
            let clash = key_sets.iter().zip(&keys).zip(&seen).any(|((ks, k), s)| ks.iter().all(|&i| row[i] != Cell::Null) && s.contains(k));
            if clash {
                continue;
            }
            for ((ks, k), s) in key_sets.iter().zip(keys).zip(seen.iter_mut()) {
                if ks.iter().all(|&i| row[i] != Cell::Null) {
                    s.insert(k);
                }
            }
            out.push(row);
            continue 'rows;
        }
        break;
    }
    out
}

/// 여러 행 INSERT 문장들 (BATCH 행씩)
fn insert_sql(table: &str, shape: &TableShape, rows: &[Vec<Cell>], kind: DbKind) -> Vec<String> {
    let names: Vec<String> = shape.cols.iter().filter(|c| !c.auto).map(|c| crate::sql::quote_ident(kind, &c.name)).collect();
    rows.chunks(BATCH)
        .map(|ch| {
            let vals: Vec<String> = ch.iter().map(|r| format!("({})", r.iter().map(|c| c.sql(kind)).collect::<Vec<_>>().join(", "))).collect();
            format!("INSERT INTO {table} ({}) VALUES\n{}", names.join(", "), vals.join(",\n"))
        })
        .collect()
}

// ───────── 구조 읽기 ─────────

/// '…' 안의 글들 (CHECK (status IN ('a','b')) · enum('a','b'))
fn quoted_literals(s: &str) -> Vec<String> {
    let re = regex::Regex::new(r"'((?:[^']|'')*)'").unwrap();
    re.captures_iter(s).map(|c| c[1].replace("''", "'")).collect()
}

/// "varchar(20)" · "numeric(10,2)" · "decimal(10,2) unsigned" → (20 / 10, 2)
fn type_args(t: &str) -> (Option<u32>, Option<u32>) {
    let Some(a) = t.find('(') else { return (None, None) };
    let Some(b) = t[a..].find(')') else { return (None, None) };
    let mut it = t[a + 1..a + b].split(',').map(|x| x.trim().parse::<u32>().ok());
    (it.next().flatten(), it.next().flatten())
}

pub(crate) fn qualified(kind: DbKind, schema: &str, table: &str) -> String {
    let q = |s: &str| crate::sql::quote_ident(kind, s);
    if schema.is_empty() { q(table) } else { format!("{}.{}", q(schema), q(table)) }
}

async fn shape(pool: &Pool, kind: DbKind, schema: &str, table: &str) -> AppResult<TableShape> {
    let rel = qualified(kind, schema, table);
    match pool {
        Pool::Pg(p) => {
            let cols: Vec<(String, String, String, bool, bool, Vec<String>)> = sqlx::query_as(
                "SELECT a.attname::text, format_type(a.atttypid, a.atttypmod), (CASE WHEN t.typtype = 'd' THEN bt.typname ELSE t.typname END)::text, \
                        NOT a.attnotnull, \
                        coalesce(a.attidentity <> '' OR a.attgenerated <> '' OR pg_get_expr(d.adbin, d.adrelid) LIKE 'nextval(%', false), \
                        coalesce((SELECT array_agg(e.enumlabel::text ORDER BY e.enumsortorder) FROM pg_enum e WHERE e.enumtypid = a.atttypid), '{}') \
                 FROM pg_attribute a JOIN pg_type t ON t.oid = a.atttypid LEFT JOIN pg_type bt ON bt.oid = t.typbasetype \
                 LEFT JOIN pg_attrdef d ON d.adrelid = a.attrelid AND d.adnum = a.attnum \
                 WHERE a.attrelid = $1::regclass AND a.attnum > 0 AND NOT a.attisdropped ORDER BY a.attnum",
            )
            .bind(&rel)
            .fetch_all(p)
            .await?;
            let keys: Vec<(String, bool, i16)> = sqlx::query_as(
                "SELECT a.attname::text, i.indisprimary, i.indnatts FROM pg_index i JOIN pg_attribute a ON a.attrelid = i.indrelid AND a.attnum = ANY(i.indkey) \
                 WHERE i.indrelid = $1::regclass AND i.indisunique AND i.indpred IS NULL",
            )
            .bind(&rel)
            .fetch_all(p)
            .await?;
            let checks: Vec<(String, String)> = sqlx::query_as(
                "SELECT a.attname::text, pg_get_constraintdef(c.oid) FROM pg_constraint c JOIN pg_attribute a ON a.attrelid = c.conrelid AND a.attnum = c.conkey[1] \
                 WHERE c.conrelid = $1::regclass AND c.contype = 'c' AND array_length(c.conkey, 1) = 1",
            )
            .bind(&rel)
            .fetch_all(p)
            .await?;
            let fks: Vec<(String, Vec<String>, Vec<String>)> = sqlx::query_as(
                "SELECT c.confrelid::regclass::text, \
                   array(SELECT a.attname::text FROM unnest(c.conkey) WITH ORDINALITY k(n, o) JOIN pg_attribute a ON a.attrelid = c.conrelid AND a.attnum = k.n ORDER BY k.o), \
                   array(SELECT a.attname::text FROM unnest(c.confkey) WITH ORDINALITY k(n, o) JOIN pg_attribute a ON a.attrelid = c.confrelid AND a.attnum = k.n ORDER BY k.o) \
                 FROM pg_constraint c WHERE c.conrelid = $1::regclass AND c.contype = 'f'",
            )
            .bind(&rel)
            .fetch_all(p)
            .await?;
            let cols = cols
                .into_iter()
                .map(|(name, data_type, base, nullable, auto, enums)| {
                    let (len, scale) = type_args(&data_type);
                    let pk = keys.iter().any(|(k, p, _)| *k == name && *p);
                    let unique = keys.iter().any(|(k, p, n)| *k == name && !*p && *n == 1) || (pk && keys.iter().filter(|(_, p, _)| *p).count() == 1);
                    let mut choices = enums;
                    if choices.is_empty() {
                        choices = checks.iter().filter(|(c, _)| *c == name).flat_map(|(_, def)| if def.contains("ANY") || def.contains(" IN ") { quoted_literals(def) } else { vec![] }).collect();
                    }
                    Col { name, data_type, base, len, scale, nullable, auto, pk, unique, choices, unsigned: false }
                })
                .collect();
            Ok(TableShape { cols, fks: fks.into_iter().map(|(ref_table, cols, ref_cols)| Fk { cols, ref_table, ref_cols }).collect() })
        }
        Pool::My(p) => {
            let cols: Vec<(String, String, String, i64, i64, String)> = sqlx::query_as(
                "SELECT CAST(COLUMN_NAME AS CHAR), CAST(COLUMN_TYPE AS CHAR), CAST(DATA_TYPE AS CHAR), CAST(IS_NULLABLE = 'YES' AS SIGNED), \
                        CAST(EXTRA LIKE '%auto_increment%' OR EXTRA LIKE '%GENERATED%' AS SIGNED), CAST(COLUMN_KEY AS CHAR) \
                 FROM information_schema.COLUMNS WHERE TABLE_SCHEMA = COALESCE(NULLIF(?, ''), DATABASE()) AND TABLE_NAME = ? ORDER BY ORDINAL_POSITION",
            )
            .bind(schema)
            .bind(table)
            .fetch_all(p)
            .await?;
            let uniq: Vec<(String,)> = sqlx::query_as(
                "SELECT CAST(MAX(COLUMN_NAME) AS CHAR) FROM information_schema.STATISTICS WHERE TABLE_SCHEMA = COALESCE(NULLIF(?, ''), DATABASE()) AND TABLE_NAME = ? \
                 AND NON_UNIQUE = 0 AND INDEX_NAME <> 'PRIMARY' GROUP BY INDEX_NAME HAVING COUNT(*) = 1",
            )
            .bind(schema)
            .bind(table)
            .fetch_all(p)
            .await?;
            let fk_rows: Vec<(String, String, String, String, String)> = sqlx::query_as(
                "SELECT CAST(CONSTRAINT_NAME AS CHAR), CAST(COLUMN_NAME AS CHAR), CAST(REFERENCED_TABLE_SCHEMA AS CHAR), CAST(REFERENCED_TABLE_NAME AS CHAR), CAST(REFERENCED_COLUMN_NAME AS CHAR) \
                 FROM information_schema.KEY_COLUMN_USAGE WHERE TABLE_SCHEMA = COALESCE(NULLIF(?, ''), DATABASE()) AND TABLE_NAME = ? AND REFERENCED_TABLE_NAME IS NOT NULL \
                 ORDER BY CONSTRAINT_NAME, ORDINAL_POSITION",
            )
            .bind(schema)
            .bind(table)
            .fetch_all(p)
            .await?;
            let mut fks: Vec<(String, Fk)> = vec![];
            for (cname, col, rs, rt, rc) in fk_rows {
                match fks.iter_mut().find(|(n, _)| *n == cname) {
                    Some((_, f)) => {
                        f.cols.push(col);
                        f.ref_cols.push(rc);
                    }
                    None => fks.push((cname, Fk { cols: vec![col], ref_table: qualified(DbKind::Mysql, &rs, &rt), ref_cols: vec![rc] })),
                }
            }
            let pks = cols.iter().filter(|c| c.5 == "PRI").count();
            let cols = cols
                .into_iter()
                .map(|(name, ctype, base, nullable, auto, key)| {
                    let lower = ctype.to_lowercase();
                    let (len, scale) = type_args(&lower);
                    let base = base.to_lowercase();
                    let choices = if base == "enum" || base == "set" { quoted_literals(&ctype) } else { vec![] };
                    let pk = key == "PRI";
                    Col { unique: uniq.iter().any(|(u,)| *u == name) || (pk && pks == 1), name, data_type: ctype, base, len, scale, nullable: nullable != 0, auto: auto != 0, pk, choices, unsigned: lower.contains("unsigned") }
                })
                .collect();
            Ok(TableShape { cols, fks: fks.into_iter().map(|x| x.1).collect() })
        }
        Pool::Redis(..) => Err(AppError::Msg("Redis has no tables".into())),
        Pool::Plugin(c) => Err(AppError::Msg(format!("Test data is not available for the {} driver yet", c.driver))),
    }
}

/// FK 가 가리킬 값들 (부모 테이블에서 무작위로). 자기 자신을 가리키면 이미 있는 행에서
async fn fk_values(pool: &Pool, kind: DbKind, f: &Fk) -> AppResult<Vec<Vec<Value>>> {
    let q = |s: &str| crate::sql::quote_ident(kind, s);
    let cols: Vec<String> = f.ref_cols.iter().map(|c| q(c)).collect();
    let rand = if kind == DbKind::Mysql { "RAND()" } else { "random()" };
    let not_null: Vec<String> = cols.iter().map(|c| format!("{c} IS NOT NULL")).collect();
    // DISTINCT 와 ORDER BY random() 은 PG 에서 같이 못 쓴다 → 감싼다
    let sql = format!("SELECT * FROM (SELECT DISTINCT {} FROM {} WHERE {}) qurry_fk ORDER BY {rand} LIMIT {FK_SAMPLE}", cols.join(", "), f.ref_table, not_null.join(" AND "));
    Ok(pool.run_query(&sql, FK_SAMPLE).await?.rows)
}

/// 정수 PK · UNIQUE 의 시작 번호: 지금 가장 큰 값 다음 (없으면 0)
async fn start_number(pool: &Pool, kind: DbKind, rel: &str, shape: &TableShape) -> u64 {
    let ints: Vec<String> = shape.cols.iter().filter(|c| !c.auto && (c.pk || c.unique) && is_int(&c.base)).map(|c| format!("coalesce(max({}), 0)", crate::sql::quote_ident(kind, &c.name))).collect();
    if ints.is_empty() {
        return rand::thread_rng().gen_range(1000..1_000_000);
    }
    let greatest = if ints.len() == 1 { ints[0].clone() } else { format!("greatest({})", ints.join(", ")) };
    let sql = format!("SELECT CAST({greatest} AS CHAR(30))");
    let sql = if kind == DbKind::Postgres { sql.replace("AS CHAR(30)", "AS text") } else { sql };
    pool.run_query(&format!("{sql} FROM {rel}"), 1)
        .await
        .ok()
        .and_then(|r| r.rows.first().and_then(|x| x.first()).and_then(|v| v.as_str().and_then(|s| s.parse::<f64>().ok()).or(v.as_f64())))
        .map(|m| m.max(0.0) as u64)
        .unwrap_or(0)
}

struct Prepared {
    kind: DbKind,
    rel: String,
    shape: TableShape,
    fk_rows: Vec<Vec<Vec<Value>>>,
    start: u64,
    warnings: Vec<String>,
}

impl AppState {
    async fn fake_prepare(&self, id: &str, schema: &str, table: &str) -> AppResult<Prepared> {
        let kind = self.kind_of(id)?;
        let pool = ensure_pool(self, id).await?;
        let shape = shape(&pool, kind, schema, table).await?;
        if shape.cols.is_empty() {
            return Err(AppError::Msg(format!("Couldn't find table {schema}.{table}")));
        }
        let rel = qualified(kind, schema, table);
        let mut warnings = vec![];
        let mut fk_rows = vec![];
        for f in &shape.fks {
            let rows = fk_values(&pool, kind, f).await?;
            let required = f.cols.iter().any(|c| shape.cols.iter().any(|x| x.name == *c && !x.nullable));
            if rows.is_empty() {
                if required {
                    return Err(AppError::Msg(format!("{} has no rows to point to — fill it first ({} → {})", f.ref_table, f.cols.join(", "), f.ref_table)));
                }
                warnings.push(format!("{} is empty: {} will be NULL", f.ref_table, f.cols.join(", ")));
            }
            fk_rows.push(rows);
        }
        let fk_cols: HashSet<&str> = shape.fks.iter().flat_map(|f| f.cols.iter().map(String::as_str)).collect();
        {
            let mut probe = rand::thread_rng();
            for c in shape.cols.iter().filter(|c| !c.auto && !fk_cols.contains(c.name.as_str()) && c.choices.is_empty()) {
                if value(&mut probe, c, 1, kind) == Cell::Null {
                    if c.nullable {
                        warnings.push(format!("{} ({}) will be NULL — this type isn't generated", c.name, c.data_type));
                    } else {
                        return Err(AppError::Msg(format!("Can't generate values for {} ({}) — it is NOT NULL", c.name, c.data_type)));
                    }
                }
            }
        }
        let start = start_number(&pool, kind, &rel, &shape).await;
        Ok(Prepared { kind, rel, shape, fk_rows, start, warnings })
    }

    /// 미리보기: 컬럼마다 채우는 법과 예시 몇 행 (아무것도 넣지 않는다)
    pub async fn fake_preview(&self, id: String, schema: String, table: String, rows: usize) -> AppResult<FakePreview> {
        let p = self.fake_prepare(&id, &schema, &table).await?;
        let fk_of = |c: &Col| p.shape.fks.iter().find(|f| f.cols.contains(&c.name));
        let plan = p.shape.cols.iter().map(|c| ColPlan { name: c.name.clone(), data_type: c.data_type.clone(), how: how(c, fk_of(c), p.kind) }).collect();
        let made = make_rows(&mut rand::thread_rng(), &p.shape, &p.fk_rows, rows.min(50), p.start, p.kind);
        let cols: Vec<&Col> = p.shape.cols.iter().filter(|c| !c.auto).collect();
        let result = crate::db::QueryResult {
            columns: cols.iter().map(|c| crate::db::ColumnInfo { name: c.name.clone(), type_name: c.data_type.clone() }).collect(),
            rows: made.iter().map(|r| r.iter().map(Cell::show).collect()).collect(),
            rows_affected: 0,
            truncated: false,
            elapsed_ms: 0,
            has_result_set: true,
            in_transaction: false,
        };
        Ok(FakePreview { plan, result, warnings: p.warnings })
    }

    /// rows 행을 만들어 한 트랜잭션으로 넣는다. 넣은 행 수 (PK · UNIQUE 가 겹치지 않게 만들 수 없으면 그보다 적다)
    pub async fn fake_fill(&self, id: String, schema: String, table: String, rows: usize) -> AppResult<u64> {
        if rows == 0 || rows > MAX_ROWS {
            return Err(AppError::Msg(format!("Choose 1 to {MAX_ROWS} rows")));
        }
        let p = self.fake_prepare(&id, &schema, &table).await?;
        let made = make_rows(&mut rand::thread_rng(), &p.shape, &p.fk_rows, rows, p.start, p.kind);
        if made.is_empty() {
            return Err(AppError::Msg("Couldn't make rows that don't clash with the keys".into()));
        }
        self.execute_batch(id, insert_sql(&p.rel, &p.shape, &made, p.kind), false).await?;
        Ok(made.len() as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    fn col(name: &str, base: &str) -> Col {
        Col { name: name.into(), data_type: base.into(), base: base.into(), ..Default::default() }
    }

    #[test]
    fn values_follow_type_and_name() {
        let mut r = rand::rngs::StdRng::seed_from_u64(7);
        let text = |c: &Col, r: &mut rand::rngs::StdRng| match value(r, c, 42, DbKind::Postgres) {
            Cell::Text(s) => s,
            other => panic!("{other:?}"),
        };
        assert!(text(&col("email", "varchar"), &mut r).ends_with("42@example.com"));
        assert!(text(&col("phone", "text"), &mut r).starts_with("010-"));
        let mut short = col("name", "varchar");
        short.len = Some(5);
        assert!(text(&short, &mut r).chars().count() <= 5);
        let mut status = col("status", "text");
        status.choices = vec!["paid".into(), "refunded".into()];
        assert!(["paid", "refunded"].contains(&text(&status, &mut r).as_str()));
        assert!(text(&col("created_at", "timestamptz"), &mut r).len() == 19);
        assert!(uuid::Uuid::parse_str(&text(&col("ext_id", "uuid"), &mut r)).is_ok());
        // 수: 정밀도 · 소수 자리
        let mut price = col("price", "numeric");
        (price.len, price.scale) = (Some(5), Some(2));
        for _ in 0..50 {
            let Cell::Num(n) = value(&mut r, &price, 1, DbKind::Postgres) else { panic!() };
            let v: f64 = n.parse().unwrap();
            assert!(v < 1000.0 && n.split('.').nth(1).unwrap().len() == 2, "{n}");
        }
        let mut id = col("id", "int4");
        id.pk = true;
        assert_eq!(value(&mut r, &id, 43, DbKind::Postgres), Cell::Num("43".into()));
        let mut flag = col("is_active", "tinyint");
        flag.len = Some(1);
        assert!(matches!(value(&mut r, &flag, 1, DbKind::Mysql), Cell::Bool(_)));
        assert_eq!(value(&mut r, &col("shape", "geometry"), 1, DbKind::Mysql), Cell::Null);
        assert_eq!(Cell::Bool(true).sql(DbKind::Mysql), "1");
        assert_eq!(Cell::Text("it's".into()).sql(DbKind::Postgres), "'it''s'");
    }

    /// PK · UNIQUE 는 겹치지 않고, FK 는 부모 값에서, NOT NULL 은 비지 않는다
    #[test]
    fn rows_respect_keys() {
        let mut r = rand::rngs::StdRng::seed_from_u64(1);
        let mut id = col("id", "int4");
        (id.auto, id.pk, id.unique) = (true, true, true);
        let mut email = col("email", "varchar");
        email.unique = true;
        let mut user_id = col("user_id", "int8");
        user_id.pk = true;
        let mut role_id = col("role_id", "int8");
        role_id.pk = true;
        let shape = TableShape {
            cols: vec![id, email, user_id, role_id, col("memo", "text")],
            fks: vec![
                Fk { cols: vec!["user_id".into()], ref_table: "users".into(), ref_cols: vec!["id".into()] },
                Fk { cols: vec!["role_id".into()], ref_table: "roles".into(), ref_cols: vec!["id".into()] },
            ],
        };
        let users: Vec<Vec<Value>> = (1..=5).map(|i| vec![Value::from(i)]).collect();
        let roles: Vec<Vec<Value>> = (1..=3).map(|i| vec![Value::from(i)]).collect();
        // (user, role) 조합은 15 개뿐 → 20 행을 바라도 15 행
        let rows = make_rows(&mut r, &shape, &[users, roles], 20, 0, DbKind::Postgres);
        assert_eq!(rows.len(), 15);
        let pairs: HashSet<String> = rows.iter().map(|x| format!("{:?}{:?}", x[1], x[2])).collect();
        assert_eq!(pairs.len(), 15);
        let emails: HashSet<String> = rows.iter().map(|x| format!("{:?}", x[0])).collect();
        assert_eq!(emails.len(), 15);
        let sql = insert_sql("t", &shape, &rows[..2], DbKind::Postgres);
        assert!(sql[0].starts_with("INSERT INTO t (email, user_id, role_id, memo) VALUES\n("), "{}", sql[0]);
        assert_eq!(insert_sql("t", &shape, &rows, DbKind::Postgres).len(), 1);
    }

    #[test]
    fn reads_type_args_and_literals() {
        assert_eq!(type_args("character varying(20)"), (Some(20), None));
        assert_eq!(type_args("decimal(10,2) unsigned"), (Some(10), Some(2)));
        assert_eq!(type_args("text"), (None, None));
        assert_eq!(quoted_literals("CHECK ((status = ANY (ARRAY['paid'::text, 'it''s'::text])))"), vec!["paid", "it's"]);
        assert_eq!(quoted_literals("enum('a','b')"), vec!["a", "b"]);
    }

    // ───────── 실제 서버 ─────────

    async fn state(kind: DbKind, env: &str) -> Option<AppState> {
        let e = std::env::var(env).ok()?;
        let p: Vec<&str> = e.split(':').collect();
        let (user, pass, db) = if kind == DbKind::Mysql { (p[2], Some(p[3].to_string()), p[4]) } else { (p[2], None, p[3]) };
        let c = crate::store::ConnectionConfig {
            id: "d".into(), name: "d".into(), kind, host: p[0].into(), port: p[1].parse().unwrap(), user: user.into(), database: db.into(),
            color: None, ssh: None, password_saved: false, ssh_secret_saved: false, scope: Default::default(), read_only: false, options: Default::default(), ssl: None, redis: None,
        };
        let st = crate::open_state(Some(std::env::temp_dir().join(format!("qurry-fake-{}", uuid::Uuid::new_v4())))).unwrap();
        st.save_connection(c, pass, None).await.unwrap();
        Some(st)
    }

    /// 실제 테이블에 넣으면 서버의 제약 (NOT NULL · 길이 · UNIQUE · FK · CHECK · enum) 을 다 지난다
    async fn fills_real_tables(kind: DbKind, env: &str, ddl: &[&str], schema: &str) {
        let Some(st) = state(kind, env).await else { return };
        let q = |s: &str| st.run_query("d".into(), s.into(), None, None);
        q("drop table if exists qf_items").await.unwrap();
        q("drop table if exists qf_users").await.unwrap();
        for d in ddl {
            q(d).await.unwrap();
        }
        // 부모가 비어 있으면 NOT NULL FK 는 못 채운다
        let e = st.fake_fill("d".into(), schema.into(), "qf_items".into(), 10).await.unwrap_err().to_string();
        assert!(e.contains("fill it first"), "{e}");
        assert_eq!(st.fake_fill("d".into(), schema.into(), "qf_users".into(), 300).await.unwrap(), 300);
        assert_eq!(st.fake_fill("d".into(), schema.into(), "qf_users".into(), 300).await.unwrap(), 300, "second fill must not clash with existing keys");
        assert_eq!(st.fake_fill("d".into(), schema.into(), "qf_items".into(), 1000).await.unwrap(), 1000);
        let pv = st.fake_preview("d".into(), schema.into(), "qf_items".into(), 5).await.unwrap();
        assert_eq!(pv.result.rows.len(), 5);
        assert!(pv.plan.iter().any(|c| c.name == "user_id" && c.how.contains("qf_users")), "{:?}", pv.plan);
        assert!(pv.plan.iter().any(|c| c.name == "id" && c.how.contains("auto")), "{:?}", pv.plan);
        let n = q("select count(*) from qf_items i join qf_users u on u.id = i.user_id").await.unwrap();
        assert_eq!(n.rows[0][0].to_string().trim_matches('"'), "1000");
        q("drop table qf_items").await.unwrap();
        q("drop table qf_users").await.unwrap();
    }

    #[tokio::test]
    #[ignore]
    async fn pg_fills_real_tables() {
        fills_real_tables(
            DbKind::Postgres,
            "QURRY_TEST_PG",
            &[
                "drop type if exists qf_status",
                "create type qf_status as enum ('new', 'paid', 'shipped')",
                "create table qf_users (id int primary key, email varchar(60) not null unique, name varchar(12) not null, age smallint check (age >= 0), \
                 tier text not null check (tier in ('free', 'pro')), joined date, active boolean not null, meta jsonb, ip inet, avatar bytea, tags text[])",
                "create table qf_items (id bigint generated always as identity primary key, user_id int not null references qf_users(id), \
                 status qf_status not null, price numeric(8,2) not null check (price > 0), qty int not null, code char(8) unique, created_at timestamptz not null default now(), note text)",
            ],
            "public",
        )
        .await;
    }

    #[tokio::test]
    #[ignore]
    async fn mysql_fills_real_tables() {
        fills_real_tables(
            DbKind::Mysql,
            "QURRY_TEST_MY",
            &[
                "create table qf_users (id int primary key, email varchar(60) not null unique, name varchar(12) not null, age tinyint unsigned, \
                 tier enum('free','pro') not null, joined date, active tinyint(1) not null, meta json, avatar blob, location point)",
                "create table qf_items (id bigint auto_increment primary key, user_id int not null, status set('new','paid') not null, price decimal(8,2) not null, \
                 qty int not null, code char(8) unique, created_at datetime not null default current_timestamp, note text, foreign key (user_id) references qf_users(id))",
            ],
            "",
        )
        .await;
    }
}
