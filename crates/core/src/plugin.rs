//! Driver plugin host: launches the qurry-driver-<name> executable and exchanges one JSON line at a time over stdio (protocol: crates/driver).
//! Executable location: QURRY_DRIVERS_DIR → config folder/drivers → next to this executable (dev builds)
use crate::error::{AppError, AppResult};
use crate::store::{ConnectionConfig, DbKind};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

/// The driver is not installed (if the message starts with this, the UI asks whether to download it)
pub const MISSING: &str = "Driver not installed:";

pub fn exe_name(driver: &str) -> String {
    format!("qurry-driver-{driver}{}", std::env::consts::EXE_SUFFIX)
}

/// Folder for downloaded drivers
pub fn drivers_dir() -> PathBuf {
    crate::config_dir().join("drivers")
}

/// Driver executable (None if missing)
pub fn driver_path(driver: &str) -> Option<PathBuf> {
    let name = exe_name(driver);
    let mut dirs: Vec<PathBuf> = vec![];
    if let Some(d) = std::env::var_os("QURRY_DRIVERS_DIR") {
        dirs.push(d.into());
    }
    dirs.push(drivers_dir());
    // Dev builds: it sits in target/debug too. App bundle: next to the executable
    if let Some(d) = std::env::current_exe().ok().and_then(|e| e.parent().map(|p| p.to_path_buf())) {
        dirs.push(d.clone());
        if let Some(up) = d.parent() {
            dirs.push(up.to_path_buf());
        }
    }
    dirs.into_iter().map(|d| d.join(&name)).find(|p| p.is_file())
}

struct Io {
    stdin: tokio::process::ChildStdin,
    stdout: tokio::io::Lines<BufReader<tokio::process::ChildStdout>>,
}

/// One launched driver (one connection)
pub struct PluginConn {
    pub driver: String,
    io: tokio::sync::Mutex<Io>,
    child: std::sync::Mutex<tokio::process::Child>,
    next: AtomicU64,
}

impl std::fmt::Debug for PluginConn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PluginConn({})", self.driver)
    }
}

impl Drop for PluginConn {
    fn drop(&mut self) {
        if let Ok(mut c) = self.child.lock() {
            let _ = c.start_kill();
        }
    }
}

