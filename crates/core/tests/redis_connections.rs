//! A Redis source's connection (#221, #226): one connection for all its databases, each command run in its own database when
//! callers on several databases send at once, and a reply that doesn't come ends at redisdb::RESPONSE_TIMEOUT for that source only.
//! Real-server tests are #[ignore]d: QURRY_TEST_REDIS=host:port cargo test -p qurry-core --test redis_connections -- --ignored
//! They use db10, db11, db13 and db15 and keys `qurry-221:*`, removed at the end (no FLUSHDB). Commands that would change the
//! shared connection are refused, and blocking ones run on a connection of their own; RESET also on QURRY_TEST_REDIS_AUTH
use qurry_core::app::AppState;
use serde_json::json;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering::SeqCst};
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const DBS: [i64; 4] = [10, 11, 13, 15];

fn source(id: &str, port: u16) -> qurry_core::store::ConnectionConfig {
    serde_json::from_value(json!({ "id": id, "name": id, "kind": "redis", "host": "127.0.0.1", "port": port, "user": "", "database": "0" })).unwrap()
}

/// A config folder with the given Redis sources (removed when the TempDir drops)
async fn state(sources: &[(&str, u16)]) -> (AppState, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let st = qurry_core::open_state(Some(dir.path().to_path_buf())).unwrap();
    for (id, port) in sources {
        st.save_connection(source(id, *port), None, None).await.unwrap();
    }
    (st, dir)
}

/// QURRY_TEST_REDIS as (address, port)
fn server() -> Option<(String, u16)> {
    let addr = qurry_core::testenv::var("QURRY_TEST_REDIS")?;
    let port = addr.rsplit_once(':').and_then(|(_, p)| p.parse().ok()).expect("QURRY_TEST_REDIS=host:port");
    Some((addr, port))
}

/// A TCP proxy in front of the server that counts its connections and can hold back the server's replies (`stall`)
#[derive(Default)]
struct Proxy {
    open: AtomicUsize,
    peak: AtomicUsize,
    total: AtomicUsize,
    stall: AtomicBool,
}

async fn proxy(upstream: String) -> (u16, Arc<Proxy>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let p = Arc::new(Proxy::default());
    let counts = p.clone();
    tokio::spawn(async move {
        while let Ok((client, _)) = listener.accept().await {
            let (p, upstream) = (counts.clone(), upstream.clone());
            tokio::spawn(async move {
                p.peak.fetch_max(p.open.fetch_add(1, SeqCst) + 1, SeqCst);
                p.total.fetch_add(1, SeqCst);
                if let Ok(server) = tokio::net::TcpStream::connect(&upstream).await {
                    let ((mut cr, mut cw), (mut sr, mut sw)) = (client.into_split(), server.into_split());
                    let down = async {
                        let mut buf = vec![0u8; 64 << 10];
                        loop {
                            while p.stall.load(SeqCst) {
                                tokio::time::sleep(Duration::from_millis(20)).await;
                            }
                            match sr.read(&mut buf).await {
                                Ok(0) | Err(_) => break,
                                Ok(n) if cw.write_all(&buf[..n]).await.is_err() => break,
                                Ok(_) => {}
                            }
                        }
                    };
                    tokio::select! {
                        _ = tokio::io::copy(&mut cr, &mut sw) => {}
                        () = down => {}
                    }
                }
                p.open.fetch_sub(1, SeqCst);
            });
        }
    });
    (port, p)
}

/// A connection of the test's own to database `db`, to check what landed where
async fn direct(addr: &str, db: i64) -> redis::aio::MultiplexedConnection {
    redis::Client::open(format!("redis://{addr}/{db}")).unwrap().get_multiplexed_async_connection().await.unwrap()
}

async fn clean(addr: &str, keys: &[String]) {
    for db in DBS {
        let _: () = redis::cmd("DEL").arg(keys).query_async(&mut direct(addr, db).await).await.unwrap();
    }
}

fn target(id: &str, db: i64) -> String {
    format!("{id}::db{db}")
}

