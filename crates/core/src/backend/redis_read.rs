//! What MCP reads from Redis (#224): one byte budget per read, measured on the server before it runs (STRLEN, HSTRLEN, MEMORY
//! USAGE), and each value in the answer cut to VALUE_MAX, with values that aren't UTF-8 as base64. The app's own reads
//! (console, value viewer) are redisdb's and aren't limited here
use crate::db::QueryResult;
use crate::error::{AppError, AppResult};
use crate::redisdb::{element_count, memory_usage, per_key, value_size, RedisConn};
use base64::Engine;
use redis::Value as RV;
use serde_json::{json, Value};

/// What one MCP read may take from the server, summed over every key and field it reads
pub const READ_MAX_BYTES: i64 = 4 << 20;
/// Longest value (string, element, field, member) an MCP answer carries; longer ones are cut and marked
pub const VALUE_MAX: usize = 64 << 10;
/// Collections up to this many elements are measured exactly (MEMORY USAGE … SAMPLES 0 walks each element); bigger ones from
/// MEMORY_SAMPLES elements, so the server isn't walked whole for a read of a few
const EXACT_ELEMENTS: i64 = 10_000;
const MEMORY_SAMPLES: u32 = 1000;

fn mb(n: i64) -> String {
    format!("{:.1} MB", n as f64 / (1 << 20) as f64)
}

/// Bytes `elements` of a collection key take: its memory (exact or sampled), shared out over its elements
async fn part_bytes(m: &RedisConn, key: &[u8], count: &str, elements: i64) -> AppResult<i64> {
    let n = element_count(m, count, key).await?;
    if n <= 0 || elements <= 0 {
        return Ok(0);
    }
    let usage = memory_usage(m, key, if n <= EXACT_ELEMENTS { 0 } else { MEMORY_SAMPLES }).await?;
    Ok(share(usage, elements, n))
}

fn share(usage: i64, elements: i64, n: i64) -> i64 {
    if elements >= n { usage } else { (usage as i128 * elements as i128 / n as i128) as i64 }
}

/// Bytes of the strings and hash fields the command reads (GET, GETRANGE, MGET, JSON.GET, JSON.MGET, HGET, HMGET)
async fn value_bytes(m: &RedisConn, cmd: &str, raw: &[Vec<u8>], rest: &[String]) -> AppResult<i64> {
    let num = |i: usize| rest.get(i).and_then(|a| a.parse::<i64>().ok());
    let one = || raw.get(..1).unwrap_or(&[]);
    Ok(match cmd {
        // A bounded range reads at most that much, whatever the string's length
        "GETRANGE" => match (num(1), num(2)) {
            (Some(a), Some(b)) if a >= 0 && b >= a => b - a + 1,
            _ => sum_strlen(m, one()).await?,
        },
        "GET" => sum_strlen(m, one()).await?,
        "MGET" => sum_strlen(m, raw).await?,
        "JSON.GET" | "JSON.MGET" => {
            let keys = if cmd == "JSON.GET" { one() } else { raw.split_last().map(|(_, k)| k).unwrap_or(&[]) };
            let mut n = 0i64;
            for k in keys {
                n = n.saturating_add(value_size(m, k, true).await?);
            }
            n
        }
        "HGET" | "HMGET" if raw.len() > 1 => {
            let mut pipe = redis::pipe();
            for f in &raw[1..] {
                pipe.cmd("HSTRLEN").arg(raw[0].as_slice()).arg(f.as_slice());
            }
            let lens: Vec<i64> = pipe.query_async(&mut m.clone()).await.map_err(crate::redisdb::err)?;
            lens.iter().fold(0i64, |a, b| a.saturating_add(*b))
        }
        _ => 0,
    })
}

/// STRLEN of the keys that are strings (MGET returns nil for the others): TYPE, then STRLEN, each in one round trip
async fn sum_strlen(m: &RedisConn, keys: &[Vec<u8>]) -> AppResult<i64> {
    let types = per_key(m, keys, &["TYPE"]).await?;
    let strings: Vec<Vec<u8>> = keys.iter().zip(&types).filter(|(_, t)| matches!(t, RV::SimpleString(s) if s == "string")).map(|(k, _)| k.clone()).collect();
    Ok(per_key(m, &strings, &["STRLEN"]).await?.iter().map(|v| if let RV::Int(n) = v { *n } else { 0 }).fold(0i64, i64::saturating_add))
}