impl PluginConn {
    /// Launch the driver and negotiate the protocol
    pub async fn start(driver: &str) -> AppResult<std::sync::Arc<Self>> {
        let path = driver_path(driver).ok_or_else(|| AppError::Msg(format!("{MISSING} {driver}")))?;
        let mut child = tokio::process::Command::new(&path)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            // So the driver's stderr does not corrupt the TUI screen
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| AppError::Msg(format!("Could not start the {driver} driver ({}): {e}", path.display())))?;
        let io = Io { stdin: child.stdin.take().unwrap(), stdout: BufReader::new(child.stdout.take().unwrap()).lines() };
        let c = std::sync::Arc::new(PluginConn { driver: driver.into(), io: tokio::sync::Mutex::new(io), child: std::sync::Mutex::new(child), next: AtomicU64::new(1) });
        let hello: Value = c.call("hello", Value::Null).await?;
        let proto = hello.get("protocol").and_then(Value::as_u64).unwrap_or(0);
        if proto != qurry_driver::PROTOCOL as u64 {
            return Err(AppError::Msg(format!("The {driver} driver speaks protocol {proto}, this Qurry needs {} — update the driver", qurry_driver::PROTOCOL)));
        }
        Ok(c)
    }

    /// One request → result (one at a time)
    pub async fn call<T: DeserializeOwned>(&self, method: &str, params: Value) -> AppResult<T> {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let mut io = self.io.lock().await;
        let line = json!({ "id": id, "method": method, "params": params }).to_string() + "\n";
        let gone = |e: std::io::Error| AppError::Msg(format!("The {} driver stopped: {e}", self.driver));
        io.stdin.write_all(line.as_bytes()).await.map_err(gone)?;
        io.stdin.flush().await.map_err(gone)?;
        loop {
            let Some(l) = io.stdout.next_line().await.map_err(gone)? else {
                return Err(AppError::Msg(format!("The {} driver stopped", self.driver)));
            };
            let Ok(v) = serde_json::from_str::<Value>(&l) else { continue };
            if v.get("id").and_then(Value::as_u64) != Some(id) {
                continue;
            }
            if let Some(e) = v.get("error").and_then(Value::as_str) {
                return Err(AppError::Msg(e.to_string()));
            }
            return serde_json::from_value(v.get("result").cloned().unwrap_or(Value::Null)).map_err(|e| AppError::Msg(format!("The {} driver sent an unexpected reply: {e}", self.driver)));
        }
    }

    pub async fn query(&self, sql: &str, limit: usize) -> AppResult<crate::db::QueryResult> {
        self.query_opts(sql, limit, false).await
    }

    /// read_only = a read that passed guard (MCP). Makes the driver read-only on the server side too (ClickHouse readonly)
    pub async fn query_opts(&self, sql: &str, limit: usize, read_only: bool) -> AppResult<crate::db::QueryResult> {
        let start = std::time::Instant::now();
        let o: qurry_driver::QueryOut = self.call("query", json!({ "sql": sql, "limit": limit, "readOnly": read_only })).await?;
        Ok(crate::db::QueryResult {
            columns: o.columns.into_iter().map(|c| crate::db::ColumnInfo { name: c.name, type_name: c.type_name }).collect(),
            rows: o.rows,
            rows_affected: o.rows_affected,
            truncated: o.truncated,
            elapsed_ms: start.elapsed().as_millis() as u64,
            has_result_set: o.has_result_set,
            in_transaction: false,
        })
    }
}

/// Release name suffix for this platform (driver file: qurry-driver-<name>_<version>_<target>.tar.gz)
pub fn target() -> Option<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => Some("aarch64-apple-darwin"),
        ("macos", "x86_64") => Some("x86_64-apple-darwin"),
        ("linux", "x86_64") => Some("x86_64-unknown-linux-gnu"),
        _ => None,
    }
}

/// Driver file name in the release
pub fn asset_name(driver: &str, version: &str, target: &str) -> String {
    format!("qurry-driver-{driver}_{version}_{target}.tar.gz")
}

/// Download and install a driver: fetch over HTTPS from this version's release, verify the SHA256SUMS signature (same key as updates) and hash, then into the drivers folder. Returns the installed path
pub async fn install(driver: &str) -> AppResult<PathBuf> {
    let driver = driver.to_string();
    tokio::task::spawn_blocking(move || install_blocking(&driver, env!("CARGO_PKG_VERSION"))).await.map_err(|e| AppError::Msg(e.to_string()))?
}

