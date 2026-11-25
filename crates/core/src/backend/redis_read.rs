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
async fn part_bytes(m: &RedisConn, key: &[u8], count: &str, elements: i64, repeats: bool) -> AppResult<i64> {
    let n = element_count(m, count, key).await?;
    if n <= 0 || elements <= 0 {
        return Ok(0);
    }
    let usage = memory_usage(m, key, if n <= EXACT_ELEMENTS { 0 } else { MEMORY_SAMPLES }).await?;
    Ok(share(usage, elements, n, repeats))
}

/// The key's memory shared out over `elements` of its `n`: at most all of it, unless elements may repeat (SRANDMEMBER k -1000
/// returns 1000 of a key's 10 elements)
fn share(usage: i64, elements: i64, n: i64, repeats: bool) -> i64 {
    if elements >= n && !repeats {
        return usage;
    }
    (usage as i128 * elements as i128 / n as i128).min(i64::MAX as i128) as i64
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
        bytes = part_bytes(m, &raw[p.key], p.count, p.elements, p.repeats).await?;
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

/// SCAN iterations (COUNT 1000 each, per cluster master) search_keys takes: 100,000 keys, so a needle nothing matches can't walk
/// a database of millions
const SEARCH_SCANS: usize = 100;

/// SCAN MATCH pattern for key names containing `query` in any ASCII case, glob characters matching themselves
fn contains_pattern(query: &str) -> String {
    let mut p = String::from("*");
    for c in query.chars() {
        match c {
            c if c.is_ascii_alphabetic() => p.push_str(&format!("[{}{}]", c.to_ascii_lowercase(), c.to_ascii_uppercase())),
            '*' | '?' | '[' | ']' | '\\' => p.push_str(&format!("\\{c}")),
            c => p.push(c),
        }
    }
    p.push('*');
    p
}

/// search_objects on Redis: key names containing `query` (any ASCII case), at most `max`, from a bounded SCAN MATCH. complete =
/// the whole database was scanned
pub(crate) async fn search_keys(m: &RedisConn, query: &str, max: usize) -> AppResult<(Vec<Vec<u8>>, bool)> {
    let pattern = contains_pattern(query);
    let mut out: Vec<Vec<u8>> = vec![];
    let mut complete = true;
    'nodes: for mut node in m.scan_nodes() {
        let mut cur = 0u64;
        for _ in 0..SEARCH_SCANS {
            let (next, keys): (u64, Vec<Vec<u8>>) =
                redis::cmd("SCAN").arg(cur).arg("MATCH").arg(&pattern).arg("COUNT").arg(1000).query_async(&mut node).await.map_err(crate::redisdb::err)?;
            out.extend(keys);
            cur = next;
            if cur == 0 {
                break;
            }
            if out.len() >= max {
                complete = false;
                break 'nodes;
            }
        }
        complete &= cur == 0;
    }
    out.sort();
    out.dedup();
    out.truncate(max);
    Ok((out, complete))
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

/// A key name in an MCP answer: {"key": text}, or {"key_base64": …} when it isn't UTF-8 — redis_get takes either back as is
pub fn key_json(k: &[u8]) -> serde_json::Map<String, Value> {
    let mut m = serde_json::Map::new();
    match std::str::from_utf8(k) {
        Ok(s) => m.insert("key".into(), Value::from(s)),
        Err(_) => m.insert("key_base64".into(), Value::from(base64::engine::general_purpose::STANDARD.encode(k))),
    };
    m
}

/// The key redis_get was given: `key` as text, or `key_base64` (from redis_scan, for a name that isn't UTF-8)
pub fn key_arg(key: Option<&str>, key_base64: Option<&str>) -> AppResult<Vec<u8>> {
    match (key, key_base64) {
        (Some(k), None) => Ok(k.as_bytes().to_vec()),
        (None, Some(b)) => base64::engine::general_purpose::STANDARD.decode(b.trim()).map_err(|e| AppError::Msg(format!("key_base64 isn't base64: {e}"))),
        (Some(_), Some(_)) => Err(AppError::Msg("Give key or key_base64, not both".into())),
        (None, None) => Err(AppError::Msg("key is required (or key_base64)".into())),
    }
}

/// A Redis reply in an MCP answer: bulk strings by value_json, the rest as the app shows them
pub(crate) fn reply_json(v: &RV) -> Value {
    match v {
        RV::BulkString(b) => value_json(b, b.len()),
        other => crate::redisdb::cell(other),
    }
}

/// An MCP read's answer (after check_read): the command's table, with SLOWLOG GET's commands masked to the command and key
/// (redis_cmd::slowlog_command) and text cells over VALUE_MAX cut (cut_cells)
pub(crate) async fn run(m: &RedisConn, line: &str, limit: usize) -> AppResult<QueryResult> {
    let args = crate::redisdb::parse_command(line.trim().trim_end_matches(';').trim())?;
    let word = |i: usize| args.get(i).map(|a| String::from_utf8_lossy(a).to_uppercase()).unwrap_or_default();
    let mut r = if (word(0).as_str(), word(1).as_str()) == ("SLOWLOG", "GET") { slowlog(m, &args, limit).await? } else { crate::redisdb::run(m, line, limit).await? };
    cut_cells(&mut r);
    Ok(r)
}

/// SLOWLOG GET [count]: one row per entry (id, unix time, microseconds, command, client address, client name); the command
/// without its values, which can hold anything another client sent (card numbers, tokens …)
async fn slowlog(m: &RedisConn, args: &[Vec<u8>], limit: usize) -> AppResult<QueryResult> {
    use crate::redisdb::{cell, text_of};
    let start = std::time::Instant::now();
    let mut c = redis::cmd("SLOWLOG");
    for a in &args[1..] {
        c.arg(a.as_slice());
    }
    let v: RV = c.query_async(&mut m.clone()).await.map_err(crate::redisdb::err)?;
    let RV::Array(entries) = v else { return Ok(crate::redisdb::table(&["result"], vec![vec![cell(&v)]], 1, start)) };
    let rows = entries
        .iter()
        .map(|e| {
            let f = match e {
                RV::Array(f) => f.as_slice(),
                _ => &[],
            };
            let at = |i: usize| f.get(i).map(cell).unwrap_or(Value::Null);
            let command = match f.get(3) {
                Some(RV::Array(words)) => Value::from(qurry_shared::redis_cmd::slowlog_command(&words.iter().map(text_of).collect::<Vec<_>>())),
                _ => Value::Null,
            };
            vec![at(0), at(1), at(2), command, at(4), at(5)]
        })
        .collect();
    Ok(crate::redisdb::table(&["id", "time", "duration_us", "command", "client", "client_name"], rows, limit, start))
}

/// An MCP query's answer: text cells over VALUE_MAX cut there (at a character boundary), their [row, column] in `cut`
fn cut_cells(r: &mut QueryResult) {
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

    /// #229: a key name that isn't UTF-8 goes out as base64 and comes back to the same bytes
    #[test]
    fn binary_key_names_round_trip() {
        let k = b"bin:\xff\xfe\x00k";
        let j = key_json(k);
        assert_eq!((j.get("key"), j.len()), (None, 1));
        assert_eq!(key_arg(None, j["key_base64"].as_str()).unwrap(), k);
        assert_eq!(key_json(b"user:1")["key"], json!("user:1"));
        assert_eq!(key_arg(Some("user:1"), None).unwrap(), b"user:1");
        assert!(key_arg(None, Some("not base64!")).is_err() && key_arg(Some("a"), Some("YQ==")).is_err() && key_arg(None, None).is_err());
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
        let keys = ["qurry:budget:l", "qurry:budget:h", "qurry:budget:big", "qurry:budget:s1", "qurry:budget:s2", "qurry:budget:s3", "qurry:budget:small"];
        let _: () = redis::cmd("DEL").arg(&keys[..]).query_async(&mut c).await.unwrap();
        // Made on the server (string.rep): sending megabytes could pass the response timeout while other tests run
        let fill = "local two = string.rep('x', 2097152) redis.call('RPUSH', KEYS[1], two, two, two) \
            redis.call('HSET', KEYS[2], 'f1', two, 'f2', two, 'f3', two) redis.call('SET', KEYS[3], string.rep('y', 5242880)) \
            for i = 4, 6 do redis.call('SET', KEYS[i], two) end redis.call('HSET', KEYS[7], 'a', '1', 'b', '2') return 1";
        let _: () = redis::cmd("EVAL").arg(fill).arg(keys.len()).arg(&keys[..]).query_async(&mut c).await.unwrap();
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
    fn search_matches_a_part_in_any_case() {
        assert_eq!(contains_pattern("Shop:1"), "*[sS][hH][oO][pP]:1*");
        assert_eq!(contains_pattern("a*b?[c]\\"), "*[aA]\\*[bB]\\?\\[[cC]\\]\\\\*");
        assert_eq!(contains_pattern("é"), "*é*");
    }

    /// #225: search_keys finds a key by any part of its name in any case, past what one SCAN page holds, and says when it stopped.
    /// Own qurry:search:* keys in db15
    #[tokio::test]
    #[ignore]
    async fn search_keys_scans_past_the_first_page() {
        let Some((st, m)) = state("db15").await else { return };
        let mut c = m.clone();
        let fill = "for i=1,3000 do redis.call(ARGV[1], 'qurry:search:k:'..i, 'v') end return 1";
        let _: () = redis::cmd("EVAL").arg(fill).arg(0).arg("SET").query_async(&mut c).await.unwrap();
        let _: () = redis::cmd("SET").arg("qurry:search:Shop:Leader*board").arg("x").query_async(&mut c).await.unwrap();
        let find = |q: &str, max: usize| st.redis_search_keys("rd::db15".into(), q.into(), max);
        for q in ["shop:leader*", "LEADER*BOARD", "Shop:Leader*board"] {
            assert_eq!(find(q, 10).await.unwrap(), (vec![b"qurry:search:Shop:Leader*board".to_vec()], true), "{q}");
        }
        // A glob character matches itself only
        assert_eq!(find("leaderb", 10).await.unwrap().0.len(), 0);
        let (some, complete) = find("qurry:search:k:", 5).await.unwrap();
        assert!(some.len() == 5 && !complete);
        let _: () = redis::cmd("EVAL").arg(fill.replace("ARGV[1], 'qurry:search:k:'..i, 'v'", "'DEL', 'qurry:search:k:'..i")).arg(0).query_async(&mut c).await.unwrap();
        let _: () = redis::cmd("DEL").arg("qurry:search:Shop:Leader*board").query_async(&mut c).await.unwrap();
    }

    /// A negative random count repeats elements (#224 review): SRANDMEMBER / HRANDFIELD of -1000 on a 3 MB key of 100 elements
    /// return about 30 MB, so they count every element they return. Own qurry:repeat:* keys in db9
    #[tokio::test]
    #[ignore]
    async fn repeating_random_reads_count_every_element() {
        let Some((st, m)) = state("db9").await else { return };
        let mut c = m.clone();
        let keys = ["qurry:repeat:s", "qurry:repeat:h"];
        let _: () = redis::cmd("DEL").arg(&keys[..]).query_async(&mut c).await.unwrap();
        let fill = "for i = 1, 100 do local v = string.rep('x', 30000)..i redis.call('SADD', KEYS[1], v) redis.call('HSET', KEYS[2], 'f'..i, v) end return 1";
        let _: () = redis::cmd("EVAL").arg(fill).arg(2).arg(&keys[..]).query_async(&mut c).await.unwrap();
        let read = |s: &str| st.run_read_only("rd::db9".into(), s.to_string(), 100);
        for over in ["SRANDMEMBER qurry:repeat:s -1000", "HRANDFIELD qurry:repeat:h -1000 WITHVALUES", "HRANDFIELD qurry:repeat:h -200 WITHVALUES"] {
            let e = read(over).await.unwrap_err().to_string();
            assert!(e.contains("per MCP read"), "{over}: {e}");
        }
        for within in ["SRANDMEMBER qurry:repeat:s -10", "SRANDMEMBER qurry:repeat:s 1000", "HRANDFIELD qurry:repeat:h 5 WITHVALUES"] {
            read(within).await.unwrap_or_else(|e| panic!("{within}: {e}"));
        }
        let _: () = redis::cmd("DEL").arg(&keys[..]).query_async(&mut c).await.unwrap();
    }

    /// At most all of the key's memory, unless the elements repeat (SRANDMEMBER k -1000 of a key of 100)
    #[test]
    fn a_part_shares_the_key_memory() {
        assert_eq!(share(1000, 10, 100, false), 100);
        assert_eq!(share(1000, 500, 100, false), 1000);
        assert_eq!(share(i64::MAX, i64::MAX - 1, i64::MAX, false), i64::MAX - 1);
        assert_eq!(share(1000, 500, 100, true), 5000);
        assert_eq!(share(i64::MAX, 1000, 1, true), i64::MAX);
    }
}
