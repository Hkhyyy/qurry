//! What each kind of data source can do, one `impl Backend` per kind (pg · mysql · redis · plugin).
//! Callers go through `dispatch!` on a `Pool`, which picks the impl at compile time (no `dyn`).
//! An operation a kind doesn't implement falls back to the default, an "<op> isn't available for <kind>" error.
//! The trait is grouped in sections (metadata first); each impl keeps the same sections in the same order.
use crate::audit;
use crate::db::QueryResult;
use crate::ddl::TableDef;
use crate::erd::Erd;
use crate::error::{AppError, AppResult};
use crate::fake::TableShape;
use crate::hints::{Hint, HintReq};
use crate::meta::{CompletionTable, FtsHit, FtsRequest, IndexEntry, SchemaInfo, SchemaObjects, TableDetail, TableStructure};
use crate::migrate::Model;
use crate::session::Session;
use crate::store::ConnectionConfig;
use crate::transfer::{Exporter, Tx};

pub(crate) mod mysql;
pub(crate) mod pg;
pub(crate) mod plugin;
pub(crate) mod redis;

/// Error for an operation this kind doesn't have
fn unsupported<T>(op: &str, b: &(impl Backend + ?Sized)) -> AppResult<T> {
    Err(AppError::Msg(format!("{op} isn't available for {}", b.label())))
}

pub(crate) trait Backend {
    /// How messages name this kind: "PostgreSQL", "the sqlite driver"
    fn label(&self) -> String;

    // ───── Metadata: meta.rs · hints.rs · erd.rs · ddl.rs · fake.rs ─────

    async fn list_databases(&self) -> AppResult<Vec<String>> {
        unsupported("Listing databases", self)
    }

    async fn list_schemas(&self) -> AppResult<Vec<SchemaInfo>> {
        unsupported("Listing schemas", self)
    }

    async fn list_objects(&self, _schema: &str) -> AppResult<SchemaObjects> {
        unsupported("Listing objects", self)
    }

    async fn table_detail(&self, _schema: &str, _table: &str) -> AppResult<TableDetail> {
        unsupported("Table details", self)
    }

    /// The Structure view: table_detail, with the sections this kind doesn't report left None
    async fn table_structure(&self, schema: &str, table: &str) -> AppResult<TableStructure> {
        Ok(self.table_detail(schema, table).await?.into())
    }

    /// kind: table|partitioned|foreign|view|matview|sequence|routine (Redis: key)
    async fn object_ddl(&self, _schema: &str, _name: &str, _kind: &str) -> AppResult<String> {
        unsupported("DDL", self)
    }

    async fn completion_meta(&self) -> AppResult<Vec<CompletionTable>> {
        unsupported("Autocomplete", self)
    }

    async fn object_index(&self) -> AppResult<Vec<IndexEntry>> {
        unsupported("Go to Database Object", self)
    }

    async fn full_text_search(&self, _r: &FtsRequest) -> AppResult<Vec<FtsHit>> {
        unsupported("Full-text search", self)
    }

    async fn column_hints(&self, _req: &HintReq) -> AppResult<Vec<Hint>> {
        unsupported("Value suggestions", self)
    }

    async fn erd(&self, _schemas: &[String]) -> AppResult<Erd> {
        unsupported("The diagram", self)
    }

    /// Modify Table…: the current structure
    async fn table_def(&self, _schema: &str, _table: &str) -> AppResult<TableDef> {
        unsupported("Modify Table", self)
    }

    /// Test data: columns, keys and foreign keys of a table
    async fn table_shape(&self, _schema: &str, _table: &str) -> AppResult<TableShape> {
        unsupported("Test data", self)
    }

    // ───── Connection: db.rs · app.rs ─────

    async fn server_version(&self) -> AppResult<String>;

    /// Close the pool's connections (Redis and driver plugins hold nothing to close)
    async fn close(&self) {}

    /// keep-alive (Pool::ping cuts it off after 5 seconds)
    async fn ping(&self) {}

    /// What to say when it connected without TLS (None: encrypted, this machine, an SSH tunnel, or a kind that doesn't tell)
    async fn plaintext_note(&self, _cfg: &ConnectionConfig) -> Option<String> {
        None
    }

    // ───── Running: db.rs · session.rs · app.rs ─────

    /// One statement on the pool, up to `limit` rows
    async fn run_query(&self, sql: &str, limit: usize) -> AppResult<QueryResult>;

    /// Rerun the query and write the full result to `out`
    async fn export(&self, sql: &str, out: &mut Exporter) -> AppResult<()>;

    /// MCP read, after the guard passed it (AppState::run_read_only): in a READ ONLY transaction with a time limit where the server has one
    async fn run_read_only(&self, sql: &str, limit: usize) -> AppResult<QueryResult>;

    /// MCP CSV export, after the guard passed it: the same rules as run_read_only for the full result
    async fn export_read_only(&self, sql: &str, out: &mut Exporter) -> AppResult<()>;

    /// A console tab's own session
    async fn open_session(&self) -> AppResult<Session>;

    /// Cancel the running query of a session (Session::backend_id). false: this kind can't
    async fn cancel(&self, _backend_id: i64) -> AppResult<bool> {
        Ok(false)
    }

    /// Statements in one transaction where the kind has them (see AppState::execute_batch). exact_one: each UPDATE/DELETE hits exactly
    /// 1 row; prepared: one statement per item, enforced by the server (MCP); snaps: the undo snapshot to take right before each statement
    async fn execute_batch(&self, statements: &[String], exact_one: bool, prepared: bool, snaps: &[Option<&str>]) -> AppResult<(u64, Snapshots)>;