fn install_blocking(driver: &str, version: &str) -> AppResult<PathBuf> {
    if !driver.chars().all(|c| c.is_ascii_lowercase()) {
        return Err(AppError::Msg(format!("bad driver name: {driver}")));
    }
    let target = target().ok_or_else(|| AppError::Msg("Drivers are built for macOS (Apple Silicon, Intel) and x86_64 Linux".into()))?;
    let base = format!("https://github.com/{}/releases/download/v{version}/", crate::update::REPO);
    let tmp = std::env::temp_dir().join(format!("qurry-driver-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&tmp)?;
    let get = |name: &str| -> AppResult<PathBuf> {
        let to = tmp.join(name);
        let out = std::process::Command::new("curl")
            .args(["-fsSL", "--proto", "=https", "--proto-redir", "=https", "--tlsv1.2", "--max-time", "600", "-o"])
            .arg(&to)
            .arg(format!("{base}{name}"))
            .output()
            .map_err(|e| AppError::Msg(format!("curl: {e}")))?;
        if !out.status.success() {
            return Err(AppError::Msg(format!("Could not download {name}: {}", String::from_utf8_lossy(&out.stderr).trim())));
        }
        Ok(to)
    };
    let r = (|| {
        let asset = asset_name(driver, version, target);
        let sums = std::fs::read(get("SHA256SUMS")?)?;
        let sig = std::fs::read_to_string(get("SHA256SUMS.sig")?)?;
        crate::update::verify_sums(&sums, &sig).map_err(|e| AppError::Msg(format!("SHA256SUMS {e} — the driver was not installed")))?;
        let tgz = get(&asset)?;
        let want = crate::update::expected_sha(&String::from_utf8_lossy(&sums), &asset).ok_or_else(|| AppError::Msg(format!("{asset} is not in this release")))?;
        if crate::update::sha256_of(&tgz)? != want {
            return Err(AppError::Msg(format!("{asset} doesn't match its checksum — not installed")));
        }
        let st = std::process::Command::new("tar").arg("-xzf").arg(&tgz).arg("-C").arg(&tmp).status()?;
        if !st.success() {
            return Err(AppError::Msg(format!("Could not unpack {asset}")));
        }
        let exe = tmp.join(exe_name(driver));
        if !exe.is_file() {
            return Err(AppError::Msg(format!("{asset} has no {}", exe_name(driver))));
        }
        let dir = drivers_dir();
        std::fs::create_dir_all(&dir)?;
        let dest = dir.join(exe_name(driver));
        // Swap in: move under a temporary name, then rename (so a driver in use is not broken)
        let part = dir.join(format!("{}.part", exe_name(driver)));
        std::fs::copy(&exe, &part)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&part, std::fs::Permissions::from_mode(0o755))?;
        }
        std::fs::rename(&part, &dest)?;
        Ok(dest)
    })();
    let _ = std::fs::remove_dir_all(&tmp);
    r
}

/// Connect via a driver
pub async fn connect(cfg: &ConnectionConfig, password: Option<&str>, port: Option<u16>) -> AppResult<std::sync::Arc<PluginConn>> {
    let driver = cfg.kind.driver().ok_or_else(|| AppError::Msg(format!("{:?} has no driver plugin", cfg.kind)))?;
    let c = PluginConn::start(driver).await?;
    let params = qurry_driver::ConnectParams {
        host: if port.is_some() { "127.0.0.1".into() } else { cfg.host.clone() },
        port: port.unwrap_or(cfg.port),
        user: cfg.user.clone(),
        password: password.map(String::from),
        database: cfg.database.clone(),
        options: json!({ "readOnly": cfg.read_only, "sandbox": crate::SANDBOX.load(std::sync::atomic::Ordering::Relaxed), "ssl": cfg.active_ssl().map(|s| s.mode().to_string()), "oracleClient": cfg.options.oracle_client }),
    };
    let _: Value = c.call("connect", serde_json::to_value(params)?).await?;
    Ok(c)
}