/// The explorer, the browser, the terminal, a console and MCP on several databases at once open one connection (#221: dozens
/// opened at once through an SSM tunnel were reset)
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore]
async fn a_redis_source_opens_one_connection_for_all_its_databases() {
    let Some((addr, _)) = server() else { return };
    let (port, p) = proxy(addr).await;
    let (st, _dir) = state(&[("rq", port)]).await;
    let st = &st;
    let mut calls: Vec<futures::future::BoxFuture<'_, Result<(), String>>> = vec![
        Box::pin(async move { st.redis_overview("rq".into()).await.map(drop).map_err(|e| e.to_string()) }),
        Box::pin(async move { st.list_databases("rq".into()).await.map(drop).map_err(|e| e.to_string()) }),
    ];
    for db in DBS {
        let t = target("rq", db);
        calls.push(Box::pin(async move { st.redis_scan(target("rq", db), "0".into(), "*".into(), 50, None).await.map(drop).map_err(|e| e.to_string()) }));
        calls.push(Box::pin(async move { st.redis_key_names(target("rq", db), "qurry-221:".into(), 10).await.map(drop).map_err(|e| e.to_string()) }));
        calls.push(Box::pin(async move { st.run_read_only(target("rq", db), "DBSIZE".into(), 10).await.map(drop).map_err(|e| e.to_string()) }));
        calls.push(Box::pin(async move { st.run_query(t, "DBSIZE".into(), None, Some(format!("tab-{db}"))).await.map(drop).map_err(|e| e.to_string()) }));
    }
    for r in futures::future::join_all(calls).await {
        r.unwrap();
    }
    assert_eq!((p.total.load(SeqCst), p.peak.load(SeqCst)), (1, 1), "connections opened, and open at once");
    // A database the server doesn't have fails where it was asked for, and the connection goes on
    let e = st.redis_exec("rq::db99999".into(), vec!["DBSIZE".into()]).await.unwrap_err().to_string();
    assert!(e.contains("DB index is out of range"), "{e}");
    st.redis_exec("rq::db10".into(), vec!["PING".into()]).await.unwrap();
    assert_eq!(p.total.load(SeqCst), 1);
    // A blocking command opens one more, for itself, and closes it when it ends
    st.redis_exec_line("rq::db10".into(), "BLPOP qurry-221:none 0.1".into()).await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!((p.total.load(SeqCst), p.open.load(SeqCst)), (2, 1));
}

/// A server that drops every connection: callers on all databases at once get the one connect's error right away, instead of
/// each trying again for about 9.5s (redis' default retries)
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn callers_on_many_databases_share_one_failed_connect() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let accepted = Arc::new(AtomicUsize::new(0));
    let n = accepted.clone();
    tokio::spawn(async move {
        while let Ok((s, _)) = listener.accept().await {
            n.fetch_add(1, SeqCst);
            drop(s);
        }
    });
    let (st, _dir) = state(&[("down", port)]).await;
    let st = &st;
    let started = Instant::now();
    let calls = (0..16).map(|db| async move { st.redis_scan(target("down", db), "0".into(), "*".into(), 50, None).await });
    let errors: Vec<String> = futures::future::join_all(calls).await.into_iter().map(|r| r.err().expect("no server").to_string()).collect();
    assert!(started.elapsed() < Duration::from_secs(2), "{:?}", started.elapsed());
    assert_eq!(accepted.load(SeqCst), 1, "one connect: {errors:?}");
    assert!(errors.iter().all(|e| e == &errors[0]), "{errors:?}");
}