/// What to do instead of a read over the budget
fn instead(cmd: &str) -> &'static str {
    match cmd {
        "GET" | "GETRANGE" | "MGET" | "JSON.GET" | "JSON.MGET" => "read a part with GETRANGE key start end, or a preview with redis_get",
        c if c.starts_with('H') => "ask for fewer fields (HSTRLEN key field gives a value's size), or HSCAN with a small COUNT",
        _ => "ask for fewer elements (a smaller range, COUNT or LIMIT), or a preview with redis_get",
    }
}

/// MCP reads (query, and reads sent to execute), after the guard's limits: whole keys (HGETALL, LRANGE 0 -1 …) up to
/// REDIS_STRICT_MAX elements, then the bytes of everything the command reads — whole keys, parts of collections (LRANGE 0 7,
/// HSCAN … COUNT 1000, HRANDFIELD …), strings and hash fields — summed and held to READ_MAX_BYTES. raw = the arguments as sent,
/// rest = the same as text
pub(crate) async fn check_budget(m: &RedisConn, cmd: &str, raw: &[Vec<u8>], rest: &[String]) -> AppResult<()> {
    use crate::guard::REDIS_STRICT_MAX;
    let mut bytes = 0i64;
    if let Some(c) = crate::guard::redis_size_check(cmd, rest) {
        let keys: Vec<&[u8]> = c.keys.iter().map(|&i| raw[i].as_slice()).collect();
        let mut n = 0i64;
        for k in &keys {
            n = n.saturating_add(element_count(m, c.count, k).await?);
        }
        if n > REDIS_STRICT_MAX {
            return Err(AppError::Msg(format!("{cmd} would read {n} elements — at most {REDIS_STRICT_MAX} in read-only mode; use {} (or redis_get for a preview)", c.instead)));
        }
        for k in &keys {
            bytes = bytes.saturating_add(memory_usage(m, k, 0).await?);
        }
    } else if let Some(p) = crate::guard::redis_part_read(cmd, rest) {
        bytes = part_bytes(m, &raw[p.key], p.count, p.elements).await?;
    }
    bytes = bytes.saturating_add(value_bytes(m, cmd, raw, rest).await?);
    if bytes > READ_MAX_BYTES {
        return Err(AppError::Msg(format!("{cmd} would read about {} — at most {} per MCP read; {}", mb(bytes), mb(READ_MAX_BYTES), instead(cmd))));
    }
    Ok(())
}

/// The elements of a collection key redis_get may read within READ_MAX_BYTES: `limit`, or fewer when they are big. None of them
/// fits: refused, with the key's size
pub(crate) async fn elements_within_budget(m: &RedisConn, key: &[u8], ty: &str, length: i64, limit: usize) -> AppResult<usize> {
    if length <= 0 {
        return Ok(limit);
    }
    let usage = memory_usage(m, key, if length <= EXACT_ELEMENTS { 0 } else { MEMORY_SAMPLES }).await?;
    let fit = (READ_MAX_BYTES as i128 * length as i128 / usage.max(1) as i128).min(limit as i128) as usize;
    if fit == 0 {
        return Err(AppError::Msg(format!(
            "{} is a {ty} of {length} elements using {}, about {} each — over MCP's {} read budget, so none is read; ask the user to open it in Qurry",
            String::from_utf8_lossy(key), mb(usage), mb(usage / length), mb(READ_MAX_BYTES)
        )));
    }
    Ok(fit)
}

/// get_ddl of a key reads it whole (redisdb::key_ddl): the same checks as the whole read of its type through query
pub(crate) async fn check_whole_key(m: &RedisConn, key: &[u8]) -> AppResult<()> {
    let ty = crate::redisdb::key_type(&mut m.clone(), key).await?;
    let read: &[&str] = match ty.as_str() {
        "string" => &["GET"],
        "hash" => &["HGETALL"],
        "list" => &["LRANGE", "0", "-1"],
        "set" => &["SMEMBERS"],
        "zset" => &["ZRANGE", "0", "-1"],
        // Other types get a comment line, no value
        _ => return Ok(()),
    };
    let mut args = vec![read[0].as_bytes().to_vec(), key.to_vec()];
    args.extend(read[1..].iter().map(|a| a.as_bytes().to_vec()));
    crate::redisdb::check_read_args(m, &args).await
}

