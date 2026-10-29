//! 운영 데이터 일부 복사: 시작 테이블의 행 (WHERE) 과 FK 로 이어진 행을 모아 다른 Data Source (같은 종류) 로 넣는다.
//! - 부모 (이 행이 가리키는 행) 는 끝까지 따라간다 — 넣었을 때 FK 가 깨지지 않게
//! - 자식 (이 행을 가리키는 행) 은 depth 단계까지 (그 자식의 부모도 같이)
//! - 개인정보로 보이는 컬럼 (이메일 · 전화 · 사람 이름 · 주소 · 주민번호 · 카드 · 비밀번호 · 생일 · IP) 은 읽을 때 SQL 로 가린다.
//!   md5 로 가려서 같은 값은 늘 같은 값이 된다. PK · FK 에 쓰는 컬럼은 가리지 않는다 (관계가 끊긴다)
//! - 대상에는 한 트랜잭션으로, 이미 있는 행은 건너뛴다 (PG: ON CONFLICT DO NOTHING · MySQL: ON DUPLICATE KEY UPDATE).
//!   값을 잃지 않게 PG 는 행을 JSON 으로 옮기고 (jsonb_populate_recordset), MySQL 은 글자 / HEX 로 옮긴다
use crate::app::{AppState, ensure_pool};
use crate::db::Pool;
use crate::erd::{Erd, ErdColumn};
use crate::error::{AppError, AppResult};
use crate::store::DbKind;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

