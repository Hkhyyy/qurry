//! A Redis source's connection (#221): one connection for all its databases, and each command run in its own database when
//! callers on several databases send at once.
//! Real-server tests are #[ignore]d: QURRY_TEST_REDIS=host:port cargo test -p qurry-core --test redis_connections -- --ignored
//! They use db10, db11, db13 and db15 and keys `qurry-221:*`, removed at the end (no FLUSHDB)
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