    // ───── Jobs: migrate.rs · transfer.rs · audit.rs ─────

    /// Schema compare: the tables (columns, keys, foreign keys) and views of `schema`
    async fn schema_model(&self, _schema: &str) -> AppResult<Model> {
        unsupported("Schema compare", self)
    }

    /// One migration file: `stmts` in one transaction. A failing statement or COMMIT goes through `fail`
    async fn apply_migration(&self, _stmts: &[&str], _fail: impl Fn(String) -> AppError) -> AppResult<()> {
        unsupported("Running migrations", self)
    }

    /// Import: the transaction the rows go into
    async fn import_tx(&self) -> AppResult<Tx> {
        unsupported("Import", self)
    }

    /// Export: an approximate row count of `sql` for the progress bar (None: no estimate)
    async fn estimate_rows(&self, _sql: &str) -> Option<u64> {
        None
    }

    /// Undo: the columns of `table` as written in SQL (empty: no such table, or a kind that keeps no undo)
    async fn table_columns(&self, _table: &str) -> AppResult<Vec<audit::Col>> {
        Ok(vec![])
    }
}

/// Each statement's undo snapshot result with the rows its statement changed (None: no snapshot for it)
pub(crate) type Snapshots = Vec<Option<(AppResult<QueryResult>, u64)>>;

/// `dispatch!(pool, b => b.op(..).await)`: run the expression with `b` bound to the `Backend` behind `pool`
/// (`Pool` or `&Pool`). Each arm is compiled for its own kind.
macro_rules! dispatch {
    ($pool:expr, $b:ident => $body:expr) => {
        match &$pool {
            $crate::db::Pool::Pg($b) => $body,
            $crate::db::Pool::My($b) => $body,
            $crate::db::Pool::Redis(conn, db) => {
                let $b = &$crate::backend::redis::Redis { conn, db: *db };
                $body
            }
            $crate::db::Pool::Plugin($b) => $body,
        }
    };
}
pub(crate) use dispatch;

/// `apply_migration` for the SQL kinds: `stmts` in one transaction, a failing statement or COMMIT through `fail`
async fn run_in_tx<DB: sqlx::Database>(pool: &sqlx::Pool<DB>, stmts: &[&str], fail: impl Fn(String) -> AppError) -> AppResult<()>
where
    for<'c> &'c mut DB::Connection: sqlx::Executor<'c, Database = DB>,
{
    let mut tx = pool.begin().await?;
    for s in stmts {
        sqlx::Executor::execute(&mut *tx, sqlx::raw_sql(s)).await.map_err(|e| fail(e.to_string()))?;
    }
    tx.commit().await.map_err(|e| fail(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A kind that implements nothing but its name
    struct Bare;

    impl Backend for Bare {
        fn label(&self) -> String {
            "the bare driver".into()
        }
        async fn server_version(&self) -> AppResult<String> {
            unreachable!()
        }
        async fn run_query(&self, _sql: &str, _limit: usize) -> AppResult<QueryResult> {
            unreachable!()
        }
        async fn export(&self, _sql: &str, _out: &mut Exporter) -> AppResult<()> {
            unreachable!()
        }
        async fn run_read_only(&self, _sql: &str, _limit: usize) -> AppResult<QueryResult> {
            unreachable!()
        }
        async fn export_read_only(&self, _sql: &str, _out: &mut Exporter) -> AppResult<()> {
            unreachable!()
        }
        async fn open_session(&self) -> AppResult<Session> {
            unreachable!()
        }
        async fn execute_batch(&self, _statements: &[String], _exact_one: bool, _prepared: bool, _snaps: &[Option<&str>]) -> AppResult<(u64, Snapshots)> {
            unreachable!()
        }
    }

    fn msg<T>(r: AppResult<T>) -> String {
        match r {
            Ok(_) => panic!("expected the unsupported error"),
            Err(e) => e.to_string(),
        }
    }

    #[tokio::test]
    async fn defaults_say_what_isnt_available_for_which_kind() {
        let b = Bare;
        let req = HintReq { schema: "s".into(), table: "t".into(), column: "c".into(), path: vec![], keys: false, pairs: false };
        let fts = FtsRequest { text: "x".into(), mode: "contains".into(), match_case: false, columns: "text".into(), schemas: vec![], tables: vec![], limit: 1 };
        let got = [
            msg(b.list_databases().await),
            msg(b.list_schemas().await),
            msg(b.list_objects("s").await),
            msg(b.table_detail("s", "t").await),
            msg(b.table_structure("s", "t").await),
            msg(b.object_ddl("s", "t", "table").await),
            msg(b.completion_meta().await),
            msg(b.object_index().await),
            msg(b.full_text_search(&fts).await),
            msg(b.column_hints(&req).await),
            msg(b.erd(&["s".into()]).await),
            msg(b.table_def("s", "t").await),
            msg(b.table_shape("s", "t").await),
            msg(b.schema_model("s").await),
            msg(b.apply_migration(&["select 1"], AppError::Msg).await),
            msg(b.import_tx().await),
        ];
        let want = [
            "Listing databases",
            "Listing schemas",
            "Listing objects",
            "Table details",
            "Table details",
            "DDL",
            "Autocomplete",
            "Go to Database Object",
            "Full-text search",
            "Value suggestions",
            "The diagram",
            "Modify Table",
            "Test data",
            "Schema compare",
            "Running migrations",
            "Import",
        ]
        .map(|op| format!("{op} isn't available for the bare driver"));
        assert_eq!(got, want);
        assert_eq!(b.estimate_rows("select 1").await, None);
        assert!(b.table_columns("t").await.unwrap().is_empty(), "no columns: no undo is kept");
    }
}