/// Whether this DB kind has no driver
pub fn is_plugin(kind: DbKind) -> bool {
    kind.driver().is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Common driver test: create table → insert → select (limit) → list/details (PK, FK) → DDL
    pub(crate) async fn conformance(cfg: ConnectionConfig, password: Option<&str>, schema: &str, setup: &[&str], parent: &str, child: &str, fk: bool) {
        let c = connect(&cfg, password, None).await.unwrap_or_else(|e| panic!("{:?}: {e}", cfg.kind));
        let v: String = c.call("version", Value::Null).await.unwrap();
        assert!(v.chars().any(|ch| ch.is_ascii_digit()), "version: {v}");
        for q in setup {
            c.query(q, 10).await.unwrap_or_else(|e| panic!("{q}: {e}"));
        }
        let r = c.query(&format!("SELECT * FROM {child} ORDER BY id"), 1).await.unwrap();
        assert_eq!((r.rows.len(), r.truncated), (1, true), "{:?}", r.rows);
        assert!(r.columns.iter().any(|c| c.name.eq_ignore_ascii_case("parent_id")));
        let objs: crate::meta::SchemaObjects = c.call("objects", json!({ "schema": schema })).await.unwrap();
        let names: Vec<String> = objs.relations.iter().map(|r| r.name.to_lowercase()).collect();
        for t in [parent, child] {
            let bare = t.rsplit('.').next().unwrap().to_lowercase();
            assert!(names.contains(&bare), "{bare} not in {names:?}");
        }
        let bare_child = child.rsplit('.').next().unwrap();
        let d: crate::meta::TableDetail = c.call("tableDetail", json!({ "schema": schema, "table": objs.relations.iter().find(|r| r.name.eq_ignore_ascii_case(bare_child)).unwrap().name })).await.unwrap();
        assert!(d.columns.iter().any(|c| c.is_pk && c.name.eq_ignore_ascii_case("id")), "{:?}", d.columns.iter().map(|c| (&c.name, c.is_pk)).collect::<Vec<_>>());
        assert!(!fk || d.constraints.iter().any(|c| c.kind == "f"), "no foreign key: {:?}", d.constraints.iter().map(|c| (&c.kind, &c.definition)).collect::<Vec<_>>());
        let ddl: String = c.call("ddl", json!({ "schema": schema, "name": objs.relations.iter().find(|r| r.name.eq_ignore_ascii_case(bare_child)).unwrap().name, "kind": "table" })).await.unwrap();
        assert!(ddl.to_uppercase().contains("CREATE"), "{ddl}");
        assert!(c.query("SELECT * FROM no_such_table_qurry", 1).await.is_err());
    }

    fn file_cfg(kind: &str, path: &std::path::Path) -> ConnectionConfig {
        serde_json::from_value(json!({ "id": "f", "name": "f", "kind": kind, "host": "", "port": 0, "user": "", "database": path.to_string_lossy() })).unwrap()
    }

    #[tokio::test]
    async fn sqlite_driver_conformance() {
        if driver_path("sqlite").is_none() {
            eprintln!("skip: build qurry-driver-sqlite first");
            return;
        }
        let file = std::env::temp_dir().join(format!("qurry-{}.db", uuid::Uuid::new_v4()));
        conformance(file_cfg("sqlite", &file), None, "main", &[
            "create table parent (id integer primary key, name text not null)",
            "create table child (id integer primary key, parent_id integer references parent(id), note text)",
            "insert into parent values (1, 'a'), (2, 'b')",
            "insert into child values (1, 1, 'x'), (2, 2, 'y')",
        ], "parent", "child", true).await;
        std::fs::remove_file(file).ok();
        // A missing driver gives the fixed message (the UI asks whether to download it)
        let e = PluginConn::start("nope").await.unwrap_err().to_string();
        assert!(e.starts_with(MISSING), "{e}");
    }

    /// Server DBs: only when QURRY_TEST_<KIND>=host:port:user:password:database is set
    fn server_cfg(kind: &str, env: &str) -> Option<(ConnectionConfig, String)> {
        let e = std::env::var(env).ok()?;
        let p: Vec<&str> = e.splitn(5, ':').collect();
        let cfg = serde_json::from_value(json!({ "id": "s", "name": "s", "kind": kind, "host": p[0], "port": p[1].parse::<u16>().ok()?, "user": p[2], "database": p[4] })).ok()?;
        Some((cfg, p[3].to_string()))
    }

    #[tokio::test]
    #[ignore]
    async fn clickhouse_driver_conformance() {
        let Some((cfg, pw)) = server_cfg("clickhouse", "QURRY_TEST_CH") else { return };
        if driver_path("clickhouse").is_none() {
            return;
        }
        let db = cfg.database.clone();
        conformance(cfg, Some(&pw), &db, &[
            "DROP TABLE IF EXISTS q_child",
            "DROP TABLE IF EXISTS q_parent",
            "CREATE TABLE q_parent (id UInt32, name String) ENGINE = MergeTree ORDER BY id",
            "CREATE TABLE q_child (id UInt32, parent_id UInt32, note Nullable(String)) ENGINE = MergeTree ORDER BY id",
            "INSERT INTO q_parent VALUES (1, 'a'), (2, 'b')",
            "INSERT INTO q_child VALUES (1, 1, 'x'), (2, 2, NULL)",
        ], "q_parent", "q_child", false).await;
    }

    #[tokio::test]
    #[ignore]
    async fn mssql_driver_conformance() {
        let Some((cfg, pw)) = server_cfg("mssql", "QURRY_TEST_MSSQL") else { return };
        if driver_path("mssql").is_none() {
            return;
        }
        conformance(cfg, Some(&pw), "dbo", &[
            "IF OBJECT_ID('dbo.q_child') IS NOT NULL DROP TABLE dbo.q_child",
            "IF OBJECT_ID('dbo.q_parent') IS NOT NULL DROP TABLE dbo.q_parent",
            "CREATE TABLE dbo.q_parent (id int PRIMARY KEY, name nvarchar(50) NOT NULL)",
            "CREATE TABLE dbo.q_child (id int PRIMARY KEY, parent_id int REFERENCES dbo.q_parent(id), note nvarchar(max))",
            "INSERT INTO dbo.q_parent VALUES (1, N'a'), (2, N'b')",
            "INSERT INTO dbo.q_child VALUES (1, 1, N'x'), (2, 2, NULL)",
        ], "q_parent", "q_child", true).await;
    }

    /// MongoDB: shell syntax (insertMany · find · sort/limit · aggregate · countDocuments), explorer SELECT * FROM, field inference
    #[tokio::test]
    #[ignore]
    async fn mongodb_driver_shell_queries() {
        let Some((cfg, pw)) = server_cfg("mongodb", "QURRY_TEST_MONGO") else { return };
        if driver_path("mongodb").is_none() {
            return;
        }
        let c = connect(&cfg, Some(&pw), None).await.unwrap();
        c.query("db.q_users.drop()", 1).await.ok();
        assert_eq!(c.query("db.q_users.insertMany([{name: 'Ada', age: 36, city: 'London'}, {name: 'Linus', age: 54, city: 'Portland', tags: ['kernel']}, {name: 'Grace', age: 85, city: 'Arlington'}])", 1).await.unwrap().rows_affected, 3);
        let r = c.query("db.q_users.find({age: {$gt: 40}}, {_id: 0}).sort({age: -1})", 1).await.unwrap();
        assert_eq!((r.rows.len(), r.truncated), (1, true));
        assert_eq!(r.rows[0][r.columns.iter().position(|c| c.name == "name").unwrap()], json!("Grace"));
        assert_eq!(c.query("db.q_users.countDocuments({city: 'London'})", 10).await.unwrap().rows[0][0], json!(1));
        let a = c.query("db.q_users.aggregate([{$group: {_id: null, total: {$sum: '$age'}}}])", 10).await.unwrap();
        assert_eq!(a.rows[0][a.columns.iter().position(|c| c.name == "total").unwrap()], json!(175));
        assert_eq!(c.query("db.q_users.updateMany({age: {$gt: 50}}, {$set: {senior: true}})", 1).await.unwrap().rows_affected, 2);
        assert_eq!(c.query("db.q_users.deleteOne({name: 'Ada'})", 1).await.unwrap().rows_affected, 1);
        // Explorer: collection list, SELECT * FROM db.coll, field inference
        let objs: crate::meta::SchemaObjects = c.call("objects", json!({ "schema": cfg.database })).await.unwrap();
        assert!(objs.relations.iter().any(|r| r.name == "q_users"));
        let all = c.query(&format!("SELECT * FROM {}.q_users", cfg.database), 500).await.unwrap();
        assert_eq!(all.rows.len(), 2);
        let d: crate::meta::TableDetail = c.call("tableDetail", json!({ "table": "q_users" })).await.unwrap();
        assert!(d.columns.iter().any(|c| c.name == "_id" && c.is_pk) && d.columns.iter().any(|c| c.name == "tags"));
        assert!(c.query("select count(*) from q_users where age > 1", 1).await.unwrap_err().to_string().contains("isn't SQL"));
        c.query("db.q_users.drop()", 1).await.ok();
    }

    /// Oracle: requires Instant Client (ORACLE_CLIENT_LIB_DIR). Schema = user (uppercase)
    #[tokio::test]
    #[ignore]
    async fn oracle_driver_conformance() {
        let Some((cfg, pw)) = server_cfg("oracle", "QURRY_TEST_ORACLE") else { return };
        if driver_path("oracle").is_none() {
            return;
        }
        let user = cfg.user.to_uppercase();
        conformance(cfg, Some(&pw), &user, &[
            "BEGIN EXECUTE IMMEDIATE 'DROP TABLE q_child'; EXCEPTION WHEN OTHERS THEN NULL; END;",
            "BEGIN EXECUTE IMMEDIATE 'DROP TABLE q_parent'; EXCEPTION WHEN OTHERS THEN NULL; END;",
            "CREATE TABLE q_parent (id NUMBER(10) PRIMARY KEY, name VARCHAR2(50) NOT NULL)",
            "CREATE TABLE q_child (id NUMBER(10) PRIMARY KEY, parent_id NUMBER(10) REFERENCES q_parent(id), note VARCHAR2(100))",
            "INSERT INTO q_parent VALUES (1, 'a')",
            "INSERT INTO q_parent VALUES (2, 'b')",
            "INSERT INTO q_child VALUES (1, 1, 'x')",
            "INSERT INTO q_child VALUES (2, 2, NULL)",
        ], "q_parent", "q_child", true).await;
    }

    /// Qdrant: REST console (create collection → upsert → search), explorer (list · scroll · vector/payload columns)
    #[tokio::test]
    #[ignore]
    async fn qdrant_driver_rest_console() {
        let Some((cfg, pw)) = server_cfg("qdrant", "QURRY_TEST_QDRANT") else { return };
        if driver_path("qdrant").is_none() {
            return;
        }
        let c = connect(&cfg, Some(&pw), None).await.unwrap();
        assert!(c.call::<String>("version", Value::Null).await.unwrap().starts_with("Qdrant 1."));
        c.query("DELETE /collections/q_items", 1).await.ok();
        c.query("PUT /collections/q_items\n{vectors: {size: 3, distance: 'Cosine'}}", 1).await.unwrap();
        c.query(r#"PUT /collections/q_items/points?wait=true
{points: [
  {id: 1, vector: [1, 0, 0], payload: {name: 'apple', shop: {city: 'Seoul'}}},
  {id: 2, vector: [0, 1, 0], payload: {name: 'pear', price: 3}},
  {id: 3, vector: [0.9, 0.1, 0], payload: {name: 'apricot'}},
]}"#, 1).await.unwrap();
        let r = c.query("POST /collections/q_items/points/search\n{vector: [1, 0, 0], limit: 2, with_payload: true}", 500).await.unwrap();
        let name = r.columns.iter().position(|c| c.name == "payload.name").unwrap();
        assert_eq!(r.rows.iter().map(|x| x[name].clone()).collect::<Vec<_>>(), vec![json!("apple"), json!("apricot")]);
        assert!(r.columns.iter().any(|c| c.name == "score"));
        let objs: crate::meta::SchemaObjects = c.call("objects", json!({ "schema": "qdrant" })).await.unwrap();
        assert!(objs.relations.iter().any(|r| r.name == "q_items"));
        let all = c.query("SELECT * FROM qdrant.q_items", 2).await.unwrap();
        assert_eq!((all.rows.len(), all.truncated), (2, true));
        assert!(all.columns.iter().any(|c| c.name == "payload.shop"), "{:?}", all.columns.iter().map(|c| &c.name).collect::<Vec<_>>());
        let d: crate::meta::TableDetail = c.call("tableDetail", json!({ "table": "q_items" })).await.unwrap();
        assert!(d.columns.iter().any(|c| c.name == "id" && c.is_pk));
        assert!(d.columns.iter().any(|c| c.name == "vector" && c.data_type == "vector(3, Cosine)"));
        assert!(d.columns.iter().any(|c| c.name == "payload.price"));
        assert!(c.call::<String>("ddl", json!({ "name": "q_items" })).await.unwrap().starts_with("PUT /collections/q_items"));
        assert!(c.query("GET /collections/no_such_qurry", 1).await.unwrap_err().to_string().contains("doesn't exist"));
        c.query("DELETE /collections/q_items", 1).await.unwrap();
    }

    /// Chroma: collection paths by name, column arrays from get/query → rows
    #[tokio::test]
    #[ignore]
    async fn chroma_driver_rest_console() {
        let Some((cfg, pw)) = server_cfg("chroma", "QURRY_TEST_CHROMA") else { return };
        if driver_path("chroma").is_none() {
            return;
        }
        let c = connect(&cfg, Some(&pw), None).await.unwrap();
        assert!(c.call::<String>("version", Value::Null).await.unwrap().starts_with("Chroma 1."));
        c.query("DELETE /collections/q_docs", 1).await.ok();
        c.query("POST /collections\n{name: 'q_docs', configuration: {hnsw: {space: 'cosine'}}}", 1).await.unwrap();
        c.query(r#"POST /collections/q_docs/add
{ids: ['a', 'b', 'c'], embeddings: [[1, 0], [0, 1], [0.9, 0.1]], documents: ['apple', 'pear', 'apricot'], metadatas: [{lang: 'en'}, {lang: 'ko', n: 2}, {lang: 'en'}]}"#, 1).await.unwrap();
        let r = c.query("POST /collections/q_docs/query\n{query_embeddings: [[1, 0]], n_results: 2, include: ['documents', 'distances', 'metadatas']}", 500).await.unwrap();
        let doc = r.columns.iter().position(|c| c.name == "document").unwrap();
        assert_eq!(r.rows.iter().map(|x| x[doc].clone()).collect::<Vec<_>>(), vec![json!("apple"), json!("apricot")]);
        assert!(r.columns.iter().any(|c| c.name == "distance") && r.columns.iter().any(|c| c.name == "metadata.lang"));
        let dbs: Vec<String> = c.call("databases", Value::Null).await.unwrap();
        assert!(dbs.contains(&"default_database".to_string()));
        let objs: crate::meta::SchemaObjects = c.call("objects", json!({ "schema": "default_database" })).await.unwrap();
        assert!(objs.relations.iter().any(|r| r.name == "q_docs"));
        let all = c.query("SELECT * FROM default_database.q_docs", 2).await.unwrap();
        assert_eq!((all.rows.len(), all.truncated), (2, true));
        let d: crate::meta::TableDetail = c.call("tableDetail", json!({ "table": "q_docs" })).await.unwrap();
        assert!(d.columns.iter().any(|c| c.name == "id" && c.is_pk));
        assert!(d.columns.iter().any(|c| c.name == "embedding" && c.data_type == "vector(2, cosine)"), "{:?}", d.columns.iter().map(|c| &c.data_type).collect::<Vec<_>>());
        assert!(d.columns.iter().any(|c| c.name == "metadata.n"));
        assert!(c.query("GET /collections/no_such_qurry", 1).await.is_err());
        c.query("DELETE /collections/q_docs", 1).await.unwrap();
    }

    /// Milvus: fill dbName in POST paths, insert count, search, explorer
    #[tokio::test]
    #[ignore]
    async fn milvus_driver_rest_console() {
        let Some((cfg, pw)) = server_cfg("milvus", "QURRY_TEST_MILVUS") else { return };
        if driver_path("milvus").is_none() {
            return;
        }
        let c = connect(&cfg, Some(&pw), None).await.unwrap();
        c.query("POST /collections/drop\n{collectionName: 'q_items'}", 1).await.ok();
        c.query("POST /collections/create\n{collectionName: 'q_items', dimension: 3, metricType: 'COSINE'}", 1).await.unwrap();
        let ins = c.query(r#"POST /entities/insert
{collectionName: 'q_items', data: [{id: 1, vector: [1, 0, 0], name: 'apple'}, {id: 2, vector: [0, 1, 0], name: 'pear'}, {id: 3, vector: [0.9, 0.1, 0], name: 'apricot'}]}"#, 1).await.unwrap();
        assert_eq!(ins.rows_affected, 3);
        let r = c.query("POST /entities/search\n{collectionName: 'q_items', data: [[1, 0, 0]], limit: 2, outputFields: ['name']}", 500).await.unwrap();
        let name = r.columns.iter().position(|c| c.name == "name").unwrap();
        assert_eq!(r.rows.iter().map(|x| x[name].clone()).collect::<Vec<_>>(), vec![json!("apple"), json!("apricot")]);
        let objs: crate::meta::SchemaObjects = c.call("objects", json!({ "schema": "default" })).await.unwrap();
        assert!(objs.relations.iter().any(|r| r.name == "q_items"));
        let all = c.query("SELECT * FROM default.q_items", 2).await.unwrap();
        assert_eq!((all.rows.len(), all.truncated), (2, true));
        let d: crate::meta::TableDetail = c.call("tableDetail", json!({ "table": "q_items" })).await.unwrap();
        assert!(d.columns.iter().any(|c| c.name == "id" && c.is_pk));
        assert!(d.columns.iter().any(|c| c.name == "vector" && c.data_type == "FloatVector(3, COSINE)"), "{:?}", d.columns.iter().map(|c| &c.data_type).collect::<Vec<_>>());
        assert!(c.call::<String>("ddl", json!({ "name": "q_items" })).await.unwrap().contains("\"dim\": \"3\""));
        assert!(c.query("POST /entities/query\n{collectionName: 'no_such_qurry', filter: ''}", 1).await.unwrap_err().to_string().contains("not found"));
        c.query("POST /collections/drop\n{collectionName: 'q_items'}", 1).await.unwrap();
    }

    #[test]
    fn driver_asset_names() {
        assert_eq!(asset_name("sqlite", "0.2.0", "aarch64-apple-darwin"), "qurry-driver-sqlite_0.2.0_aarch64-apple-darwin.tar.gz");
        assert!(install_blocking("../evil", "0.2.0").unwrap_err().to_string().contains("bad driver name"));
    }

    /// Releases of nonexistent versions cannot be downloaded (meaningful only with network access)
    #[test]
    #[ignore]
    fn install_fails_cleanly_for_a_missing_release() {
        let e = install_blocking("sqlite", "0.0.0-nope").unwrap_err().to_string();
        assert!(e.contains("Could not download"), "{e}");
    }

    #[tokio::test]
    async fn duckdb_driver_conformance() {
        if driver_path("duckdb").is_none() {
            eprintln!("skip: build qurry-driver-duckdb first");
            return;
        }
        let file = std::env::temp_dir().join(format!("qurry-{}.duckdb", uuid::Uuid::new_v4()));
        conformance(file_cfg("duckdb", &file), None, "main", &[
            "create table parent (id integer primary key, name varchar not null)",
            "create table child (id integer primary key, parent_id integer references parent(id), note varchar)",
            "insert into parent values (1, 'a'), (2, 'b')",
            "insert into child values (1, 1, 'x'), (2, 2, 'y')",
        ], "parent", "child", true).await;
        std::fs::remove_file(file).ok();
    }
}