/// Commands sent at once on different databases of one source run in their own database, whichever way they come: the
/// browser (redis_exec), the terminal (redis_exec_line), a console on the pool or in its session (run_query), MCP
/// (run_read_only, execute). The same key names in four databases each end with their own value
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
#[ignore]
async fn commands_on_databases_sent_at_once_stay_in_their_own() {
    let Some((addr, port)) = server() else { return };
    let (st, _dir) = state(&[("rs", port)]).await;
    let st = &st;
    let keys: Vec<String> = (0..60).map(|i| format!("qurry-221:{i}")).chain(DBS.iter().map(|db| format!("qurry-221:only-db{db}"))).collect();
    clean(&addr, &keys).await;
    let calls = DBS.iter().flat_map(|&db| {
        (0..60).map(move |i| async move {
            let (t, k, v) = (target("rs", db), format!("qurry-221:{i}"), format!("db{db}-{i}"));
            let set = format!("SET {k} {v}");
            let wrote = match i % 4 {
                0 => st.redis_exec(t.clone(), vec!["SET".into(), k.clone(), v.clone()]).await.map(drop),
                1 => st.run_query(t.clone(), set, None, None).await.map(drop),
                2 => st.run_query(t.clone(), set, None, Some(format!("tab-{db}"))).await.map(drop),
                _ => st.execute_batch_prepared(t.clone(), vec![set]).await.map(drop),
            };
            wrote.unwrap_or_else(|e| panic!("{t} {k}: {e}"));
            let got = match i % 2 {
                0 => st.redis_exec_line(t.clone(), format!("GET {k}")).await.unwrap(),
                _ => st.run_read_only(t.clone(), format!("GET {k}"), 1).await.unwrap().rows[0][0].clone(),
            };
            assert_eq!(got, json!(v), "{t} {k}");
        })
    });
    futures::future::join_all(calls).await;
    // SCAN-based reads (the browser's key list, the terminal's completion) on all databases at once see their own keys only
    for db in DBS {
        st.redis_exec(target("rs", db), vec!["SET".into(), format!("qurry-221:only-db{db}"), "1".into()]).await.unwrap();
    }
    let scans = DBS.iter().cycle().take(DBS.len() * 10).map(|&db| async move {
        let t = target("rs", db);
        let only = vec![format!("qurry-221:only-db{db}")];
        assert_eq!(st.redis_key_names(t.clone(), "qurry-221:only-".into(), 10).await.unwrap(), only, "{t}");
        let page = st.redis_scan(t.clone(), "0".into(), "qurry-221:only-*".into(), 100, None).await.unwrap();
        assert_eq!(page.keys.iter().map(|k| k.key.clone()).collect::<Vec<_>>(), only, "{t}");
    });
    futures::future::join_all(scans).await;
    for db in DBS {
        let mut c = direct(&addr, db).await;
        let vals: Vec<Option<String>> = redis::cmd("MGET").arg(&keys[..60]).query_async(&mut c).await.unwrap();
        let want: Vec<Option<String>> = (0..60).map(|i| Some(format!("db{db}-{i}"))).collect();
        assert_eq!(vals, want, "db{db}");
    }
    clean(&addr, &keys).await;
}

/// A reply that comes after redis' own 500 ms default still arrives; one that doesn't come ends at the response timeout with
/// its cause. Meanwhile a call that is cancelled stops waiting at once, and MCP writes on another source aren't held up by the
/// stalled one's (the write lock is per source)
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore]
async fn a_stalled_server_ends_at_the_response_timeout_and_holds_up_only_its_source() {
    let Some((addr, port)) = server() else { return };
    let (slow_port, p) = proxy(addr.clone()).await;
    let (st, _dir) = state(&[("slow", slow_port), ("fast", port)]).await;
    let st = &st;
    let keys = vec!["qurry-221:stall".to_string()];
    clean(&addr, &keys).await;
    st.redis_exec("slow::db10".into(), vec!["SET".into(), keys[0].clone(), "v".into()]).await.unwrap();
    // A slow reply (1.5s)
    p.stall.store(true, SeqCst);
    let late = async {
        tokio::time::sleep(Duration::from_millis(1500)).await;
        p.stall.store(false, SeqCst);
    };
    let (got, ()) = tokio::join!(st.redis_exec_line("slow::db10".into(), "GET qurry-221:stall".into()), late);
    assert_eq!(got.unwrap(), json!("v"), "a reply after 1.5s");
    // No reply: an MCP write on the stalled source holds its lock until the timeout; one on another source goes on
    p.stall.store(true, SeqCst);
    let started = Instant::now();
    let stalled = st.execute_batch_prepared("slow::db10".into(), vec!["SET qurry-221:stall w".into()]);
    let others = async {
        tokio::time::sleep(Duration::from_millis(300)).await;
        let t = Instant::now();
        st.execute_batch_prepared("fast::db10".into(), vec!["SET qurry-221:stall x".into()]).await.unwrap();
        let fast = t.elapsed();
        // Cancelled (a tab closed, Cancel): dropping the call stops its wait
        let t = Instant::now();
        assert!(tokio::time::timeout(Duration::from_millis(500), st.redis_exec_line("slow::db11".into(), "PING".into())).await.is_err());
        (fast, t.elapsed())
    };
    let (r, (fast, cancelled)) = tokio::join!(stalled, others);
    let waited = started.elapsed();
    let e = r.unwrap_err().to_string();
    assert!(e.contains("didn't answer within 30s"), "{e}");
    assert!(waited >= Duration::from_secs(30) && waited < Duration::from_secs(36), "{waited:?}");
    assert!(fast < Duration::from_secs(2), "the other source's write waited {fast:?}");
    assert!(cancelled < Duration::from_secs(1), "{cancelled:?}");
    p.stall.store(false, SeqCst);
    // The source works again once the server answers
    assert_eq!(st.redis_exec_line("slow::db10".into(), "GET qurry-221:stall".into()).await.unwrap(), json!("x"));
    clean(&addr, &keys).await;
}