/// A value in an MCP answer: text up to VALUE_MAX as is; longer text as {"text": its first VALUE_MAX bytes, "size": its bytes};
/// bytes that aren't UTF-8 as {"base64": …}, with "size" when cut too. `size` = the value's whole length when `b` is only its
/// beginning (a string preview), else b.len()
pub fn value_json(b: &[u8], size: usize) -> Value {
    let part = &b[..b.len().min(VALUE_MAX)];
    let cut = size > part.len();
    let text = match std::str::from_utf8(part) {
        Ok(s) => Some(s),
        // Cut inside a character: the text up to it
        Err(e) if cut && e.error_len().is_none() => std::str::from_utf8(&part[..e.valid_up_to()]).ok(),
        Err(_) => None,
    };
    match (text, cut) {
        (Some(s), false) => Value::from(s),
        (Some(s), true) => json!({ "text": s, "size": size }),
        (None, false) => json!({ "base64": base64::engine::general_purpose::STANDARD.encode(part) }),
        (None, true) => json!({ "base64": base64::engine::general_purpose::STANDARD.encode(part), "size": size }),
    }
}

/// A Redis reply in an MCP answer: bulk strings by value_json, the rest as the app shows them
pub(crate) fn reply_json(v: &RV) -> Value {
    match v {
        RV::BulkString(b) => value_json(b, b.len()),
        other => crate::redisdb::cell(other),
    }
}

