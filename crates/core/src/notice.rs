//! Server notices a console statement raised (PostgreSQL NOTICE / WARNING / INFO: RAISE NOTICE in a DO block, "already exists,
//! skipping" from IF NOT EXISTS), returned with its result (QueryResult::notices) for the console's Output.
//!
//! sqlx 0.8 reads a NoticeResponse while it runs the statement and only reports it as a `tracing` event (target
//! `sqlx::postgres::notice`, the level standing for the severity). It does so inside the poll of the future that drives the
//! connection: PostgreSQL connections have no background task. So `collect` runs the statement's future with a dispatcher of its
//! own (`WithSubscriber`), which tracing sets as the thread's default only while that future is polled and puts back after: a
//! notice lands in the collector of the statement whose connection read it, never in another console's, and nothing global is set.
//! Qurry's own lines (`log!`, crate::logfile) don't go through tracing, so the collector can't take them.
use serde::Serialize;
use std::sync::{Arc, Mutex};
use tracing::instrument::WithSubscriber;
use tracing::subscriber::Interest;
use tracing::{Event, Level, Metadata, span};

/// At most this many notices per statement, then one "… N more" line, so a chatty loop can't flood the Output
pub const NOTICE_CAP: usize = 100;

/// Where sqlx 0.8 reports PostgreSQL notices (sqlx-postgres connection/stream.rs)
const PG_TARGET: &str = "sqlx::postgres::notice";

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Notice {
    /// The server's word for it: NOTICE, WARNING, INFO. Empty on the "… N more" line after NOTICE_CAP
    pub level: String,
    pub message: String,
}

impl Notice {
    /// The Output line: `NOTICE: …`, or the message alone for the "… N more" line
    pub fn line(&self) -> String {
        match self.level.is_empty() {
            true => self.message.clone(),
            false => format!("{}: {}", self.level, self.message),
        }
    }
}

#[derive(Default)]
struct Kept {
    notices: Vec<Notice>,
    more: usize,
}

struct Collector(Arc<Mutex<Kept>>);

/// sqlx's level for a severity → the severity. DEBUG and LOG are left out; INFO and LOG share TRACE, and LOG reaches the client
/// only when client_min_messages was lowered to it
fn pg_level(l: &Level) -> Option<&'static str> {
    match *l {
        Level::WARN => Some("WARNING"),
        Level::INFO => Some("NOTICE"),
        Level::TRACE => Some("INFO"),
        _ => None,
    }
}

struct Message(String);

impl tracing::field::Visit for Message {
    fn record_str(&mut self, f: &tracing::field::Field, v: &str) {
        if f.name() == "message" {
            self.0 = v.to_string();
        }
    }
    fn record_debug(&mut self, f: &tracing::field::Field, v: &dyn std::fmt::Debug) {
        if f.name() == "message" && self.0.is_empty() {
            self.0 = format!("{v:?}");
        }
    }
}

