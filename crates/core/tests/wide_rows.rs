//! Wide rows through a slow link (#183): a console's first page and its load-mores of a table whose rows hold a ~30 KB JSON
//! document and a ~2 KB note, read whole (gridCutChars = 0) and cut (4096), through a 300 KB/s proxy in front of the server.
//! Prints, per cut: time to the first page, time per load-more (3 pages), bytes the server sent per page, ru_maxrss.
//! #[ignore]d, on a scratch server (the table, about 160 MB, fills in under a minute and is dropped at the end):
//! QURRY_TEST_PG=host:port:user:db[:password] cargo test --release -p qurry-core --test wide_rows -- --ignored --nocapture
use qurry_core::app::AppState;
use qurry_core::store::ConnectionConfig;
use serde_json::json;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

const ROWS: i64 = 200_000;
/// Bytes per second, each way
const RATE: f64 = 300.0 * 1024.0;

/// A made-up order document of about 30 KB that compresses well (TOAST keeps it small on disk, as real documents of repeated
/// keys are), and a note of about 2 KB
const FILL: &str = r#"
create table qurry_demo_wide (id serial primary key, created_at timestamptz not null default now(), payload jsonb, note text);
insert into qurry_demo_wide (created_at, payload, note)
select now() - make_interval(secs => g),
       ('{"order": ' || g || ', "kind": "order.created", "items": [' ||
        rtrim(repeat('{"sku": "SKU-1042", "qty": 2, "price": 1250, "memo": "gift wrap, leave at the door"}, ', 330), ', ') || ']}')::jsonb,
       repeat('Made-up delivery note for a demo row. ', 54)
from generate_series(1, 200000) g;
"#;

/// Bytes the server sent / the client sent, through the proxy
#[derive(Default)]
struct Wire {
    down: AtomicU64,
    up: AtomicU64,
}

/// Copy one way at RATE (a token bucket with a 0.1 s burst), counting the bytes
async fn pipe(mut from: tokio::net::tcp::OwnedReadHalf, mut to: tokio::net::tcp::OwnedWriteHalf, count: Arc<Wire>, down: bool) {
    let mut buf = vec![0u8; 16 * 1024];
    let mut clock = Instant::now();
    loop {
        let n = match from.read(&mut buf).await {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        let now = Instant::now();
        clock = clock.max(now - Duration::from_millis(100)) + Duration::from_secs_f64(n as f64 / RATE);
        if clock > now {
            tokio::time::sleep(clock - now).await;
        }
        if to.write_all(&buf[..n]).await.is_err() {
            break;
        }
        (if down { &count.down } else { &count.up }).fetch_add(n as u64, Relaxed);
    }
    let _ = to.shutdown().await;
}

/// A throttling proxy to `upstream` on a free local port
async fn proxy(upstream: String, wire: Arc<Wire>) -> u16 {
    let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = l.local_addr().unwrap().port();
    tokio::spawn(async move {
        while let Ok((client, _)) = l.accept().await {
            let Ok(server) = TcpStream::connect(&upstream).await else { continue };
            let (cr, cw) = client.into_split();
            let (sr, sw) = server.into_split();
            tokio::spawn(pipe(cr, sw, wire.clone(), false));
            tokio::spawn(pipe(sr, cw, wire.clone(), true));
        }
    });
    port
}

fn max_rss_mb() -> f64 {
    let mut u: libc::rusage = unsafe { std::mem::zeroed() };
    unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut u) };
    // Bytes on macOS, kilobytes on Linux
    let b = if cfg!(target_os = "macos") { u.ru_maxrss as f64 } else { u.ru_maxrss as f64 * 1024.0 };
    b / (1024.0 * 1024.0)
}

