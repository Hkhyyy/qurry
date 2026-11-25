//! Redis through the real stdio server (qurry-mcp --allow-write): redis_scan, redis_get, query refusing writes, and execute refusing
//! writes that empty a key while partial ones run (#223); a hidden source's exact name is refused (#228).
//! QURRY_TEST_REDIS=host:port cargo test -p qurry-mcp --test redis -- --ignored. Own keys (qurry:mcp:*) in db10

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

const H: &str = "qurry:mcp:h";
const S: &str = "qurry:mcp:s";
const Z: &str = "qurry:mcp:z";
const L: &str = "qurry:mcp:l";
const X: &str = "qurry:mcp:x";

fn source(id: &str, name: &str, host: &str, port: &str, mcp: Option<&str>, production: bool) -> qurry_core::store::ConnectionConfig {
    let mut c: qurry_core::store::ConnectionConfig =
        serde_json::from_value(json!({ "id": id, "name": name, "kind": "redis", "host": host, "port": port.parse::<u16>().unwrap(), "user": "", "database": "db10" })).unwrap();
    c.options.mcp = mcp.map(String::from);
    c.options.confirm_changes = production;
    c
}

/// The server, one request at a time (it answers requests concurrently, so writes must not overlap)
struct Mcp {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    id: u64,
}

impl Mcp {
    fn request(&mut self, method: &str, params: Value) -> Value {
        self.send(method, params);
        let r = self.reply();
        assert_eq!(r["id"], json!(self.id), "{r}");
        r["result"].clone()
    }

    fn send(&mut self, method: &str, params: Value) {
        self.id += 1;
        writeln!(self.stdin, "{}", json!({ "jsonrpc": "2.0", "id": self.id, "method": method, "params": params })).unwrap();
    }

    fn reply(&mut self) -> Value {
        let mut line = String::new();
        self.stdout.read_line(&mut line).unwrap();
        serde_json::from_str(&line).unwrap_or_else(|e| panic!("not JSON-RPC: {line} ({e})"))
    }

    /// (isError, text) of a tool call
    fn call(&mut self, name: &str, args: Value) -> (bool, String) {
        let r = self.request("tools/call", json!({ "name": name, "arguments": args }));
        (r["isError"] == json!(true), r["content"][0]["text"].as_str().unwrap_or_default().to_string())
    }

    fn execute(&mut self, statements: &[&str]) -> (bool, String) {
        self.call("execute", json!({ "data_source": "Demo Redis ProdCopy", "statements": statements }))
    }

    /// HLEN, SCARD, ZCARD, LLEN, XLEN of the test keys, read with query
    fn sizes(&mut self) -> Vec<i64> {
        [("HLEN", H), ("SCARD", S), ("ZCARD", Z), ("LLEN", L), ("XLEN", X)]
            .iter()
            .map(|(cmd, k)| {
                let (err, t) = self.call("query", json!({ "data_source": "Demo Redis ProdCopy", "sql": format!("{cmd} {k}") }));
                assert!(!err, "{cmd} {k}: {t}");
                let v: Value = serde_json::from_str(&t).unwrap();
                v["rows"][0][0].as_i64().or_else(|| v["rows"][0][0].as_str().and_then(|s| s.parse().ok())).unwrap_or_else(|| panic!("{t}"))
            })
            .collect()
    }
}