/// MCP execute on Redis holds its source's write lock for all its commands: a call with too many is refused before anything
/// connects (the source's port has no server)
#[tokio::test]
async fn an_mcp_execute_on_redis_takes_at_most_100_commands() {
    let (st, _dir) = state(&[("cap", 1)]).await;
    let e = st.execute_batch_prepared("cap".into(), vec!["SET k v".into(); 101]).await.unwrap_err().to_string();
    assert!(e.contains("At most 100 Redis commands per execute call"), "{e}");
}

/// Two keys on db11 with their own values, read while another caller's command ran first: each GET gets its own key's value
async fn replies_stay_in_order(st: &AppState, id: &str) {
    st.redis_exec(target(id, 11), vec!["SET".into(), "qurry-221:a".into(), "va".into()]).await.unwrap();
    st.redis_exec(target(id, 11), vec!["SET".into(), "qurry-221:b".into(), "vb".into()]).await.unwrap();
    for _ in 0..3 {
        let read = |k: &'static str| tokio::time::timeout(Duration::from_secs(2), st.redis_exec(target(id, 11), vec!["GET".into(), k.into()]));
        let (a, b) = tokio::join!(read("qurry-221:a"), read("qurry-221:b"));
        assert_eq!((a.expect("GET a answered").unwrap(), b.expect("GET b answered").unwrap()), (json!("va"), json!("vb")));
    }
}

/// CLIENT REPLY SKIP / OFF from the Terminal would make the shared connection's next reply go to another caller (H1): refused,
/// from the Terminal line and the argument array alike, and the other databases' replies stay their own
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore]
async fn client_reply_from_the_terminal_is_refused_and_replies_stay_in_order() {
    let Some((_, port)) = server() else { return };
    let (st, _dir) = state(&[("cr", port)]).await;
    for line in ["CLIENT REPLY SKIP", "client reply off"] {
        let r = tokio::time::timeout(Duration::from_secs(2), st.redis_exec_line(target("cr", 10), line.into())).await;
        let e = r.map(|r| r.map_err(|e| e.to_string())).unwrap_or_else(|_| Err("no answer".into())).unwrap_err();
        assert!(e.contains("would change the connection Qurry shares"), "{line}: {e}");
        replies_stay_in_order(&st, "cr").await;
    }
    let e = st.run_query(target("cr", 10), "CLIENT REPLY SKIP".into(), None, Some("tab".into())).await.unwrap_err().to_string();
    assert!(e.contains("would change the connection"), "{e}");
}

/// MULTI from the Terminal would queue the browser's edits on another database (H2), and SUBSCRIBE would take the connection
/// over (M1): refused, and a SET from the browser on another database is set at once
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore]
async fn multi_and_subscribe_from_the_terminal_are_refused_and_edits_elsewhere_run() {
    let Some((addr, port)) = server() else { return };
    let (st, _dir) = state(&[("mu", port)]).await;
    for args in [vec!["MULTI"], vec!["SUBSCRIBE", "qurry-221:c"], vec!["WATCH", "qurry-221:a"]] {
        let args: Vec<String> = args.into_iter().map(String::from).collect();
        let r = tokio::time::timeout(Duration::from_secs(2), st.redis_exec(target("mu", 10), args.clone())).await;
        let e = r.map(|r| r.map_err(|e| e.to_string())).unwrap_or_else(|_| Err("no answer".into())).unwrap_err();
        assert!(e.contains("would change the connection Qurry shares"), "{args:?}: {e}");
        let set = tokio::time::timeout(Duration::from_secs(2), st.redis_exec(target("mu", 11), vec!["SET".into(), "qurry-221:edit".into(), "now".into()])).await;
        assert_eq!(set.expect("SET answered").unwrap(), json!({ "status": "OK" }), "after {args:?}");
        let got: Option<String> = redis::cmd("GET").arg("qurry-221:edit").query_async(&mut direct(&addr, 11).await).await.unwrap();
        assert_eq!(got.as_deref(), Some("now"));
        replies_stay_in_order(&st, "mu").await;
    }
    clean(&addr, &["qurry-221:edit".into(), "qurry-221:a".into(), "qurry-221:b".into()]).await;
}