impl tracing::Subscriber for Collector {
    fn register_callsite(&self, m: &'static Metadata<'static>) -> Interest {
        if m.target() == PG_TARGET { Interest::always() } else { Interest::never() }
    }
    fn enabled(&self, m: &Metadata<'_>) -> bool {
        m.target() == PG_TARGET
    }
    fn new_span(&self, _: &span::Attributes<'_>) -> span::Id {
        span::Id::from_u64(1)
    }
    fn record(&self, _: &span::Id, _: &span::Record<'_>) {}
    fn record_follows_from(&self, _: &span::Id, _: &span::Id) {}
    fn event(&self, e: &Event<'_>) {
        let Some(level) = pg_level(e.metadata().level()) else { return };
        let mut m = Message(String::new());
        e.record(&mut m);
        let Ok(mut k) = self.0.lock() else { return };
        if k.notices.len() < NOTICE_CAP {
            k.notices.push(Notice { level: level.into(), message: m.0 });
        } else {
            k.more += 1;
        }
    }
    fn enter(&self, _: &span::Id) {}
    fn exit(&self, _: &span::Id) {}
}

/// Run `f` (one statement on a PostgreSQL connection) and collect the notices its connection read meanwhile, in order, capped
pub(crate) async fn collect<F: Future>(f: F) -> (F::Output, Vec<Notice>) {
    let kept = Arc::new(Mutex::new(Kept::default()));
    let out = f.with_subscriber(tracing::Dispatch::new(Collector(kept.clone()))).await;
    let k = std::mem::take(&mut *kept.lock().unwrap_or_else(|e| e.into_inner()));
    let mut notices = k.notices;
    if k.more > 0 {
        notices.push(Notice { level: String::new(), message: format!("… {} more", k.more) });
    }
    (out, notices)
}

/// The notices a statement raised (collect) on its result. A failed statement keeps only its error
pub(crate) fn attach((mut r, notices): (crate::error::AppResult<crate::db::QueryResult>, Vec<Notice>)) -> crate::error::AppResult<crate::db::QueryResult> {
    if let Ok(q) = &mut r {
        q.notices = notices;
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn collects_only_pg_notices_in_order_with_their_level() {
        let (out, n) = collect(async {
            tracing::event!(target: "sqlx::postgres::notice", Level::INFO, message = "one");
            tracing::event!(target: "sqlx::postgres::notice", Level::WARN, message = "two");
            tracing::event!(target: "sqlx::postgres::notice", Level::TRACE, message = "three");
            tracing::event!(target: "sqlx::postgres::notice", Level::DEBUG, message = "debug");
            tracing::event!(target: "sqlx::query", Level::INFO, message = "a query log");
            7
        })
        .await;
        assert_eq!(out, 7);
        let lines: Vec<String> = n.iter().map(Notice::line).collect();
        assert_eq!(lines, ["NOTICE: one", "WARNING: two", "INFO: three"]);
    }

    #[tokio::test]
    async fn caps_with_a_more_line() {
        let (_, n) = collect(async {
            for i in 0..NOTICE_CAP + 5 {
                tracing::event!(target: "sqlx::postgres::notice", Level::INFO, message = i.to_string().as_str());
            }
        })
        .await;
        assert_eq!(n.len(), NOTICE_CAP + 1);
        assert_eq!(n[NOTICE_CAP - 1].line(), format!("NOTICE: {}", NOTICE_CAP - 1));
        assert_eq!(n[NOTICE_CAP].line(), "… 5 more");
    }

    /// Two statements polled on the same thread, in turns: each keeps its own notices
    #[tokio::test]
    async fn interleaved_futures_keep_their_own() {
        let one = |tag: &'static str| async move {
            for i in 0..3 {
                tracing::event!(target: "sqlx::postgres::notice", Level::INFO, message = format!("{tag}{i}").as_str());
                tokio::task::yield_now().await;
            }
        };
        let ((_, a), (_, b)) = tokio::join!(collect(one("a")), collect(one("b")));
        assert_eq!(a.iter().map(|n| n.message.as_str()).collect::<Vec<_>>(), ["a0", "a1", "a2"]);
        assert_eq!(b.iter().map(|n| n.message.as_str()).collect::<Vec<_>>(), ["b0", "b1", "b2"]);
        // Outside a collect nothing is kept anywhere
        tracing::event!(target: "sqlx::postgres::notice", Level::INFO, message = "stray");
    }

    /// A console session on QURRY_TEST_PG (crate::testenv)
    async fn session() -> Option<crate::session::Session> {
        let s = crate::testenv::pg()?;
        let pool = crate::db::connect(&s.cfg, s.password.as_deref()).await.unwrap();
        Some(crate::session::Session::open(&pool).await.unwrap())
    }

    fn lines(q: &crate::db::QueryResult) -> Vec<String> {
        q.notices.iter().map(Notice::line).collect()
    }

    #[tokio::test]
    #[ignore]
    async fn pg_do_block_notices_with_their_level() {
        let Some(mut s) = session().await else { return };
        let q = s.run("DO $$ BEGIN RAISE NOTICE 'role % skipped', 'app_owner'; RAISE WARNING 'careful'; RAISE INFO 'fyi'; RAISE DEBUG 'hidden'; END $$", 500).await.unwrap();
        assert_eq!(lines(&q), ["NOTICE: role app_owner skipped", "WARNING: careful", "INFO: fyi"]);
        // The next statement starts empty
        assert!(s.run("select 1", 500).await.unwrap().notices.is_empty());
        // A function raising them in a SELECT (read through the cursor)
        s.run("create or replace function pg_temp.qurry_shout() returns int language plpgsql as $$ begin raise notice 'from a select'; return 1; end $$", 500).await.unwrap();
        assert_eq!(lines(&s.run("select pg_temp.qurry_shout()", 500).await.unwrap()), ["NOTICE: from a select"]);
        // A statement that fails keeps its error; notices never fail one that runs
        assert!(s.run("DO $$ BEGIN RAISE NOTICE 'before'; RAISE EXCEPTION 'boom'; END $$", 500).await.is_err());
        assert!(s.run("select 2", 500).await.unwrap().notices.is_empty());
    }

    #[tokio::test]
    #[ignore]
    async fn pg_if_not_exists_skipping_notice() {
        let Some(mut s) = session().await else { return };
        s.run("drop table if exists qurry_notice_t", 500).await.unwrap();
        assert!(s.run("create table if not exists qurry_notice_t (id int)", 500).await.unwrap().notices.is_empty());
        let q = s.run("create table if not exists qurry_notice_t (id int)", 500).await.unwrap();
        assert_eq!(lines(&q), ["NOTICE: relation \"qurry_notice_t\" already exists, skipping"]);
        s.run("drop table qurry_notice_t", 500).await.unwrap();
    }

    #[tokio::test]
    #[ignore]
    async fn pg_notices_in_a_manual_transaction() {
        let Some(mut s) = session().await else { return };
        s.set_manual(true).await.unwrap();
        let q = s.run("DO $$ BEGIN RAISE NOTICE 'in the transaction'; END $$", 500).await.unwrap();
        assert!(q.in_transaction);
        assert_eq!(lines(&q), ["NOTICE: in the transaction"]);
        // The server's own warning about a BEGIN inside the open transaction
        let q = s.run("begin", 500).await.unwrap();
        assert_eq!(lines(&q), ["WARNING: there is already a transaction in progress"]);
        s.rollback_tx().await.unwrap();
    }

    #[tokio::test]
    #[ignore]
    async fn pg_cap_per_statement() {
        let Some(mut s) = session().await else { return };
        let q = s.run(&format!("DO $$ BEGIN FOR i IN 1..{} LOOP RAISE NOTICE 'n%', i; END LOOP; END $$", NOTICE_CAP + 50), 500).await.unwrap();
        assert_eq!(q.notices.len(), NOTICE_CAP + 1);
        assert_eq!(q.notices[0].line(), "NOTICE: n1");
        assert_eq!(q.notices[NOTICE_CAP].line(), "… 50 more");
    }

    /// Two consoles running at once, on other threads and in turns: each gets only the notices its own connection read
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[ignore]
    async fn pg_two_consoles_keep_their_own_notices() {
        let Some(srv) = crate::testenv::pg() else { return };
        // Two tabs on one data source: two connections of the same pool
        let pool = crate::db::connect(&srv.cfg, srv.password.as_deref()).await.unwrap();
        let (a, b) = (crate::session::Session::open(&pool).await.unwrap(), crate::session::Session::open(&pool).await.unwrap());
        let run = |mut s: crate::session::Session, tag: &'static str| {
            tokio::spawn(async move {
                let sql = format!("DO $$ BEGIN FOR i IN 1..5 LOOP RAISE NOTICE '{tag}%', i; PERFORM pg_sleep(0.05); END LOOP; END $$");
                let mut seen = Vec::new();
                for _ in 0..3 {
                    seen.extend(lines(&s.run(&sql, 500).await.unwrap()));
                }
                seen
            })
        };
        let (a, b) = tokio::join!(run(a, "a"), run(b, "b"));
        let want = |tag: &str| (0..3).flat_map(|_| (1..=5).map(move |i| format!("NOTICE: {tag}{i}"))).collect::<Vec<_>>();
        assert_eq!(a.unwrap(), want("a"));
        assert_eq!(b.unwrap(), want("b"));
    }
}
