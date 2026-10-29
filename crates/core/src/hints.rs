//! 콘솔 자동완성의 값 추천: `where status = '` → 그 컬럼에 실제로 있는 값, `data->>'` → jsonb 안의 키
//! 테이블 앞쪽 일부 행만 (SAMPLE) 읽어 많이 나온 순으로. READ ONLY 트랜잭션 + 짧은 시간 제한, 결과는 잠깐 캐시
use crate::app::{AppState, ensure_pool};
use crate::db::{self, Pool};
use crate::error::AppResult;
use crate::sql::quote_ident;
use crate::store::DbKind;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

/// 값을 세는 행 수 (테이블 앞쪽)
const SAMPLE: usize = 5000;
/// jsonb 키를 세는 행 수
const KEY_SAMPLE: usize = 2000;
const MAX_HINTS: usize = 100;
/// 값 하나의 최대 글자 수 (긴 text · json 은 추천하지 않는다)
const MAX_LEN: usize = 200;
const CACHE_FOR: Duration = Duration::from_secs(120);

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HintReq {
    pub schema: String,
    pub table: String,
    pub column: String,
    /// json 안의 경로 (`data->'a'->>'b'` 면 ["a", "b"])
    #[serde(default)]
    pub path: Vec<String>,
    /// true = 그 경로의 json 객체 키, false = 값
    #[serde(default)]
    pub keys: bool,
    /// true = 그 경로의 json 에서 많이 나온 `키: 값` 을 @> 조건으로 (`[{"code": "x"}]` · `{"code": "x"}`, PostgreSQL)
    #[serde(default)]
    pub pairs: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Hint {
    pub value: String,
    pub count: i64,
}

type Cache = HashMap<String, (Instant, Vec<Hint>)>;
static CACHE: LazyLock<Mutex<Cache>> = LazyLock::new(Default::default);

/// PG 문자열 리터럴
fn lit(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

fn pg_path(path: &[String]) -> String {
    // text[] 리터럴: {"a","b"}
    let inner: Vec<String> = path.iter().map(|p| format!("\"{}\"", p.replace('\\', "\\\\").replace('"', "\\\""))).collect();
    lit(&format!("{{{}}}", inner.join(",")))
}

fn my_path(path: &[String]) -> String {
    let mut s = "$".to_string();
    for p in path {
        s.push_str(&format!(".\"{}\"", p.replace('\\', "\\\\").replace('"', "\\\"")));
    }
    format!("'{}'", s.replace('\\', "\\\\").replace('\'', "''"))
}

/// 값 추천 SQL (첫 컬럼 = 값, 둘째 = 개수)
pub fn values_sql(kind: DbKind, r: &HintReq) -> String {
    let t = format!("{}.{}", quote_ident(kind, &r.schema), quote_ident(kind, &r.table));
    let c = quote_ident(kind, &r.column);
    match kind {
        DbKind::Mysql => {
            let e = if r.path.is_empty() { format!("CAST({c} AS CHAR)") } else { format!("CAST(JSON_UNQUOTE(JSON_EXTRACT({c}, {})) AS CHAR)", my_path(&r.path)) };
            format!(
                "SELECT /*+ MAX_EXECUTION_TIME(3000) */ v, COUNT(*) AS n FROM (SELECT {e} AS v FROM {t} LIMIT {SAMPLE}) s \
                 WHERE v IS NOT NULL AND CHAR_LENGTH(v) <= {MAX_LEN} GROUP BY v ORDER BY n DESC, v LIMIT {MAX_HINTS}"
            )
        }
        _ => {
            let e = if r.path.is_empty() { "c::text".to_string() } else { format!("c::jsonb #>> {}", pg_path(&r.path)) };
            format!(
                "SELECT v, count(*) AS n FROM (SELECT {e} AS v FROM (SELECT {c} AS c FROM {t} LIMIT {SAMPLE}) s0) s \
                 WHERE v IS NOT NULL AND length(v) <= {MAX_LEN} GROUP BY v ORDER BY n DESC, v LIMIT {MAX_HINTS}"
            )
        }
    }
}

/// json 키 추천 SQL. PG 는 키 · 개수, MySQL 은 행마다 JSON_KEYS 배열 (세기는 여기서)
pub fn keys_sql(kind: DbKind, r: &HintReq) -> String {
    let t = format!("{}.{}", quote_ident(kind, &r.schema), quote_ident(kind, &r.table));
    let c = quote_ident(kind, &r.column);
    match kind {
        DbKind::Mysql => format!("SELECT /*+ MAX_EXECUTION_TIME(3000) */ CAST(JSON_KEYS({c}, {}) AS CHAR) FROM {t} WHERE {c} IS NOT NULL LIMIT {KEY_SAMPLE}", my_path(&r.path)),
        _ => {
            let e = if r.path.is_empty() { "c::jsonb".to_string() } else { format!("c::jsonb #> {}", pg_path(&r.path)) };
            format!(
                "SELECT k, count(*) AS n FROM (SELECT {e} AS j FROM (SELECT {c} AS c FROM {t} WHERE {c} IS NOT NULL LIMIT {KEY_SAMPLE}) s0) s \
                 CROSS JOIN LATERAL jsonb_object_keys(CASE WHEN jsonb_typeof(j) = 'object' THEN j END) k GROUP BY k ORDER BY n DESC, k LIMIT {MAX_HINTS}"
            )
        }
    }
}

/// @> 조건 추천 SQL (PostgreSQL): 배열이면 원소 객체의, 객체면 그 객체의 `키: 값` (값이 글 · 수 · 참거짓) 을 많이 나온 순으로.
/// 개수 = 그 조건에 맞는 행 수
pub fn pairs_sql(r: &HintReq) -> String {
    let kind = DbKind::Postgres;
    let t = format!("{}.{}", quote_ident(kind, &r.schema), quote_ident(kind, &r.table));
    let c = quote_ident(kind, &r.column);
    let e = if r.path.is_empty() { "c::jsonb".to_string() } else { format!("c::jsonb #> {}", pg_path(&r.path)) };
    format!(
        "SELECT (CASE WHEN x.arr THEN jsonb_build_array(jsonb_build_object(p.k, p.v)) ELSE jsonb_build_object(p.k, p.v) END)::text AS v, count(DISTINCT s.rn) AS n \
         FROM (SELECT row_number() OVER () AS rn, {e} AS j FROM (SELECT {c} AS c FROM {t} WHERE {c} IS NOT NULL LIMIT {KEY_SAMPLE}) s0) s \
         CROSS JOIN LATERAL (SELECT true, el FROM jsonb_array_elements(CASE WHEN jsonb_typeof(s.j) = 'array' THEN s.j END) el \
                             UNION ALL SELECT false, s.j WHERE jsonb_typeof(s.j) = 'object') x(arr, o) \
         CROSS JOIN LATERAL jsonb_each(CASE WHEN jsonb_typeof(x.o) = 'object' THEN x.o END) p(k, v) \
         WHERE jsonb_typeof(p.v) IN ('string', 'number', 'boolean') AND length(p.v::text) <= {MAX_LEN} \
         GROUP BY 1 ORDER BY 2 DESC, 1 LIMIT {MAX_HINTS}"
    )
}

fn pairs(rows: &[Vec<Value>]) -> Vec<Hint> {
    rows.iter()
        .filter_map(|r| {
            let value = match r.first()? {
                Value::String(s) => s.clone(),
                Value::Null => return None,
                v => v.to_string(),
            };
            let count = r.get(1).and_then(|n| n.as_i64().or_else(|| n.as_str().and_then(|s| s.parse().ok()))).unwrap_or(0);
            Some(Hint { value, count })
        })
        .collect()
}

/// MySQL JSON_KEYS 결과 (행마다 `["a", "b"]`) 를 센다
fn count_keys(rows: &[Vec<Value>]) -> Vec<Hint> {
    let mut n: HashMap<String, i64> = HashMap::new();
    for r in rows {
        let Some(Value::String(s)) = r.first() else { continue };
        if let Ok(Value::Array(keys)) = serde_json::from_str::<Value>(s) {
            for k in keys.iter().filter_map(|k| k.as_str()) {
                *n.entry(k.to_string()).or_default() += 1;
            }
        }
    }
    let mut v: Vec<Hint> = n.into_iter().map(|(value, count)| Hint { value, count }).collect();
    v.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.value.cmp(&b.value)));
    v.truncate(MAX_HINTS);
    v
}