/// RESET from the Terminal would log the shared connection out on a server with a password (M1, NOAUTH until Disconnect): refused
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore]
async fn reset_from_the_terminal_is_refused_on_a_server_with_a_password() {
    let Some(e) = qurry_core::testenv::var("QURRY_TEST_REDIS_AUTH") else { return };
    let p: Vec<&str> = e.splitn(3, ':').collect();
    let dir = tempfile::tempdir().unwrap();
    let st = qurry_core::open_state(Some(dir.path().to_path_buf())).unwrap();
    st.save_connection(source("au", p[1].parse().unwrap()), Some(p[2].to_string()), None).await.unwrap();
    st.redis_exec("au".into(), vec!["PING".into()]).await.unwrap();
    for line in ["RESET", "AUTH default nope", "HELLO 3"] {
        let e = st.redis_exec_line(target("au", 10), line.into()).await.unwrap_err().to_string();
        assert!(e.contains("would change the connection Qurry shares"), "{line}: {e}");
    }
    assert_eq!(st.redis_exec(target("au", 11), vec!["PING".into()]).await.unwrap(), json!({ "status": "PONG" }));
}

/// A blocking command (H3) runs on a connection of its own: the other databases' commands answer meanwhile, one cancelled or
/// timed out ends on the server, so it never pops what another client pushes later, and one that would wait forever is refused
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore]
async fn blocking_commands_run_on_their_own_connection_and_end_when_cancelled() {
    let Some((addr, _)) = server() else { return };
    let (proxy_port, p) = proxy(addr.clone()).await;
    let (st, _dir) = state(&[("bl", proxy_port)]).await;
    let st = &st;
    let q = "qurry-221:queue".to_string();
    clean(&addr, std::slice::from_ref(&q)).await;
    st.redis_exec(target("bl", 11), vec!["PING".into()]).await.unwrap();
    let shared = p.total.load(SeqCst);
    // Meanwhile: a GET on another database answers at once
    let blocked = st.redis_exec_line(target("bl", 10), format!("BLPOP {q} 2"));
    let other = async {
        tokio::time::sleep(Duration::from_millis(200)).await;
        let t = Instant::now();
        st.redis_exec(target("bl", 11), vec!["PING".into()]).await.unwrap();
        t.elapsed()
    };
    let (r, waited) = tokio::join!(blocked, other);
    assert_eq!(r.unwrap(), json!(null), "BLPOP timed out empty");
    assert!(waited < Duration::from_millis(500), "the other database waited {waited:?}");
    assert_eq!(p.total.load(SeqCst), shared + 1, "one connection of its own");
    // Cancelled (a tab closed): what is pushed afterwards stays in the list
    assert!(tokio::time::timeout(Duration::from_millis(300), st.redis_exec_line(target("bl", 10), format!("BLPOP {q} 5"))).await.is_err());
    tokio::time::sleep(Duration::from_millis(300)).await;
    let mut c = direct(&addr, 10).await;
    let _: i64 = redis::cmd("RPUSH").arg(&q).arg("job").query_async(&mut c).await.unwrap();
    tokio::time::sleep(Duration::from_millis(500)).await;
    let n: i64 = redis::cmd("LLEN").arg(&q).query_async(&mut c).await.unwrap();
    assert_eq!(n, 1, "the cancelled BLPOP took the job");
    // A wait that ends gives its value; forever is refused
    assert_eq!(st.redis_exec_line(target("bl", 10), format!("BLPOP {q} 1")).await.unwrap(), json!([q, "job"]));
    for line in [format!("BLPOP {q} 0"), "XREAD BLOCK 0 STREAMS qurry-221:s $".into()] {
        let e = st.redis_exec_line(target("bl", 10), line.clone()).await.unwrap_err().to_string();
        assert!(e.contains("waits forever"), "{line}: {e}");
    }
    assert_eq!(p.open.load(SeqCst), 1, "the blocking connections are closed");
    clean(&addr, &[q]).await;
}