/// An MCP query's answer: text cells over VALUE_MAX cut there (at a character boundary), their [row, column] in `cut`
pub(crate) fn cut_cells(r: &mut QueryResult) {
    for (i, row) in r.rows.iter_mut().enumerate() {
        for (j, v) in row.iter_mut().enumerate() {
            if let Value::String(s) = v
                && s.len() > VALUE_MAX
            {
                let mut at = VALUE_MAX;
                while !s.is_char_boundary(at) {
                    at -= 1;
                }
                s.truncate(at);
                r.cut.push([i, j]);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_are_text_cut_text_or_base64() {
        assert_eq!(value_json(b"hi", 2), json!("hi"));
        let big = "é".repeat(VALUE_MAX);
        let v = value_json(big.as_bytes(), big.len());
        let text = v["text"].as_str().unwrap();
        assert!(text.len() <= VALUE_MAX && text.len() > VALUE_MAX - 2 && v["size"] == json!(big.len()), "cut at a character");
        assert_eq!(value_json(b"\x89PNG\xff\xfe", 6), json!({ "base64": "iVBOR//+" }));
        let bin = vec![0xffu8; VALUE_MAX + 5];
        let v = value_json(&bin, bin.len());
        assert_eq!((v["size"].clone(), v["base64"].as_str().unwrap().len()), (json!(VALUE_MAX + 5), VALUE_MAX.div_ceil(3) * 4));
        // A string preview: the beginning of a longer value
        assert_eq!(value_json(b"abc", 10), json!({ "text": "abc", "size": 10 }));
        // Ends inside a character and isn't cut: not UTF-8
        assert!(value_json(&[b'a', 0xc3], 2).get("base64").is_some());
    }

    #[test]
    fn long_cells_are_cut_and_listed() {
        let mut r = crate::db::query_result(vec![], vec![vec![json!(1), json!("x".repeat(VALUE_MAX + 1))], vec![json!("ok"), json!("€".repeat(VALUE_MAX))]], 0, false, std::time::Instant::now(), true);
        cut_cells(&mut r);
        assert_eq!(r.cut, [[0, 1], [1, 1]]);
        assert_eq!(r.rows[0][1].as_str().unwrap().len(), VALUE_MAX);
        assert!(r.rows[1][1].as_str().unwrap().len() <= VALUE_MAX);
    }

    /// A source on QURRY_TEST_REDIS (host:port), saved in a new config folder
    async fn state(db: &str) -> Option<(std::sync::Arc<crate::app::AppState>, RedisConn)> {
        let addr = crate::testenv::var("QURRY_TEST_REDIS")?;
        let (host, port) = addr.rsplit_once(':').expect("QURRY_TEST_REDIS=host:port");
        let cfg: crate::store::ConnectionConfig = serde_json::from_value(json!({ "id": "rd", "name": "rd", "kind": "redis", "host": host, "port": port.parse::<u16>().unwrap(), "user": "", "database": db })).unwrap();
        let st = crate::open_state(Some(std::env::temp_dir().join(format!("qurry-rbudget-{}", uuid::Uuid::new_v4())))).unwrap();
        st.save_connection(cfg.clone(), None, None).await.unwrap();
        let crate::db::Pool::Redis(m, _) = crate::db::connect(&cfg, None).await.expect("connect") else { panic!() };
        Some((std::sync::Arc::new(st), m))
    }

    /// #224: every MCP read stays within READ_MAX_BYTES, measured before it runs, and its values come back cut at VALUE_MAX.
    /// Own qurry:budget:* keys in db9
    #[tokio::test]
    #[ignore]
    async fn mcp_reads_stay_within_the_byte_budget() {
        let Some((st, m)) = state("db9").await else { return };
        let mut c = m.clone();
        let two = "x".repeat(2 << 20);
        let keys = ["qurry:budget:l", "qurry:budget:h", "qurry:budget:big", "qurry:budget:s1", "qurry:budget:s2", "qurry:budget:s3", "qurry:budget:small"];
        let _: () = redis::cmd("DEL").arg(&keys[..]).query_async(&mut c).await.unwrap();
        let _: () = redis::cmd("RPUSH").arg(keys[0]).arg(&two).arg(&two).arg(&two).query_async(&mut c).await.unwrap();
        let _: () = redis::cmd("HSET").arg(keys[1]).arg("f1").arg(&two).arg("f2").arg(&two).arg("f3").arg(&two).query_async(&mut c).await.unwrap();
        let _: () = redis::cmd("SET").arg(keys[2]).arg("y".repeat(5 << 20)).query_async(&mut c).await.unwrap();
        for k in &keys[3..6] {
            let _: () = redis::cmd("SET").arg(*k).arg(&two).query_async(&mut c).await.unwrap();
        }
        let _: () = redis::cmd("HSET").arg(keys[6]).arg("a").arg("1").arg("b").arg("2").query_async(&mut c).await.unwrap();
        let read = |s: &str| st.run_read_only("rd::db9".into(), s.to_string(), 100);
        for over in ["LRANGE qurry:budget:l 0 2", "LRANGE qurry:budget:l 0 -1", "HMGET qurry:budget:h f1 f2 f3", "HRANDFIELD qurry:budget:h 3 WITHVALUES",
            "HSCAN qurry:budget:h 0 COUNT 1000", "HGETALL qurry:budget:h", "MGET qurry:budget:s1 qurry:budget:s2 qurry:budget:s3", "GET qurry:budget:big"] {
            let e = read(over).await.unwrap_err().to_string();
            assert!(e.contains("per MCP read"), "{over}: {e}");
        }
        for (one, cell) in [("LRANGE qurry:budget:l 0 0", [0, 1]), ("HMGET qurry:budget:h f1", [0, 1]), ("MGET qurry:budget:s1", [0, 1]), ("GET qurry:budget:s2", [0, 0])] {
            let r = read(one).await.unwrap_or_else(|e| panic!("{one}: {e}"));
            assert_eq!(r.cut, [cell], "{one}");
            assert_eq!(r.rows[0][cell[1]].as_str().unwrap().len(), VALUE_MAX, "{one}");
        }
        assert!(read("GETRANGE qurry:budget:big 0 99").await.is_ok());
        let e = read(&format!("MGET {}", (0..1001).map(|i| format!("k{i}")).collect::<Vec<_>>().join(" "))).await.unwrap_err().to_string();
        assert!(e.contains("at most 1000 arguments"), "{e}");
        for e in [read("DBTOOL.VIEW qurry:budget:big").await.unwrap_err(), st.execute_batch_prepared("rd::db9".into(), vec!["DBTOOL.VIEW qurry:budget:big".into()]).await.unwrap_err()] {
            assert!(e.to_string().contains("use redis_get"), "{e}");
        }
        // redis_get: as many elements as fit, each cut; a string's first VALUE_MAX bytes
        let d = st.redis_key_read_only("rd::db9".into(), keys[0].as_bytes().to_vec(), 200).await.unwrap();
        let v = d.value.as_array().unwrap();
        assert!(d.truncated && !v.is_empty() && v.len() < 3, "{} of 3", v.len());
        assert_eq!((v[0]["text"].as_str().unwrap().len(), v[0]["size"].clone()), (VALUE_MAX, json!(2 << 20)));
        let d = st.redis_key_read_only("rd::db9".into(), keys[2].as_bytes().to_vec(), 200).await.unwrap();
        assert!(d.truncated && d.value["text"].as_str().unwrap().len() == VALUE_MAX && d.value["size"] == json!(5 << 20));
        // get_ddl reads the key whole
        assert_eq!(st.redis_key_ddl_read_only("rd::db9".into(), keys[6].into()).await.unwrap(), "HSET qurry:budget:small a 1 b 2");
        let e = st.redis_key_ddl_read_only("rd::db9".into(), keys[1].into()).await.unwrap_err().to_string();
        assert!(e.contains("reads the whole key") && e.contains("per MCP read"), "{e}");
        let _: () = redis::cmd("DEL").arg(&keys[..]).query_async(&mut c).await.unwrap();
    }

    #[test]
    fn a_part_shares_the_key_memory() {
        assert_eq!(share(1000, 10, 100), 100);
        assert_eq!(share(1000, 500, 100), 1000);
        assert_eq!(share(i64::MAX, i64::MAX - 1, i64::MAX), i64::MAX - 1);
    }
}