impl AppState {
    /// 컬럼 값 · json 키 추천 (PostgreSQL · MySQL. 그 밖은 빈 목록)
    pub async fn column_hints(&self, id: String, req: HintReq) -> AppResult<Vec<Hint>> {
        use sqlx::Executor;
        let key = format!("{id}\u{1}{}\u{1}{}\u{1}{}\u{1}{}\u{1}{}{}", req.schema, req.table, req.column, req.path.join("\u{2}"), req.keys, req.pairs);
        if let Some((at, v)) = CACHE.lock().unwrap().get(&key)
            && at.elapsed() < CACHE_FOR
        {
            return Ok(v.clone());
        }
        let hints = match ensure_pool(self, &id).await? {
            Pool::Pg(p) => {
                let sql = if req.pairs {
                    pairs_sql(&req)
                } else if req.keys {
                    keys_sql(DbKind::Postgres, &req)
                } else {
                    values_sql(DbKind::Postgres, &req)
                };
                let mut tx = p.begin().await?;
                tx.execute("SET TRANSACTION READ ONLY").await?;
                tx.execute("SET LOCAL statement_timeout = '3s'").await?;
                let r = db::run_on(&mut tx, &sql, MAX_HINTS).await;
                let _ = tx.rollback().await;
                pairs(&r?.rows)
            }
            // MySQL 은 JSON_CONTAINS 이 따로라 @> 조건은 없다
            Pool::My(_) if req.pairs => vec![],
            Pool::My(p) => {
                let mut conn = p.acquire().await?;
                conn.execute("START TRANSACTION READ ONLY").await?;
                let r = if req.keys {
                    crate::mysql::run_on(&mut conn, &keys_sql(DbKind::Mysql, &req), KEY_SAMPLE).await.map(|r| count_keys(&r.rows))
                } else {
                    crate::mysql::run_on(&mut conn, &values_sql(DbKind::Mysql, &req), MAX_HINTS).await.map(|r| pairs(&r.rows))
                };
                let _ = conn.execute("ROLLBACK").await;
                r?
            }
            Pool::Redis(..) | Pool::Plugin(_) => vec![],
        };
        let mut c = CACHE.lock().unwrap();
        c.retain(|_, (at, _)| at.elapsed() < CACHE_FOR);
        c.insert(key, (Instant::now(), hints.clone()));
        Ok(hints)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(path: &[&str], keys: bool) -> HintReq {
        HintReq { schema: "public".into(), table: "Orders".into(), column: "data".into(), path: path.iter().map(|s| s.to_string()).collect(), keys, pairs: false }
    }

    #[test]
    fn sql_shapes() {
        let v = values_sql(DbKind::Postgres, &req(&[], false));
        assert!(v.contains("FROM public.\"Orders\" LIMIT 5000") && v.contains("c::text"), "{v}");
        let p = values_sql(DbKind::Postgres, &req(&["a", "it's"], false));
        assert!(p.contains(r#"c::jsonb #>> '{"a","it''s"}'"#), "{p}");
        let k = keys_sql(DbKind::Postgres, &req(&["a"], true));
        assert!(k.contains("jsonb_object_keys") && k.contains(r#"#> '{"a"}'"#), "{k}");
        let m = values_sql(DbKind::Mysql, &req(&["a"], false));
        assert!(m.contains(r#"JSON_EXTRACT(data, '$."a"')"#) && !m.contains('`') && m.contains("public.Orders"), "{m}");
        assert!(keys_sql(DbKind::Mysql, &req(&[], true)).contains("JSON_KEYS(data, '$')"));
    }

    #[test]
    fn counts_mysql_keys() {
        let rows = vec![vec![Value::from(r#"["a", "b"]"#)], vec![Value::from(r#"["b"]"#)], vec![Value::Null]];
        assert_eq!(count_keys(&rows), vec![Hint { value: "b".into(), count: 2 }, Hint { value: "a".into(), count: 1 }]);
    }

    /// 실제 DB: QURRY_TEST_PG=host:port:user:db cargo test -p qurry-core hints -- --ignored
    #[tokio::test]
    #[ignore]
    async fn pg_values_and_keys() {
        use sqlx::Executor;
        let env = std::env::var("QURRY_TEST_PG").unwrap();
        let s: Vec<&str> = env.split(':').collect();
        let p = sqlx::PgPool::connect(&format!("postgres://{}@{}:{}/{}", s[2], s[0], s[1], s[3])).await.unwrap();
        p.execute("drop table if exists public.hint_t; create table public.hint_t (id int, status text, data jsonb); \
                   insert into public.hint_t select g, case when g % 3 = 0 then 'paid' else 'ready' end, jsonb_build_object('kind', 'k' || (g % 2), 'meta', jsonb_build_object('src', 'web')) from generate_series(1, 30) g")
            .await
            .unwrap();
        let rq = |column: &str, path: &[&str], keys: bool| HintReq { schema: "public".into(), table: "hint_t".into(), column: column.into(), path: path.iter().map(|s| s.to_string()).collect(), keys, pairs: false };
        let run = |sql: String| {
            let p = p.clone();
            async move { pairs(&db::run_on(&mut p.acquire().await.unwrap(), &sql, 100).await.unwrap().rows) }
        };
        assert_eq!(run(values_sql(DbKind::Postgres, &rq("status", &[], false))).await, vec![Hint { value: "ready".into(), count: 20 }, Hint { value: "paid".into(), count: 10 }]);
        assert_eq!(run(keys_sql(DbKind::Postgres, &rq("data", &[], true))).await.iter().map(|h| h.value.as_str()).collect::<Vec<_>>(), ["kind", "meta"]);
        assert_eq!(run(keys_sql(DbKind::Postgres, &rq("data", &["meta"], true))).await, vec![Hint { value: "src".into(), count: 30 }]);
        assert_eq!(run(values_sql(DbKind::Postgres, &rq("data", &["kind"], false))).await.len(), 2);
        assert_eq!(run(values_sql(DbKind::Postgres, &rq("id", &[], false))).await.len(), 30);
        // 객체의 배열: 키는 없고, @> 조건은 원소의 키: 값
        p.execute("alter table public.hint_t add column reasons jsonb; \
                   update public.hint_t set reasons = case when id % 2 = 0 then '[{\"code\": \"self_transfer\", \"n\": 1}, {\"code\": \"overpay\"}]'::jsonb else '[{\"code\": \"overpay\", \"detail\": [1]}]'::jsonb end")
            .await
            .unwrap();
        assert!(run(keys_sql(DbKind::Postgres, &rq("reasons", &[], true))).await.is_empty());
        let mut pr = rq("reasons", &[], false);
        pr.pairs = true;
        assert_eq!(
            run(pairs_sql(&pr)).await,
            vec![
                Hint { value: r#"[{"code": "overpay"}]"#.into(), count: 30 },
                Hint { value: r#"[{"code": "self_transfer"}]"#.into(), count: 15 },
                Hint { value: r#"[{"n": 1}]"#.into(), count: 15 },
            ]
        );
        // 객체면 {키: 값}
        let mut po = rq("data", &[], false);
        po.pairs = true;
        assert_eq!(run(pairs_sql(&po)).await.iter().map(|h| h.value.as_str()).collect::<Vec<_>>(), [r#"{"kind": "k0"}"#, r#"{"kind": "k1"}"#]);
        // 고른 조건이 실제로 맞는 행 수
        let n: i64 = sqlx::query_scalar(r#"select count(*) from public.hint_t where reasons @> '[{"code": "self_transfer"}]'"#).fetch_one(&p).await.unwrap();
        assert_eq!(n, 15);
        p.execute("drop table public.hint_t").await.unwrap();
    }

    /// 실제 DB: QURRY_TEST_MY=host:port:user:password:db cargo test -p qurry-core hints -- --ignored
    #[tokio::test]
    #[ignore]
    async fn mysql_values_and_keys() {
        use sqlx::Executor;
        let env = std::env::var("QURRY_TEST_MY").unwrap();
        let s: Vec<&str> = env.split(':').collect();
        let db = s[4].to_string();
        let p = sqlx::MySqlPool::connect(&format!("mysql://{}:{}@{}:{}/{db}?ssl-mode=disabled", s[2], s[3], s[0], s[1])).await.unwrap();
        p.execute("drop table if exists hint_t").await.unwrap();
        p.execute("create table hint_t (id int, status varchar(20), data json)").await.unwrap();
        for g in 1..=30 {
            p.execute(format!("insert into hint_t values ({g}, '{}', json_object('kind', 'k{}', 'meta', json_object('src', 'web')))", if g % 3 == 0 { "paid" } else { "ready" }, g % 2).as_str()).await.unwrap();
        }
        let rq = |column: &str, path: &[&str], keys: bool| HintReq { schema: db.clone(), table: "hint_t".into(), column: column.into(), path: path.iter().map(|s| s.to_string()).collect(), keys, pairs: false };
        let rows = |sql: String| {
            let p = p.clone();
            async move { crate::mysql::run_on(&mut p.acquire().await.unwrap(), &sql, 5000).await.unwrap().rows }
        };
        assert_eq!(pairs(&rows(values_sql(DbKind::Mysql, &rq("status", &[], false))).await), vec![Hint { value: "ready".into(), count: 20 }, Hint { value: "paid".into(), count: 10 }]);
        assert_eq!(count_keys(&rows(keys_sql(DbKind::Mysql, &rq("data", &[], true))).await).iter().map(|h| h.value.as_str()).collect::<Vec<_>>(), ["kind", "meta"]);
        assert_eq!(count_keys(&rows(keys_sql(DbKind::Mysql, &rq("data", &["meta"], true))).await), vec![Hint { value: "src".into(), count: 30 }]);
        assert_eq!(pairs(&rows(values_sql(DbKind::Mysql, &rq("data", &["kind"], false))).await).iter().map(|h| h.value.as_str()).collect::<Vec<_>>(), ["k0", "k1"]);
        assert_eq!(pairs(&rows(values_sql(DbKind::Mysql, &rq("id", &[], false))).await).len(), 30);
        p.execute("drop table hint_t").await.unwrap();
    }
}