/// 모든 테이블을 합쳐 이만큼까지 (넘으면 WHERE 를 좁히라고)
pub const MAX_ROWS: usize = 50_000;
/// IN (…) 한 번에 넣는 키 수
const KEYS_PER_QUERY: usize = 500;
/// INSERT 한 문장에 넣는 행 수
const ROWS_PER_INSERT: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Mask {
    Email,
    Phone,
    Name,
    Address,
    Secret,
    Birth,
    Ip,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MaskedColumn {
    pub column: String,
    pub mask: Mask,
}

/// 테이블 하나: 몇 행 · 가린 컬럼
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubsetTable {
    pub schema: String,
    pub table: String,
    pub rows: usize,
    pub masked: Vec<MaskedColumn>,
    /// 어떻게 들어왔는지: start · parent of x · child of x
    pub reason: String,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubsetPlan {
    /// 넣는 순서 (부모 먼저)
    pub tables: Vec<SubsetTable>,
    pub total: usize,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubsetSpec {
    pub schema: String,
    pub table: String,
    /// 시작 행 조건 (WHERE 뒤에 오는 글). 비면 전부
    #[serde(default)]
    pub filter: String,
    /// 자식을 몇 단계까지 (0 = 부모만)
    #[serde(default)]
    pub depth: u32,
    /// 개인정보 가리기
    #[serde(default = "yes")]
    pub mask: bool,
}

fn yes() -> bool {
    true
}

type Key = (String, String);

/// 읽은 행: PG 는 JSON 글 (넣을 때 그대로), MySQL 은 컬럼 순서대로 글 / NULL (바이너리는 HEX)
#[derive(Debug, Clone)]
struct Row {
    pg_json: Option<String>,
    cells: Vec<Value>,
}

struct Table {
    cols: Vec<ErdColumn>,
    masks: Vec<(usize, Mask)>,
    /// PK (없으면 행 전체) → 행
    rows: BTreeMap<String, Row>,
    reason: String,
}

// ───────── 가리기 ─────────

fn seg(name: &str, words: &[&str]) -> bool {
    name.split('_').any(|x| words.contains(&x))
}

fn texty(t: &str) -> bool {
    let t = t.to_lowercase();
    ["char", "text", "citext", "inet", "string"].iter().any(|x| t.contains(x)) && !t.contains("[]")
}

fn datey(t: &str) -> bool {
    let t = t.to_lowercase();
    t.starts_with("date") || t.starts_with("timestamp")
}

/// 이름 · 타입으로 개인정보인지 (사람 이름 "name" 은 사람 테이블에서만)
pub fn mask_for(table: &str, col: &str, data_type: &str) -> Option<Mask> {
    let (t, n) = (table.to_lowercase(), col.to_lowercase());
    if datey(data_type) {
        return (n.contains("birth") || seg(&n, &["dob", "bday"])).then_some(Mask::Birth);
    }
    if !texty(data_type) {
        return None;
    }
    let people = ["user", "member", "customer", "employee", "person", "people", "profile", "contact", "account", "staff", "client", "patient", "student", "buyer", "seller"].iter().any(|p| t.contains(p));
    Some(if n.contains("email") || n.contains("e_mail") {
        Mask::Email
    } else if n.contains("phone") || n.contains("mobile") || seg(&n, &["tel", "fax", "hp"]) {
        Mask::Phone
    } else if ["first_name", "last_name", "full_name", "real_name", "given_name", "family_name", "surname", "middle_name", "contact_name", "recipient_name", "receiver_name", "customer_name", "member_name", "owner_name", "holder_name"]
        .iter()
        .any(|x| n.contains(x))
        || (people && (n == "name" || n == "user_name" || n == "username" || n == "nickname"))
    {
        Mask::Name
    } else if n.contains("address") || seg(&n, &["addr", "street", "zip", "zipcode", "postal", "postcode"]) {
        Mask::Address
    } else if n.contains("password") || n.contains("passwd") || n.contains("secret") || n.contains("token") || n.contains("api_key") || n.contains("apikey")
        || seg(&n, &["pwd", "salt", "otp", "ssn", "rrn", "jumin", "passport", "iban", "card", "cvc", "cvv"])
        || ["resident", "national_id", "tax_id", "card_no", "card_number", "account_no", "account_number", "bank_account", "license_no"].iter().any(|x| n.contains(x))
    {
        Mask::Secret
    } else if n.contains("birth") || seg(&n, &["dob"]) {
        Mask::Birth
    } else if seg(&n, &["ip"]) || n.contains("ip_addr") || n.contains("ipaddr") {
        Mask::Ip
    } else {
        return None;
    })
}

/// 가린 값을 만드는 SQL (v = 컬럼). NULL 은 NULL 그대로
fn mask_sql(kind: DbKind, m: Mask, v: &str) -> String {
    let e = match kind {
        DbKind::Mysql => {
            let h = format!("MD5({v})");
            match m {
                Mask::Email => format!("CONCAT('user_', LEFT({h}, 10), '@example.com')"),
                Mask::Phone => format!("CONCAT('010-', LPAD(CONV(SUBSTR({h}, 1, 6), 16, 10) % 10000, 4, '0'), '-', LPAD(CONV(SUBSTR({h}, 7, 6), 16, 10) % 10000, 4, '0'))"),
                Mask::Name => format!("CONCAT('User ', LEFT({h}, 4))"),
                Mask::Address => format!("CONCAT('Address ', LEFT({h}, 4))"),
                Mask::Secret => format!("LEFT({h}, 8)"),
                Mask::Birth => format!("CAST(MAKEDATE(YEAR({v}), 1) AS CHAR)"),
                Mask::Ip => format!("CONCAT('10.0.', CONV(SUBSTR({h}, 1, 2), 16, 10), '.', CONV(SUBSTR({h}, 3, 2), 16, 10))"),
            }
        }
        _ => {
            let h = format!("md5({v}::text)");
            let num = |from: usize| format!("lpad(((('x' || substr({h}, {from}, 6))::bit(24)::int) % 10000)::text, 4, '0')");
            match m {
                Mask::Email => format!("'user_' || left({h}, 10) || '@example.com'"),
                Mask::Phone => format!("'010-' || {} || '-' || {}", num(1), num(7)),
                Mask::Name => format!("'User ' || left({h}, 4)"),
                Mask::Address => format!("'Address ' || left({h}, 4)"),
                Mask::Secret => format!("left({h}, 8)"),
                Mask::Birth => format!("date_trunc('year', {v})"),
                Mask::Ip => format!("'10.0.' || (('x' || substr({h}, 1, 2))::bit(8)::int) || '.' || (('x' || substr({h}, 3, 2))::bit(8)::int)"),
            }
        }
    };
    format!("CASE WHEN {v} IS NULL THEN NULL ELSE {e} END")
}

// ───────── 읽기 ─────────

fn qualified(kind: DbKind, k: &Key) -> String {
    crate::fake::qualified(kind, &k.0, &k.1)
}

fn binary(t: &str) -> bool {
    let t = t.to_lowercase();
    t.contains("blob") || t.contains("binary") || t.starts_with("bit") || ["geometry", "point", "linestring", "polygon", "multipoint", "multilinestring", "multipolygon", "geometrycollection"].iter().any(|g| t.starts_with(g))
}

/// 행을 읽는 SELECT (가리기 포함)
fn select_sql(kind: DbKind, key: &Key, t: &Table, cond: &str, limit: usize) -> String {
    let q = |s: &str| crate::sql::quote_ident(kind, s);
    match kind {
        DbKind::Postgres => {
            let base = format!("SELECT * FROM {} WHERE {cond} LIMIT {limit}", qualified(kind, key));
            if t.masks.is_empty() {
                return format!("SELECT to_jsonb(qurry_x)::text FROM ({base}) qurry_x");
            }
            let over: Vec<String> = t.masks.iter().map(|(i, m)| format!("{}, {}", crate::transfer::sql_literal(kind, &Value::String(t.cols[*i].name.clone())), mask_sql(kind, *m, &format!("qurry_x.{}", q(&t.cols[*i].name))))).collect();
            format!("SELECT (to_jsonb(qurry_x) || jsonb_build_object({}))::text FROM ({base}) qurry_x", over.join(", "))
        }
        _ => {
            let exprs: Vec<String> = t
                .cols
                .iter()
                .enumerate()
                .map(|(i, c)| match t.masks.iter().find(|(j, _)| *j == i) {
                    Some((_, m)) => mask_sql(kind, *m, &q(&c.name)),
                    None if binary(&c.data_type) => format!("HEX({})", q(&c.name)),
                    None => format!("CAST({} AS CHAR)", q(&c.name)),
                })
                .collect();
            format!("SELECT {} FROM {} WHERE {cond} LIMIT {limit}", exprs.join(", "), qualified(kind, key))
        }
    }
}

/// 컬럼 값 (키 비교용)
fn cell(kind: DbKind, t: &Table, r: &Row, col: &str) -> Value {
    match kind {
        DbKind::Postgres => r.pg_json.as_deref().and_then(|j| serde_json::from_str::<serde_json::Map<String, Value>>(j).ok()).and_then(|m| m.get(col).cloned()).unwrap_or(Value::Null),
        _ => t.cols.iter().position(|c| c.name == col).map(|i| r.cells[i].clone()).unwrap_or(Value::Null),
    }
}

/// 비교에 쓸 SQL 값 (PG JSON 의 객체 · 배열은 글로)
fn lit(kind: DbKind, v: &Value) -> String {
    match v {
        Value::Object(_) | Value::Array(_) => crate::transfer::sql_literal(kind, &Value::String(v.to_string())),
        v => crate::transfer::sql_literal(kind, v),
    }
}

/// (a, b) IN ((1, 2), …) — 한 컬럼이면 a IN (1, …)
fn in_cond(kind: DbKind, cols: &[String], tuples: &[Vec<Value>]) -> String {
    let q = |s: &str| crate::sql::quote_ident(kind, s);
    if cols.len() == 1 {
        let vals: Vec<String> = tuples.iter().map(|t| lit(kind, &t[0])).collect();
        return format!("{} IN ({})", q(&cols[0]), vals.join(", "));
    }
    let names: Vec<String> = cols.iter().map(|c| q(c)).collect();
    let vals: Vec<String> = tuples.iter().map(|t| format!("({})", t.iter().map(|v| lit(kind, v)).collect::<Vec<_>>().join(", "))).collect();
    format!("({}) IN ({})", names.join(", "), vals.join(", "))
}

struct Walk<'a> {
    st: &'a AppState,
    id: String,
    pool: Pool,
    kind: DbKind,
    erd: Erd,
    mask: bool,
    tables: HashMap<Key, Table>,
    /// 처음 들어온 순서
    order: Vec<Key>,
    total: usize,
}

impl Walk<'_> {
    fn ensure_table(&mut self, key: &Key, reason: &str) -> AppResult<()> {
        if self.tables.contains_key(key) {
            return Ok(());
        }
        let et = self.erd.tables.iter().find(|t| t.schema == key.0 && t.name == key.1).ok_or_else(|| AppError::Msg(format!("Couldn't read table {}.{}", key.0, key.1)))?;
        // PK · FK 컬럼은 가리지 않는다
        let keys: HashSet<&str> = self
            .erd
            .relations
            .iter()
            .flat_map(|r| {
                let mine = |s: &str, t: &str| s == key.0 && t == key.1;
                let a = if mine(&r.from_schema, &r.from_table) { r.from_columns.iter().map(String::as_str).collect::<Vec<_>>() } else { vec![] };
                let b = if mine(&r.to_schema, &r.to_table) { r.to_columns.iter().map(String::as_str).collect::<Vec<_>>() } else { vec![] };
                a.into_iter().chain(b)
            })
            .chain(et.columns.iter().filter(|c| c.pk).map(|c| c.name.as_str()))
            .collect();
        let masks = if self.mask { et.columns.iter().enumerate().filter(|(_, c)| !keys.contains(c.name.as_str())).filter_map(|(i, c)| mask_for(&key.1, &c.name, &c.data_type).map(|m| (i, m))).collect() } else { vec![] };
        self.tables.insert(key.clone(), Table { cols: et.columns.clone(), masks, rows: BTreeMap::new(), reason: reason.into() });
        self.order.push(key.clone());
        Ok(())
    }

    /// cond 에 맞는 행을 읽어 새 것만 남긴다 (새로 들어온 행의 키)
    async fn fetch(&mut self, key: &Key, cond: &str, reason: &str) -> AppResult<Vec<String>> {
        self.ensure_table(key, reason)?;
        let left = MAX_ROWS.saturating_sub(self.total);
        let sql = select_sql(self.kind, key, &self.tables[key], cond, left + 1);
        self.st.check_read_only(&self.id, &sql)?;
        let r = self.pool.run_query(&sql, left + 1).await.map_err(|e| AppError::Msg(format!("{}.{}: {e}", key.0, key.1)))?;
        let kind = self.kind;
        let t = self.tables.get_mut(key).unwrap();
        let pk: Vec<String> = t.cols.iter().filter(|c| c.pk).map(|c| c.name.clone()).collect();
        let mut new = vec![];
        for cells in r.rows {
            let row = match kind {
                DbKind::Postgres => Row { pg_json: cells.first().and_then(|v| v.as_str()).map(String::from), cells: vec![] },
                _ => Row { pg_json: None, cells },
            };
            let id = if pk.is_empty() { format!("{:?}{:?}", row.pg_json, row.cells) } else { pk.iter().map(|c| cell(kind, t, &row, c).to_string()).collect::<Vec<_>>().join("|") };
            if !t.rows.contains_key(&id) {
                t.rows.insert(id.clone(), row);
                new.push(id);
            }
        }
        self.total += new.len();
        if self.total > MAX_ROWS {
            return Err(AppError::Msg(format!("More than {MAX_ROWS} rows in total — narrow the WHERE, or copy fewer levels of child rows")));
        }
        Ok(new)
    }

    /// cols 가 tuples 중 하나인 행 (여러 번 나눠 읽는다)
    async fn fetch_in(&mut self, key: &Key, cols: &[String], tuples: Vec<Vec<Value>>, reason: &str) -> AppResult<Vec<String>> {
        let mut new = vec![];
        for ch in tuples.chunks(KEYS_PER_QUERY) {
            let cond = in_cond(self.kind, cols, ch);
            new.extend(self.fetch(key, &cond, reason).await?);
        }
        Ok(new)
    }

    /// 새 행들의 cols 값 (NULL 이 낀 것 · 겹치는 것은 빼고)
    fn tuples(&self, key: &Key, ids: &[String], cols: &[String]) -> Vec<Vec<Value>> {
        let t = &self.tables[key];
        let mut seen = HashSet::new();
        ids.iter()
            .filter_map(|id| t.rows.get(id))
            .map(|r| cols.iter().map(|c| cell(self.kind, t, r, c)).collect::<Vec<_>>())
            .filter(|v| v.iter().all(|x| !x.is_null()))
            .filter(|v| seen.insert(format!("{v:?}")))
            .collect()
    }

    async fn run(&mut self, spec: &SubsetSpec) -> AppResult<()> {
        let root: Key = (spec.schema.clone(), spec.table.clone());
        let cond = if spec.filter.trim().is_empty() { "TRUE".to_string() } else { format!("({})", spec.filter.trim().trim_end_matches(';')) };
        let cond = if self.kind == DbKind::Mysql && cond == "TRUE" { "1 = 1".to_string() } else { cond };
        let first = self.fetch(&root, &cond, "start").await?;
        if first.is_empty() {
            return Err(AppError::Msg(format!("No rows in {}.{} match the condition", root.0, root.1)));
        }
        let mut queue: VecDeque<(Key, Vec<String>, u32)> = VecDeque::from([(root, first, spec.depth)]);
        while let Some((key, ids, depth)) = queue.pop_front() {
            let rels = self.erd.relations.clone();
            // 부모: 이 행이 가리키는 행 (끝까지)
            for r in rels.iter().filter(|r| r.from_schema == key.0 && r.from_table == key.1) {
                let parent: Key = (r.to_schema.clone(), r.to_table.clone());
                let tuples = self.tuples(&key, &ids, &r.from_columns);
                if tuples.is_empty() {
                    continue;
                }
                let new = self.fetch_in(&parent, &r.to_columns, tuples, &format!("parent of {}", key.1)).await?;
                if !new.is_empty() {
                    queue.push_back((parent, new, 0));
                }
            }
            // 자식: 이 행을 가리키는 행 (depth 단계까지)
            if depth > 0 {
                for r in rels.iter().filter(|r| r.to_schema == key.0 && r.to_table == key.1) {
                    let child: Key = (r.from_schema.clone(), r.from_table.clone());
                    let tuples = self.tuples(&key, &ids, &r.to_columns);
                    if tuples.is_empty() {
                        continue;
                    }
                    let new = self.fetch_in(&child, &r.from_columns, tuples, &format!("child of {}", key.1)).await?;
                    if !new.is_empty() {
                        queue.push_back((child, new, depth - 1));
                    }
                }
            }
        }
        Ok(())
    }

    /// 넣는 순서: 부모 먼저 (자기 자신을 가리키는 관계는 빼고. 순환이면 남은 것은 들어온 순서대로)
    fn insert_order(&self) -> Vec<Key> {
        let mut left: Vec<Key> = self.order.clone();
        let mut out = vec![];
        while !left.is_empty() {
            let ready = left.iter().position(|k| {
                !self.erd.relations.iter().any(|r| r.from_schema == k.0 && r.from_table == k.1 && !(r.to_schema == k.0 && r.to_table == k.1) && left.iter().any(|o| o.0 == r.to_schema && o.1 == r.to_table))
            });
            out.push(left.remove(ready.unwrap_or(0)));
        }
        out
    }

    fn plan(&self) -> SubsetPlan {
        let tables: Vec<SubsetTable> = self
            .insert_order()
            .into_iter()
            .map(|k| {
                let t = &self.tables[&k];
                SubsetTable { masked: t.masks.iter().map(|(i, m)| MaskedColumn { column: t.cols[*i].name.clone(), mask: *m }).collect(), rows: t.rows.len(), reason: t.reason.clone(), schema: k.0, table: k.1 }
            })
            .collect();
        SubsetPlan { total: tables.iter().map(|t| t.rows).sum(), tables }
    }
}

impl AppState {
    async fn subset_walk(&self, source: &str, spec: &SubsetSpec) -> AppResult<Walk<'_>> {
        let kind = self.kind_of(source)?;
        if !matches!(kind, DbKind::Postgres | DbKind::Mysql) {
            return Err(AppError::Msg("Copying related rows works for PostgreSQL and MySQL".into()));
        }
        // 관계는 시작 스키마 기준 (다른 스키마를 가리키면 그 테이블도 들어온다)
        let erd = self.erd(source.into(), vec![spec.schema.clone()]).await?;
        let pool = ensure_pool(self, source).await?;
        let mut w = Walk { st: self, id: source.to_string(), pool, kind, erd, mask: spec.mask, tables: HashMap::new(), order: vec![], total: 0 };
        w.run(spec).await?;
        Ok(w)
    }

    /// 무엇이 몇 행 복사될지 (읽기만 한다)
    pub async fn subset_preview(&self, source: String, spec: SubsetSpec) -> AppResult<SubsetPlan> {
        Ok(self.subset_walk(&source, &spec).await?.plan())
    }

    /// 모은 행을 target 에 한 트랜잭션으로 넣는다 (이미 있는 행은 건너뛴다). 넣으려고 보낸 행 수와 계획
    pub async fn subset_copy(&self, source: String, spec: SubsetSpec, target: String) -> AppResult<SubsetPlan> {
        let (sk, tk) = (self.kind_of(&source)?, self.kind_of(&target)?);
        if sk != tk {
            return Err(AppError::Msg("The target must be the same kind of database as the source".into()));
        }
        if crate::app::split_target(&source) == crate::app::split_target(&target) || source == target {
            return Err(AppError::Msg("Pick a different data source (or database) to copy into".into()));
        }
        let w = self.subset_walk(&source, &spec).await?;
        let tpool = ensure_pool(self, &target).await?;
        let mut stmts = vec![];
        for key in w.insert_order() {
            let t = &w.tables[&key];
            if t.rows.is_empty() {
                continue;
            }
            // MySQL: 대상 DB 의 같은 이름 테이블 (원본 DB 이름을 붙이면 원본에 들어간다)
            let rel = if tk == DbKind::Mysql { crate::sql::quote_ident(tk, &key.1) } else { qualified(tk, &key) };
            let tcols = crate::audit::table_columns(&tpool, &rel, tk).await?;
            if tcols.is_empty() {
                return Err(AppError::Msg(format!("{}.{} doesn't exist in the target — create it first (Compare Schemas makes the DDL)", key.0, key.1)));
            }
            let q = |s: &str| crate::sql::quote_ident(tk, s);
            let rows: Vec<&Row> = t.rows.values().collect();
            match tk {
                DbKind::Postgres => {
                    let names: Vec<String> = tcols.iter().filter(|c| !c.generated).map(|c| q(&c.name)).collect();
                    let list = names.join(", ");
                    for ch in rows.chunks(ROWS_PER_INSERT) {
                        let json = format!("[{}]", ch.iter().filter_map(|r| r.pg_json.as_deref()).collect::<Vec<_>>().join(","));
                        stmts.push(format!(
                            "INSERT INTO {rel} ({list}) OVERRIDING SYSTEM VALUE SELECT {list} FROM jsonb_populate_recordset(NULL::{rel}, {}) ON CONFLICT DO NOTHING",
                            crate::audit::pg_text(&json)
                        ));
                    }
                }
                _ => {
                    // 원본 · 대상 둘 다 있는 컬럼만 (계산 컬럼은 빼고)
                    let pick: Vec<(usize, &crate::audit::Col)> = tcols.iter().filter(|c| !c.generated).filter_map(|c| t.cols.iter().position(|s| s.name.eq_ignore_ascii_case(&c.name)).map(|i| (i, c))).collect();
                    let names: Vec<String> = pick.iter().map(|(_, c)| q(&c.name)).collect();
                    for ch in rows.chunks(ROWS_PER_INSERT) {
                        let vals: Vec<String> = ch
                            .iter()
                            .map(|r| {
                                let cells: Vec<String> = pick
                                    .iter()
                                    .map(|(i, c)| match r.cells[*i].as_str() {
                                        None => "NULL".into(),
                                        Some("") if c.binary && binary(&t.cols[*i].data_type) => "''".into(),
                                        Some(h) if c.binary && binary(&t.cols[*i].data_type) => format!("X'{h}'"),
                                        Some(_) => crate::transfer::sql_literal(tk, &r.cells[*i]),
                                    })
                                    .collect();
                                format!("({})", cells.join(", "))
                            })
                            .collect();
                        // 이미 있는 행만 건너뛴다 (INSERT IGNORE 는 FK 오류까지 삼킨다)
                        let first = names.first().cloned().unwrap_or_default();
                        stmts.push(format!("INSERT INTO {rel} ({}) VALUES\n{}\nON DUPLICATE KEY UPDATE {first} = {first}", names.join(", "), vals.join(",\n")));
                    }
                }
            }
        }
        let plan = w.plan();
        self.execute_batch(target, stmts, false).await?;
        Ok(plan)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_personal_columns() {
        assert_eq!(mask_for("orders", "email", "text"), Some(Mask::Email));
        assert_eq!(mask_for("users", "mobile_phone", "varchar(20)"), Some(Mask::Phone));
        assert_eq!(mask_for("users", "name", "text"), Some(Mask::Name));
        // 사람 테이블이 아닌 name 은 그대로
        assert_eq!(mask_for("products", "name", "text"), None);
        assert_eq!(mask_for("orders", "recipient_name", "text"), Some(Mask::Name));
        assert_eq!(mask_for("users", "password_hash", "character varying(100)"), Some(Mask::Secret));
        assert_eq!(mask_for("members", "jumin", "char(13)"), Some(Mask::Secret));
        assert_eq!(mask_for("users", "birth_date", "date"), Some(Mask::Birth));
        assert_eq!(mask_for("users", "created_at", "timestamp with time zone"), None);
        assert_eq!(mask_for("logs", "client_ip", "inet"), Some(Mask::Ip));
        assert_eq!(mask_for("logs", "description", "text"), None);
        assert_eq!(mask_for("hotels", "hotel_name", "text"), None);
        // 글이 아니면 (숫자 id 등) 그대로
        assert_eq!(mask_for("users", "phone_id", "integer"), None);
    }

    #[test]
    fn in_conditions() {
        let one = in_cond(DbKind::Postgres, &["id".into()], &[vec![Value::from(1)], vec![Value::from("a'b")]]);
        assert_eq!(one, "id IN (1, 'a''b')");
        let two = in_cond(DbKind::Mysql, &["a".into(), "Order".into()], &[vec![Value::from(1), Value::from(2)]]);
        assert_eq!(two, "(a, `Order`) IN ((1, 2))");
    }

    // ───────── 실제 서버 ─────────

    async fn two_dbs(kind: DbKind, env: &str) -> Option<AppState> {
        let e = std::env::var(env).ok()?;
        let p: Vec<&str> = e.split(':').collect();
        let (user, pass, db) = if kind == DbKind::Mysql { (p[2], Some(p[3].to_string()), p[4]) } else { (p[2], None, p[3]) };
        let st = crate::open_state(Some(std::env::temp_dir().join(format!("qurry-subset-{}", uuid::Uuid::new_v4())))).unwrap();
        let c = crate::store::ConnectionConfig {
            id: "s".into(), name: "source".into(), kind, host: p[0].into(), port: p[1].parse().unwrap(), user: user.into(), database: db.into(),
            color: None, ssh: None, password_saved: false, ssh_secret_saved: false, scope: Default::default(), read_only: false, options: Default::default(), ssl: None, redis: None,
        };
        st.save_connection(c, pass, None).await.unwrap();
        Some(st)
    }

    /// 고객 한 명 → 주문 (자식) → 주문 품목 (자식의 자식) · 상품 (품목의 부모) · 추천인 (자기 참조 부모). 개인정보는 가리고, FK 는 다 맞는다
    async fn copies_related_rows(kind: DbKind, env: &str, target_db: &str) {
        let Some(st) = two_dbs(kind, env).await else { return };
        let q = |id: &str, s: &str| st.run_query(id.into(), s.into(), None, None);
        let src_db = if kind == DbKind::Mysql { std::env::var(env).unwrap().split(':').nth(4).unwrap().to_string() } else { "public".into() };
        let src_db = src_db.as_str();
        if kind == DbKind::Postgres {
            let _ = q("s", &format!("drop database if exists {target_db}")).await;
            q("s", &format!("create database {target_db}")).await.unwrap();
        } else {
            q("s", &format!("drop database if exists {target_db}")).await.unwrap();
            q("s", &format!("create database {target_db}")).await.unwrap();
        }
        let tgt = crate::sql::target_id("s", Some(target_db));
        // (MySQL 은 컬럼 옆 REFERENCES 를 무시한다 → 테이블 단위 FOREIGN KEY)
        let ddl = [
            "drop table if exists qs_items, qs_orders, qs_products, qs_customers",
            "create table qs_customers (id int primary key, email varchar(60) not null unique, name varchar(40), phone varchar(20), referrer_id int, foreign key (referrer_id) references qs_customers(id))",
            "create table qs_products (id int primary key, name varchar(40) not null, price decimal(8,2))",
            "create table qs_orders (id int primary key, customer_id int not null, note text, foreign key (customer_id) references qs_customers(id))",
            "create table qs_items (order_id int not null, product_id int not null, qty int, primary key (order_id, product_id), foreign key (order_id) references qs_orders(id), foreign key (product_id) references qs_products(id))",
        ];
        for d in ddl {
            q("s", d).await.unwrap();
            q(&tgt, d).await.unwrap();
        }
        q("s", "insert into qs_customers values (1, 'ref@x.com', 'Ref Kim', '010-1111-2222', null), (2, 'kim@x.com', 'Minjun Kim', '010-3333-4444', 1), (3, 'other@x.com', 'Other', null, null)").await.unwrap();
        q("s", "insert into qs_products values (10, 'Pen', 1.50), (11, 'Book', 12.00), (12, 'Unused', 1)").await.unwrap();
        q("s", "insert into qs_orders values (100, 2, 'first'), (101, 2, 'second'), (102, 3, 'not this one')").await.unwrap();
        q("s", "insert into qs_items values (100, 10, 2), (100, 11, 1), (101, 10, 5), (102, 12, 1)").await.unwrap();
        let spec = SubsetSpec { schema: src_db.into(), table: "qs_customers".into(), filter: "id = 2".into(), depth: 2, mask: true };
        let plan = st.subset_preview("s".into(), spec.clone()).await.unwrap();
        let count = |t: &str| plan.tables.iter().find(|x| x.table == t).map(|x| x.rows).unwrap_or(0);
        assert_eq!((count("qs_customers"), count("qs_orders"), count("qs_items"), count("qs_products")), (2, 2, 3, 2), "{plan:?}");
        // 부모가 먼저
        let pos = |t: &str| plan.tables.iter().position(|x| x.table == t).unwrap();
        assert!(pos("qs_customers") < pos("qs_orders") && pos("qs_orders") < pos("qs_items") && pos("qs_products") < pos("qs_items"), "{plan:?}");
        let masked: Vec<&str> = plan.tables[pos("qs_customers")].masked.iter().map(|m| m.column.as_str()).collect();
        assert_eq!(masked, vec!["email", "name", "phone"]);
        st.subset_copy("s".into(), spec.clone(), tgt.clone()).await.unwrap();
        // 두 번 해도 (이미 있는 행은 건너뛴다)
        st.subset_copy("s".into(), spec, tgt.clone()).await.unwrap();
        let rows = |s: &str| q(&tgt, s);
        assert_eq!(rows("select count(*) from qs_items").await.unwrap().rows[0][0].to_string().trim_matches('"'), "3");
        let c = rows("select id, email, name, phone, referrer_id from qs_customers order by id").await.unwrap().rows;
        assert_eq!(c.len(), 2);
        let email = c[1][1].as_str().unwrap();
        assert!(email.starts_with("user_") && email.ends_with("@example.com") && email != "kim@x.com", "{email}");
        assert!(c[1][2].as_str().unwrap().starts_with("User "), "{c:?}");
        assert!(c[1][3].as_str().unwrap().starts_with("010-") && c[1][3].as_str().unwrap() != "010-3333-4444");
        assert_eq!(c[1][4].to_string().trim_matches('"'), "1");
        assert_eq!(rows("select note from qs_orders where id = 101").await.unwrap().rows[0][0], serde_json::json!("second"));
        assert_eq!(rows("select price from qs_products where id = 11").await.unwrap().rows[0][0].to_string().trim_matches('"'), "12.00");
        // 같은 곳으로는 안 된다
        assert!(st.subset_copy("s".into(), SubsetSpec { schema: src_db.into(), table: "qs_customers".into(), filter: String::new(), depth: 0, mask: true }, "s".into()).await.is_err());
        for d in ["drop table qs_items", "drop table qs_orders", "drop table qs_products", "drop table qs_customers"] {
            q("s", d).await.unwrap();
        }
        crate::app::drop_pool(&st, &tgt).await;
        q("s", &format!("drop database {target_db}")).await.unwrap();
    }

    #[tokio::test]
    #[ignore]
    async fn pg_copies_related_rows() {
        copies_related_rows(DbKind::Postgres, "QURRY_TEST_PG", "qurry_subset_tgt").await;
    }

    #[tokio::test]
    #[ignore]
    async fn mysql_copies_related_rows() {
        copies_related_rows(DbKind::Mysql, "QURRY_TEST_MY", "qurry_subset_tgt").await;
    }
}