#[tokio::test]
#[ignore]
async fn redis_tools_over_stdio() {
    let Some(addr) = qurry_core::testenv::var("QURRY_TEST_REDIS") else { return };
    let (host, port) = addr.rsplit_once(':').expect("QURRY_TEST_REDIS=host:port");
    let dir = std::env::temp_dir().join(format!("qurry-mcp-redis-{}-{}", std::process::id(), std::time::UNIX_EPOCH.elapsed().unwrap().as_nanos()));
    let config = dir.join("config");
    // The sources and the test keys, written the way the app does
    let st = qurry_core::open_state(Some(config.clone())).unwrap();
    st.save_connection(source("rw", "Demo Redis ProdCopy", host, port, Some("write"), false), None, None).await.unwrap();
    // A Production source with no MCP setting: hidden. Its exact name is part of the shared one's name
    st.save_connection(source("prod", "Demo Redis Prod", host, port, None, true), None, None).await.unwrap();
    let run = |s: String| st.run_query("rw::db10".into(), s, None, None);
    run(format!("DEL {H} {S} {Z} {L} {X}")).await.unwrap();
    for s in [format!("HSET {H} name n email e tier t"), format!("SADD {S} sale new hot"), format!("ZADD {Z} 1 a 2 b"), format!("RPUSH {L} x x y"),
        format!("XADD {X} 1-0 a b"), format!("XADD {X} 2-0 a b")] {
        run(s).await.unwrap();
    }

    let mut child = Command::new(env!("CARGO_BIN_EXE_qurry-mcp"))
        .arg("--allow-write")
        .arg("--config-dir")
        .arg(&config)
        .env("HOME", &dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let (stdin, stdout) = (child.stdin.take().unwrap(), BufReader::new(child.stdout.take().unwrap()));
    let mut m = Mcp { child, stdin, stdout, id: 0 };
    m.request("initialize", json!({ "protocolVersion": "2025-06-18" }));

    // redis_scan and redis_get
    let (err, t) = m.call("redis_scan", json!({ "data_source": "Demo Redis ProdCopy", "pattern": "qurry:mcp:*" }));
    assert!(!err && [H, S, Z, L, X].iter().all(|k| t.contains(k)), "{t}");
    let (err, t) = m.call("redis_get", json!({ "data_source": "Demo Redis ProdCopy", "key": H }));
    let v: Value = serde_json::from_str(&t).unwrap_or_else(|_| panic!("{t}"));
    assert!(!err && v["type"] == "hash" && v["length"] == 3, "{t}");

    // query never writes
    let (err, t) = m.call("query", json!({ "data_source": "Demo Redis ProdCopy", "sql": format!("HDEL {H} name") }));
    assert!(err && t.contains("not a read-only command"), "{t}");
    assert_eq!(m.sizes(), [3, 3, 2, 3, 2]);

    // #228: the hidden source's exact name (any case) doesn't land on the shared source that contains it
    for name in ["Demo Redis Prod", "demo redis prod", "prod"] {
        let (err, t) = m.call("redis_get", json!({ "data_source": name, "key": H }));
        assert!(err && t.contains("is not shared with MCP"), "{name}: {t}");
        let (err, t) = m.call("execute", json!({ "data_source": name, "statements": [format!("HDEL {H} name")] }));
        assert!(err && t.contains("is not shared with MCP"), "{name}: {t}");
    }

    // #223: writes that empty a key or a stream are refused, the keys stay as they were
    for bad in [
        vec![format!("SREM {S} sale new hot")],
        vec![format!("SREM {S} sale new hot hot")],
        vec![format!("HDEL {H} name email tier")],
        vec![format!("HDEL {H} name email"), format!("HDEL {H} tier")],
        vec![format!("ZREM {Z} a b")],
        vec![format!("XADD {X} MAXLEN 0 * a b")],
        vec![format!("XADD {X} MINID ~ 9-0 * a b")],
        vec![format!("XDEL {X} 1-0 2-0")],
        vec![format!("XTRIM {X} MAXLEN 0")],
        vec![format!("LREM {L} 0 x")],
        vec![format!("LREM {L} 3 x")],
        vec![format!("LPOP {L} 3")],
        vec![format!("LTRIM {L} 1 0")],
        vec![format!("SPOP {S} 3")],
        vec![format!("ZPOPMIN {Z} 2")],
        vec![format!("GETDEL {H}")],
        // Trimming options in any order (LIMIT first)
        vec![format!("XADD {X} LIMIT 0 MAXLEN ~ 0 * a b")],
        vec![format!("XADD {X} NOMKSTREAM LIMIT 5 MINID ~ 9-0 * a b")],
        // A string write onto a hash, set or stream would replace it whole
        vec![format!("SET {H} x")],
        vec![format!("MSET qurry:mcp:new 1 {S} x")],
        vec![format!("APPEND {X} x")],
    ] {
        let s: Vec<&str> = bad.iter().map(String::as_str).collect();
        let (err, t) = m.execute(&s);
        assert!(err && t.contains("run it in Qurry"), "{bad:?}: {t}");
        assert_eq!(m.sizes(), [3, 3, 2, 3, 2], "{bad:?} changed nothing");
    }

    // Partial removals and a stream append without trimming run
    for ok in [format!("HDEL {H} name"), format!("SREM {S} sale sale"), format!("ZREM {Z} a"), format!("XADD {X} * a b"), format!("LPOP {L} 1"),
        format!("LREM {L} 1 x"), format!("XDEL {X} 1-0")] {
        let (err, t) = m.execute(&[&ok]);
        assert!(!err, "{ok}: {t}");
    }
    assert_eq!(m.sizes(), [2, 2, 1, 1, 2]);
    let (err, t) = m.call("redis_get", json!({ "data_source": "Demo Redis ProdCopy", "key": H }));
    assert!(!err && t.contains("\"hash\""), "{t}");

    // Two calls sent at once, each taking out one of the two fields: one runs, the other sees the field it left (the server answers
    // requests concurrently; the count and the run are one at a time)
    for round in 0..5 {
        run(format!("HSET {H} name n email e")).await.unwrap();
        run(format!("HDEL {H} tier")).await.unwrap();
        m.send("tools/call", json!({ "name": "execute", "arguments": { "data_source": "Demo Redis ProdCopy", "statements": [format!("HDEL {H} name")] } }));
        m.send("tools/call", json!({ "name": "execute", "arguments": { "data_source": "Demo Redis ProdCopy", "statements": [format!("HDEL {H} email")] } }));
        let errors = [m.reply(), m.reply()].iter().filter(|r| r["result"]["isError"] == json!(true)).count();
        assert_eq!((errors, m.sizes()[0]), (1, 1), "round {round}");
    }

    drop(m.stdin);
    let _ = m.child.wait();
    run(format!("DEL {H} {S} {Z} {L} {X}")).await.unwrap();
    st.disconnect("rw".into()).await.unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}