#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn wide_rows_through_a_slow_link() {
    let Ok(env) = std::env::var("QURRY_TEST_PG") else { return };
    let mut p = env.splitn(5, ':');
    let (host, port, user, db, password) = (p.next().unwrap(), p.next().unwrap(), p.next().unwrap(), p.next().unwrap(), p.next().map(String::from));
    // The table, filled once on a direct connection
    let url = format!("postgres://{user}:{}@{host}:{port}/{db}", password.clone().unwrap_or_default());
    let direct = sqlx::PgPool::connect(&url).await.unwrap();
    let have: Option<i64> = sqlx::query_scalar("select count(*) from qurry_demo_wide").fetch_one(&direct).await.ok();
    if have != Some(ROWS) {
        let t = Instant::now();
        sqlx::raw_sql("drop table if exists qurry_demo_wide").execute(&direct).await.unwrap();
        sqlx::raw_sql(FILL).execute(&direct).await.unwrap();
        sqlx::raw_sql("vacuum analyze qurry_demo_wide").execute(&direct).await.unwrap();
        println!("filled {ROWS} rows in {:.1} s", t.elapsed().as_secs_f64());
    }
    let (size, row): (String, i32) = sqlx::query_as("select pg_size_pretty(pg_total_relation_size('qurry_demo_wide')), (select length(payload::text) + length(note) from qurry_demo_wide where id = 1)")
        .fetch_one(&direct)
        .await
        .unwrap();
    println!("table on disk {size}, one row's text {row} bytes");
    direct.close().await;

    let wire = Arc::new(Wire::default());
    let via = proxy(format!("{host}:{port}"), wire.clone()).await;
    let dir = tempfile::tempdir().unwrap();
    let mut st: AppState = qurry_core::open_state(Some(dir.path().to_path_buf())).unwrap();
    // As the GUI (the TUI's grid reads whole values)
    st.cut_grids = true;
    let cfg: ConnectionConfig = serde_json::from_value(json!({ "id": "w", "name": "wide", "kind": "postgres", "host": "127.0.0.1", "port": via, "user": user, "database": db })).unwrap();
    st.save_connection(cfg, password, None).await.unwrap();
    println!("cut | first page s | load-more s (3) | bytes/page (first, 3 more) | ru_maxrss MB");
    // Cut first: ru_maxrss only ever grows, so the whole read's peak shows after it
    for cut in [4096, 0] {
        st.store.set_grid_cut_chars(cut).unwrap();
        let tab = format!("t{cut}");
        // The session opens before the clock starts: its connection's handshake isn't the read
        st.run_query("w".into(), "select 1".into(), None, Some(tab.clone())).await.unwrap();
        let mut bytes = Vec::new();
        let mark = wire.down.load(Relaxed);
        let t = Instant::now();
        let r = st.run_query("w".into(), "select * from qurry_demo_wide".into(), None, Some(tab.clone())).await.unwrap();
        let first = t.elapsed().as_secs_f64();
        bytes.push(wire.down.load(Relaxed) - mark);
        assert_eq!((r.rows.len(), r.truncated), (500, true));
        assert_eq!(r.cut.is_empty(), cut == 0, "cut {cut}");
        let mut pages = Vec::new();
        for _ in 0..3 {
            let mark = wire.down.load(Relaxed);
            let t = Instant::now();
            let m = st.fetch_more(tab.clone(), None).await.unwrap();
            pages.push(t.elapsed().as_secs_f64());
            bytes.push(wire.down.load(Relaxed) - mark);
            assert_eq!(m.rows.len(), 500);
        }
        st.close_session(tab).await.unwrap();
        let p: Vec<String> = pages.iter().map(|s| format!("{s:.2}")).collect();
        println!("{cut} | {first:.2} | {} | {bytes:?} | {:.0}", p.join(", "), max_rss_mb());
    }
    println!("client sent {} bytes in all", wire.up.load(Relaxed));
    let direct = sqlx::PgPool::connect(&url).await.unwrap();
    sqlx::raw_sql("drop table qurry_demo_wide").execute(&direct).await.unwrap();
}
