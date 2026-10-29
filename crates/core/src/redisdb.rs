//! Redis driver. In the console one line = one command; results come back as the same table (QueryResult) as SQL results.
//! Tree: data source → db0..dbN → keys (icon per type). Opening a key shows its value as a table using the command for its type.
use crate::db::{int_value, query_result, ColumnInfo, QueryResult};
use crate::error::{AppError, AppResult};
use crate::meta::{IndexEntry, RelationInfo, SchemaInfo, SchemaObjects};
use crate::store::ConnectionConfig;
use redis::aio::ConnectionManager;
use redis::Value as RV;
use serde_json::Value;
use std::time::Instant;

/// Fake command the frontend sends when opening a single key
pub const VIEW_CMD: &str = "DBTOOL.VIEW";
/// Max number of keys fetched at once for the tree·search
const MAX_KEYS: usize = 5000;

fn pct(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// "db3" / "3" / "" → 3 / 0
pub fn db_index(name: &str) -> i64 {
    name.trim().trim_start_matches("db").parse().unwrap_or(0)
}

/// Standalone·Sentinel use one ConnectionManager; Cluster uses the cluster connection + a connection per master (for SCAN)
#[derive(Clone)]
pub enum RedisConn {
    Single(ConnectionManager),
    Cluster { conn: redis::cluster_async::ClusterConnection, masters: Vec<ConnectionManager> },
}

impl redis::aio::ConnectionLike for RedisConn {
    fn req_packed_command<'a>(&'a mut self, cmd: &'a redis::Cmd) -> redis::RedisFuture<'a, RV> {
        match self {
            RedisConn::Single(m) => m.req_packed_command(cmd),
            RedisConn::Cluster { conn, .. } => conn.req_packed_command(cmd),
        }
    }
    fn req_packed_commands<'a>(&'a mut self, cmd: &'a redis::Pipeline, offset: usize, count: usize) -> redis::RedisFuture<'a, Vec<RV>> {
        match self {
            RedisConn::Single(m) => m.req_packed_commands(cmd, offset, count),
            RedisConn::Cluster { conn, .. } => conn.req_packed_commands(cmd, offset, count),
        }
    }
    fn get_db(&self) -> i64 {
        match self {
            RedisConn::Single(m) => m.get_db(),
            RedisConn::Cluster { .. } => 0,
        }
    }
}

impl RedisConn {
    pub fn is_cluster(&self) -> bool {
        matches!(self, RedisConn::Cluster { .. })
    }
    /// Nodes to run SCAN on (all masters for a cluster)
    fn scan_nodes(&self) -> Vec<ConnectionManager> {
        match self {
            RedisConn::Single(m) => vec![m.clone()],
            RedisConn::Cluster { masters, .. } => masters.clone(),
        }
    }
}

fn expand_home(p: &str) -> String {
    match p.strip_prefix("~/") {
        Some(rest) => format!("{}/{rest}", std::env::var("HOME").unwrap_or_default()),
        None => p.to_string(),
    }
}

fn read_pem(path: &str, what: &str) -> AppResult<Option<Vec<u8>>> {
    let p = path.trim();
    if p.is_empty() {
        return Ok(None);
    }
    std::fs::read(expand_home(p)).map(Some).map_err(|e| AppError::Msg(format!("{what} {p}: {e}")))
}

/// List of "host:port" (separated by commas·spaces·newlines)
pub fn parse_nodes(s: &str, default_port: u16) -> Vec<(String, u16)> {
    s.split(|c: char| c == ',' || c.is_whitespace())
        .map(str::trim)
        .filter(|x| !x.is_empty())
        .map(|x| match x.rsplit_once(':') {
            Some((h, p)) if p.parse::<u16>().is_ok() => (h.trim_matches(|c| c == '[' || c == ']').to_string(), p.parse().unwrap()),
            _ => (x.to_string(), default_port),
        })
        .collect()
}

struct Endpoint {
    scheme: &'static str,
    auth: String,
    insecure: bool,
    certs: Option<redis::TlsCertificates>,
}

impl Endpoint {
    fn url(&self, host: &str, port: u16, db: i64) -> String {
        let h = if host.contains(':') { format!("[{host}]") } else { host.to_string() };
        format!("{}://{}{h}:{port}/{db}{}", self.scheme, self.auth, if self.insecure { "#insecure" } else { "" })
    }
    fn client(&self, host: &str, port: u16, db: i64) -> AppResult<redis::Client> {
        let url = self.url(host, port, db);
        match &self.certs {
            Some(c) => redis::Client::build_with_tls(url.as_str(), c.clone()),
            None => redis::Client::open(url.as_str()),
        }
        .map_err(err)
    }
    async fn manager(&self, host: &str, port: u16, db: i64) -> AppResult<ConnectionManager> {
        let cfg = redis::aio::ConnectionManagerConfig::new().set_connection_timeout(Some(std::time::Duration::from_secs(10)));
        let mut m = ConnectionManager::new_with_config(self.client(host, port, db)?, cfg).await.map_err(|e| AppError::Msg(format!("Redis {host}:{port}: {e}")))?;
        let _: String = redis::cmd("PING").query_async(&mut m).await.map_err(err)?;
        Ok(m)
    }
}

fn endpoint(cfg: &ConnectionConfig, user: &str, password: Option<&str>) -> AppResult<Endpoint> {
    let auth = match (user.trim(), password.filter(|p| !p.is_empty())) {
        ("", None) => String::new(),
        ("", Some(p)) => format!(":{}@", pct(p)),
        (u, None) => format!("{}@", pct(u)),
        (u, Some(p)) => format!("{}:{}@", pct(u), pct(p)),
    };
    let Some(ssl) = cfg.active_ssl() else {
        return Ok(Endpoint { scheme: "redis", auth, insecure: false, certs: None });
    };
    let root = read_pem(&ssl.ca_file, "CA file")?;
    let client_tls = match (read_pem(&ssl.cert_file, "Client certificate")?, read_pem(&ssl.key_file, "Client key")?) {
        (Some(client_cert), Some(client_key)) => Some(redis::ClientTlsConfig { client_cert, client_key }),
        (None, None) => None,
        _ => return Err(AppError::Msg("Both client certificate and client key are required".into())),
    };
    // require = encryption only (no certificate check). verify-ca / verify-full = verified
    let insecure = match ssl.mode() {
        "require" => true,
        "verify-ca" | "verify-full" => false,
        m => return Err(AppError::Msg(format!("Unknown SSL mode '{m}' (use require, verify-ca or verify-full)"))),
    };
    let certs = if root.is_some() || client_tls.is_some() { Some(redis::TlsCertificates { client_tls, root_cert: root }) } else { None };
    Ok(Endpoint { scheme: "rediss", auth, insecure, certs })
}

/// Ask Sentinel for the current master address
async fn sentinel_master(cfg: &ConnectionConfig, ep: &Endpoint, host: &str, port: u16) -> AppResult<(String, u16)> {
    let r = cfg.redis.clone().unwrap_or_default();
    let master = r.sentinel_master.trim();
    if master.is_empty() {
        return Err(AppError::Msg("Sentinel master name is required".into()));
    }
    let mut nodes = parse_nodes(&r.nodes, 26379);
    if nodes.is_empty() {
        nodes.push((host.to_string(), port));
    }
    // Sentinel usually has no password: try without auth first, then with the same password
    let no_auth = Endpoint { scheme: ep.scheme, auth: String::new(), insecure: ep.insecure, certs: ep.certs.clone() };
    let mut errors = vec![];
    for (h, p) in nodes {
        for e in [&no_auth, ep] {
            let attempt = async {
                let mut c = e.client(&h, p, 0)?.get_multiplexed_async_connection().await.map_err(err)?;
                let addr: Option<(String, u16)> = redis::cmd("SENTINEL").arg("get-master-addr-by-name").arg(master).query_async(&mut c).await.map_err(err)?;
                addr.ok_or_else(|| AppError::Msg(format!("Sentinel {h}:{p} does not know master '{master}'")))
            };
            match tokio::time::timeout(std::time::Duration::from_secs(8), attempt).await {
                Ok(Ok(a)) => return Ok(a),
                Ok(Err(x)) => errors.push(format!("{h}:{p}: {x}")),
                Err(_) => errors.push(format!("{h}:{p}: timeout")),
            }
        }
    }
    Err(AppError::Msg(format!("No sentinel answered: {}", errors.join("; "))))
}

pub async fn connect(cfg: &ConnectionConfig, password: Option<&str>, host: &str, port: u16) -> AppResult<RedisConn> {
    let ep = endpoint(cfg, &cfg.user, password)?;
    let db = db_index(&cfg.database);
    let mode = cfg.redis_mode();
    if mode != "standalone" && cfg.active_ssh().is_some() {
        return Err(AppError::Msg(format!("SSH tunnel is not supported with Redis {mode} (nodes announce their own addresses)")));
    }
    let conn = match mode {
        "cluster" => {
            let mut seeds = vec![(host.to_string(), port)];
            seeds.extend(parse_nodes(&cfg.redis.as_ref().map(|r| r.nodes.clone()).unwrap_or_default(), 6379));
            let mut b = redis::cluster::ClusterClientBuilder::new(seeds.iter().map(|(h, p)| ep.url(h, *p, 0)).collect::<Vec<_>>())
                .connection_timeout(std::time::Duration::from_secs(10))
                .response_timeout(std::time::Duration::from_secs(30));
            if let Some(p) = password.filter(|p| !p.is_empty()) {
                b = b.password(p);
            }
            if !cfg.user.trim().is_empty() {
                b = b.username(cfg.user.trim());
            }
            if ep.scheme == "rediss" {
                b = b.tls(if ep.insecure { redis::TlsMode::Insecure } else { redis::TlsMode::Secure });
                if let Some(c) = &ep.certs {
                    b = b.certs(c.clone());
                }
            }
            let mut conn = b.build().map_err(err)?.get_async_connection().await.map_err(|e| AppError::Msg(format!("Redis cluster: {e}")))?;
            // Master list (SCAN runs per node)
            let nodes: String = redis::cmd("CLUSTER").arg("NODES").query_async(&mut conn).await.map_err(err)?;
            let mut masters = vec![];
            for line in nodes.lines() {
                let f: Vec<&str> = line.split_whitespace().collect();
                if f.len() < 3 || !f[2].contains("master") || f[2].contains("fail") || f[2].contains("noaddr") {
                    continue;
                }
                let addr = f[1].split('@').next().unwrap_or("");
                let addr = addr.split(',').next().unwrap_or(addr);
                if let Some((h, p)) = addr.rsplit_once(':') {
                    let h = if h.is_empty() { host } else { h };
                    masters.push(ep.manager(h, p.parse().unwrap_or(6379), 0).await?);
                }
            }
            if masters.is_empty() {
                return Err(AppError::Msg("Redis cluster: no reachable master nodes".into()));
            }
            RedisConn::Cluster { conn, masters }
        }
        "sentinel" => {
            let (h, p) = sentinel_master(cfg, &ep, host, port).await?;
            RedisConn::Single(ep.manager(&h, p, db).await?)
        }
        _ => RedisConn::Single(ep.manager(host, port, db).await?),
    };
    let mut c = conn.clone();
    // Startup script (one line = one command)
    for line in cfg.options.startup_script.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')) {
        let args = parse_command(line)?;
        let name = String::from_utf8_lossy(&args[0]).to_string();
        let mut cmd = redis::cmd(&name);
        for a in &args[1..] {
            cmd.arg(a.as_slice());
        }
        let _: RV = cmd.query_async(&mut c).await.map_err(err)?;
    }
    Ok(conn)
}

fn err(e: redis::RedisError) -> AppError {
    AppError::Msg(format!("Redis: {e}"))
}

pub async fn server_version(m: &RedisConn) -> AppResult<String> {
    let mut m = m.clone();
    let info: String = redis::cmd("INFO").arg("server").query_async(&mut m).await.map_err(err)?;
    let v = info.lines().find_map(|l| l.strip_prefix("redis_version:")).unwrap_or("?").trim().to_string();
    Ok(format!("Redis {v}"))
}

pub async fn ping(m: &RedisConn) {
    let mut m = m.clone();
    let _: Result<String, _> = redis::cmd("PING").query_async(&mut m).await;
}

/// redis-cli style argument splitting: spaces, "double quotes"(\n \t \" \\ \xHH), 'single quotes'
pub fn parse_command(line: &str) -> AppResult<Vec<Vec<u8>>> {
    let mut out: Vec<Vec<u8>> = Vec::new();
    let b = line.as_bytes();
    let mut i = 0;
    while i < b.len() {
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= b.len() {
            break;
        }
        let mut cur = Vec::new();
        let mut quoted = false;
        while i < b.len() && !b[i].is_ascii_whitespace() {
            match b[i] {
                b'"' => {
                    quoted = true;
                    i += 1;
                    loop {
                        if i >= b.len() {
                            return Err(AppError::Msg("Unbalanced quotes".into()));
                        }
                        match b[i] {
                            b'"' => {
                                i += 1;
                                break;
                            }
                            b'\\' if i + 1 < b.len() => {
                                i += 1;
                                match b[i] {
                                    b'n' => cur.push(b'\n'),
                                    b't' => cur.push(b'\t'),
                                    b'r' => cur.push(b'\r'),
                                    b'x' if i + 2 < b.len() => {
                                        let h = std::str::from_utf8(&b[i + 1..i + 3]).ok().and_then(|h| u8::from_str_radix(h, 16).ok());
                                        match h {
                                            Some(v) => {
                                                cur.push(v);
                                                i += 2;
                                            }
                                            None => cur.push(b'x'),
                                        }
                                    }
                                    c => cur.push(c),
                                }
                                i += 1;
                            }
                            c => {
                                cur.push(c);
                                i += 1;
                            }
                        }
                    }
                }
                b'\'' => {
                    quoted = true;
                    i += 1;
                    loop {
                        if i >= b.len() {
                            return Err(AppError::Msg("Unbalanced quotes".into()));
                        }
                        if b[i] == b'\'' {
                            i += 1;
                            break;
                        }
                        cur.push(b[i]);
                        i += 1;
                    }
                }
                c => {
                    cur.push(c);
                    i += 1;
                }
            }
        }
        if !cur.is_empty() || quoted {
            out.push(cur);
        }
    }
    if out.is_empty() {
        return Err(AppError::Msg("Empty command".into()));
    }
    Ok(out)
}

/// Quoting for command-line display (Go to DDL)
pub fn quote_arg(s: &str) -> String {
    if !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || ":_-./@".contains(c)) {
        s.to_string()
    } else {
        let mut o = String::from("\"");
        for c in s.chars() {
            match c {
                '"' => o.push_str("\\\""),
                '\\' => o.push_str("\\\\"),
                '\n' => o.push_str("\\n"),
                '\t' => o.push_str("\\t"),
                '\r' => o.push_str("\\r"),
                c => o.push(c),
            }
        }
        o.push('"');
        o
    }
}

fn bytes_text(b: &[u8]) -> Value {
    match std::str::from_utf8(b) {
        Ok(s) => Value::from(s),
        Err(_) => {
            let hex: String = b.iter().take(64).map(|x| format!("{x:02X}")).collect();
            Value::from(format!("0x{hex}{}", if b.len() > 64 { "…" } else { "" }))
        }
    }
}

/// Redis value → cell
fn cell(v: &RV) -> Value {
    match v {
        RV::Nil => Value::Null,
        RV::Int(i) => int_value(*i),
        RV::BulkString(b) => bytes_text(b),
        RV::SimpleString(s) => Value::from(s.as_str()),
        RV::Okay => Value::from("OK"),
        RV::Double(d) => Value::from(*d),
        RV::Boolean(b) => Value::from(*b),
        RV::Array(xs) | RV::Set(xs) => Value::from(Value::Array(xs.iter().map(cell).collect()).to_string()),
        RV::Map(kv) => {
            let m: serde_json::Map<String, Value> = kv.iter().map(|(k, v)| (text_of(k), cell(v))).collect();
            Value::from(Value::Object(m).to_string())
        }
        other => Value::from(format!("{other:?}")),
    }
}
fn text_of(v: &RV) -> String {
    match cell(v) {
        Value::String(s) => s,
        Value::Null => String::new(),
        o => o.to_string(),
    }
}

fn cols(names: &[&str]) -> Vec<ColumnInfo> {
    names.iter().map(|n| ColumnInfo { name: n.to_string(), type_name: "TEXT".into() }).collect()
}

fn table(names: &[&str], mut rows: Vec<Vec<Value>>, limit: usize, start: Instant) -> QueryResult {
    let truncated = rows.len() > limit;
    rows.truncate(limit);
    query_result(cols(names), rows, 0, truncated, start, true)
}

fn pairs(xs: &[RV]) -> Vec<Vec<Value>> {
    xs.chunks(2).map(|c| vec![cell(&c[0]), c.get(1).map(cell).unwrap_or(Value::Null)]).collect()
}

/// Command result as a table
fn shape(cmd: &str, args: &[Vec<u8>], v: RV, limit: usize, start: Instant) -> QueryResult {
    let has = |w: &str| args.iter().any(|a| a.eq_ignore_ascii_case(w.as_bytes()));
    match (cmd, v) {
        ("INFO", v) => {
            let text = text_of(&v);
            let mut section = String::new();
            let mut rows = Vec::new();
            for l in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
                if let Some(s) = l.strip_prefix("# ") {
                    section = s.to_string();
                } else if let Some((k, val)) = l.split_once(':') {
                    rows.push(vec![Value::from(section.as_str()), Value::from(k), Value::from(val)]);
                }
            }
            table(&["section", "key", "value"], rows, usize::MAX, start)
        }
        ("HGETALL" | "CONFIG", RV::Array(xs)) => table(&["field", "value"], pairs(&xs), limit, start),
        ("HGETALL" | "CONFIG", RV::Map(kv)) => table(&["field", "value"], kv.iter().map(|(k, v)| vec![cell(k), cell(v)]).collect(), limit, start),
        ("ZRANGE" | "ZREVRANGE" | "ZRANGEBYSCORE" | "ZREVRANGEBYSCORE" | "ZPOPMIN" | "ZPOPMAX", RV::Array(xs)) if has("WITHSCORES") || cmd.starts_with("ZPOP") => {
            table(&["member", "score"], pairs(&xs), limit, start)
        }
        ("XRANGE" | "XREVRANGE", RV::Array(xs)) => {
            let rows = xs
                .iter()
                .map(|e| match e {
                    RV::Array(p) if p.len() == 2 => {
                        let fields = match &p[1] {
                            RV::Array(f) => {
                                let m: serde_json::Map<String, Value> = f.chunks(2).map(|c| (text_of(&c[0]), c.get(1).map(cell).unwrap_or(Value::Null))).collect();
                                Value::from(Value::Object(m).to_string())
                            }
                            o => cell(o),
                        };
                        vec![cell(&p[0]), fields]
                    }
                    o => vec![cell(o), Value::Null],
                })
                .collect();
            table(&["id", "fields"], rows, limit, start)
        }
        ("SCAN" | "SSCAN" | "HSCAN" | "ZSCAN", RV::Array(xs)) if xs.len() == 2 => {
            let cursor = text_of(&xs[0]);
            let items = match &xs[1] {
                RV::Array(i) => i.clone(),
                _ => vec![],
            };
            let head = format!("next cursor: {cursor}");
            if cmd == "HSCAN" || cmd == "ZSCAN" {
                let second = if cmd == "HSCAN" { "value" } else { "score" };
                let first = if cmd == "HSCAN" { "field" } else { "member" };
                let mut r = table(&[first, second], pairs(&items), limit, start);
                r.columns[0].name = format!("{first} ({head})");
                r
            } else {
                let mut r = table(&["value"], items.iter().map(|x| vec![cell(x)]).collect(), limit, start);
                r.columns[0].name = format!("value ({head})");
                r
            }
        }
        (_, RV::Array(xs)) | (_, RV::Set(xs)) => table(&["#", "value"], xs.iter().enumerate().map(|(i, x)| vec![Value::from(i as i64 + 1), cell(x)]).collect(), limit, start),
        (_, RV::Map(kv)) => table(&["field", "value"], kv.iter().map(|(k, v)| vec![cell(k), cell(v)]).collect(), limit, start),
        (_, v) => table(&["result"], vec![vec![cell(&v)]], limit, start),
    }
}

/// Whether the command doesn't change values (MCP read-only mode)
pub fn is_read_command(cmd: &str, sub: &str) -> bool {
    const READ: &[&str] = &[
        "GET", "MGET", "GETRANGE", "STRLEN", "EXISTS", "TYPE", "TTL", "PTTL", "EXPIRETIME", "PEXPIRETIME", "DBSIZE", "KEYS", "SCAN", "RANDOMKEY",
        "HGET", "HMGET", "HGETALL", "HKEYS", "HVALS", "HLEN", "HEXISTS", "HSTRLEN", "HSCAN", "HRANDFIELD",
        "LRANGE", "LLEN", "LINDEX", "LPOS", "SMEMBERS", "SCARD", "SISMEMBER", "SMISMEMBER", "SSCAN", "SRANDMEMBER", "SINTER", "SUNION", "SDIFF", "SINTERCARD",
        "ZRANGE", "ZRANGEBYSCORE", "ZRANGEBYLEX", "ZREVRANGE", "ZREVRANGEBYSCORE", "ZREVRANGEBYLEX", "ZSCORE", "ZMSCORE", "ZCARD", "ZCOUNT", "ZLEXCOUNT", "ZRANK", "ZREVRANK", "ZSCAN", "ZRANDMEMBER",
        "XRANGE", "XREVRANGE", "XLEN", "XINFO", "XPENDING", "PFCOUNT", "GETBIT", "BITCOUNT", "BITPOS", "GEOPOS", "GEODIST", "GEOSEARCH", "GEOHASH",
        "INFO", "PING", "TIME", "ECHO", "LASTSAVE", "ROLE", "LOLWUT", "DBTOOL.VIEW", "JSON.GET", "JSON.TYPE", "JSON.MGET",
    ];
    if READ.contains(&cmd) {
        return true;
    }
    match cmd {
        "MEMORY" => matches!(sub, "USAGE" | "STATS" | "DOCTOR"),
        "OBJECT" => matches!(sub, "ENCODING" | "FREQ" | "IDLETIME" | "REFCOUNT"),
        "CONFIG" => sub == "GET",
        "CLIENT" => matches!(sub, "LIST" | "INFO" | "GETNAME" | "ID"),
        "SLOWLOG" => matches!(sub, "GET" | "LEN"),
        "COMMAND" => matches!(sub, "" | "COUNT" | "INFO" | "DOCS"),
        _ => false,
    }
}

/// Run one console command
pub async fn run(m: &RedisConn, line: &str, limit: usize) -> AppResult<QueryResult> {
    let start = Instant::now();
    let line = line.trim().trim_end_matches(';').trim();
    let args = parse_command(line)?;
    let cmd = String::from_utf8_lossy(&args[0]).to_uppercase();
    if cmd == VIEW_CMD {
        let key = args.get(1).ok_or_else(|| AppError::Msg("key required".into()))?;
        return view_key(m, key, limit).await;
    }
    // The connection is shared, so DB switching goes through the console's database selection
    if cmd == "SELECT" {
        return Err(AppError::Msg("Use the database selector in the console toolbar to switch Redis databases".into()));
    }
    if matches!(cmd.as_str(), "SUBSCRIBE" | "PSUBSCRIBE" | "MONITOR" | "SSUBSCRIBE") {
        return Err(AppError::Msg(format!("{cmd} is not supported in the console")));
    }
    let mut c = redis::cmd(&cmd);
    for a in &args[1..] {
        c.arg(a.as_slice());
    }
    let mut mgr = m.clone();
    let v: RV = c.query_async(&mut mgr).await.map_err(err)?;
    Ok(shape(&cmd, &args[1..], v, limit, start))
}

async fn key_type(m: &mut RedisConn, key: &[u8]) -> AppResult<String> {
    redis::cmd("TYPE").arg(key).query_async(m).await.map_err(err)
}

/// Open a key: show its value as a table using the command for its type (DataGrip's Redis value viewer)
pub async fn view_key(m: &RedisConn, key: &[u8], limit: usize) -> AppResult<QueryResult> {
    let start = Instant::now();
    let mut mgr = m.clone();
    let ty = key_type(&mut mgr, key).await?;
    let lim = limit as i64;
    let q = |name: &str| redis::cmd(name);
    let r = match ty.as_str() {
        "none" => return Err(AppError::Msg(format!("Key not found: {}", String::from_utf8_lossy(key)))),
        "string" => {
            let v: RV = q("GET").arg(key).query_async(&mut mgr).await.map_err(err)?;
            table(&["value"], vec![vec![cell(&v)]], 1, start)
        }
        "hash" => {
            let mut rows = Vec::new();
            let mut cur = 0u64;
            loop {
                let (next, items): (u64, Vec<RV>) = q("HSCAN").arg(key).arg(cur).arg("COUNT").arg(500).query_async(&mut mgr).await.map_err(err)?;
                rows.extend(pairs(&items));
                cur = next;
                if cur == 0 || rows.len() > limit {
                    break;
                }
            }
            rows.sort_by(|a, b| a[0].as_str().cmp(&b[0].as_str()));
            table(&["field", "value"], rows, limit, start)
        }
        "list" => {
            let xs: Vec<RV> = q("LRANGE").arg(key).arg(0).arg(lim).query_async(&mut mgr).await.map_err(err)?;
            table(&["index", "value"], xs.iter().enumerate().map(|(i, x)| vec![Value::from(i as i64), cell(x)]).collect(), limit, start)
        }
        "set" => {
            let mut rows = Vec::new();
            let mut cur = 0u64;
            loop {
                let (next, items): (u64, Vec<RV>) = q("SSCAN").arg(key).arg(cur).arg("COUNT").arg(500).query_async(&mut mgr).await.map_err(err)?;
                rows.extend(items.iter().map(|x| vec![cell(x)]));
                cur = next;
                if cur == 0 || rows.len() > limit {
                    break;
                }
            }
            table(&["member"], rows, limit, start)
        }
        "zset" => {
            let xs: Vec<RV> = q("ZRANGE").arg(key).arg(0).arg(lim).arg("WITHSCORES").query_async(&mut mgr).await.map_err(err)?;
            table(&["member", "score"], pairs(&xs), limit, start)
        }
        "stream" => {
            let v: RV = q("XRANGE").arg(key).arg("-").arg("+").arg("COUNT").arg(lim + 1).query_async(&mut mgr).await.map_err(err)?;
            shape("XRANGE", &[], v, limit, start)
        }
        other => return Err(AppError::Msg(format!("Key type '{other}' is not supported by the value viewer; use the console"))),
    };
    Ok(r)
}

// ───────── Metadata ─────────

pub async fn list_databases(m: &RedisConn, current: i64) -> AppResult<Vec<String>> {
    if m.is_cluster() {
        return Ok(vec!["db0".into()]);
    }
    let mut mgr = m.clone();
    // DBs with keys + db0 + configured DBs (empty DBs are hidden, like DataGrip)
    let info: String = redis::cmd("INFO").arg("keyspace").query_async(&mut mgr).await.map_err(err)?;
    let mut dbs: Vec<i64> = info.lines().filter_map(|l| l.strip_prefix("db")?.split(':').next()?.parse().ok()).collect();
    dbs.push(0);
    dbs.push(current);
    dbs.sort();
    dbs.dedup();
    Ok(dbs.into_iter().map(|d| format!("db{d}")).collect())
}

/// Schema row in the tree (hidden, like MySQL)
pub fn list_schemas(db: &str) -> Vec<SchemaInfo> {
    vec![SchemaInfo { name: db.to_string(), is_system: false }]
}

async fn scan_keys(m: &RedisConn, pattern: &str, max: usize) -> AppResult<Vec<Vec<u8>>> {
    let mut out = Vec::new();
    for mut node in m.scan_nodes() {
        let mut cur = 0u64;
        loop {
            let (next, keys): (u64, Vec<Vec<u8>>) = redis::cmd("SCAN").arg(cur).arg("MATCH").arg(pattern).arg("COUNT").arg(1000).query_async(&mut node).await.map_err(err)?;
            out.extend(keys);
            cur = next;
            if cur == 0 || out.len() >= max {
                break;
            }
        }
        if out.len() >= max {
            break;
        }
    }
    out.truncate(max);
    out.sort();
    out.dedup();
    Ok(out)
}

/// Same command per key (cluster: slots differ, so run them one by one concurrently instead of a pipeline)
async fn per_key(m: &RedisConn, keys: &[Vec<u8>], cmds: &[&str]) -> AppResult<Vec<RV>> {
    if keys.is_empty() {
        return Ok(vec![]);
    }
    if !m.is_cluster() {
        let mut pipe = redis::pipe();
        for k in keys {
            for c in cmds {
                pipe.cmd(c).arg(k.as_slice());
            }
        }
        let mut mgr = m.clone();
        return pipe.query_async(&mut mgr).await.map_err(err);
    }
    let jobs: Vec<(Vec<u8>, String)> = keys.iter().flat_map(|k| cmds.iter().map(move |c| (k.clone(), c.to_string()))).collect();
    let mut out = vec![];
    for chunk in jobs.chunks(256) {
        let futs = chunk.iter().map(|(k, c)| {
            let mut mgr = m.clone();
            async move { redis::cmd(c).arg(k.as_slice()).query_async::<RV>(&mut mgr).await }
        });
        for r in futures::future::join_all(futs).await {
            out.push(r.map_err(err)?);
        }
    }
    Ok(out)
}

pub async fn list_objects(m: &RedisConn) -> AppResult<SchemaObjects> {
    let keys = scan_keys(m, "*", MAX_KEYS).await?;
    let types: Vec<String> = per_key(m, &keys, &["TYPE"]).await?.iter().map(text_of).collect();
    let relations = keys
        .iter()
        .zip(types)
        .map(|(k, t)| RelationInfo { name: String::from_utf8_lossy(k).into_owned(), kind: "key".into(), comment: Some(t), row_estimate: None })
        .collect();
    Ok(SchemaObjects { relations, routines: vec![] })
}

pub async fn object_index(m: &RedisConn, db: &str) -> AppResult<Vec<IndexEntry>> {
    Ok(scan_keys(m, "*", MAX_KEYS)
        .await?
        .into_iter()
        .map(|k| IndexEntry { schema: db.to_string(), name: String::from_utf8_lossy(&k).into_owned(), kind: "key".into() })
        .collect())
}

/// Go to DDL: commands that recreate the key
pub async fn key_ddl(m: &RedisConn, key: &str) -> AppResult<String> {
    let mut mgr = m.clone();
    let k = key.as_bytes();
    let ty = key_type(&mut mgr, k).await?;
    let qk = quote_arg(key);
    let qv = |v: &RV| quote_arg(&text_of(v));
    let mut lines = Vec::new();
    match ty.as_str() {
        "string" => {
            let v: RV = redis::cmd("GET").arg(k).query_async(&mut mgr).await.map_err(err)?;
            lines.push(format!("SET {qk} {}", qv(&v)));
        }
        "hash" => {
            let xs: Vec<RV> = redis::cmd("HGETALL").arg(k).query_async(&mut mgr).await.map_err(err)?;
            lines.push(format!("HSET {qk} {}", xs.iter().map(qv).collect::<Vec<_>>().join(" ")));
        }
        "list" => {
            let xs: Vec<RV> = redis::cmd("LRANGE").arg(k).arg(0).arg(-1).query_async(&mut mgr).await.map_err(err)?;
            lines.push(format!("RPUSH {qk} {}", xs.iter().map(qv).collect::<Vec<_>>().join(" ")));
        }
        "set" => {
            let xs: Vec<RV> = redis::cmd("SMEMBERS").arg(k).query_async(&mut mgr).await.map_err(err)?;
            lines.push(format!("SADD {qk} {}", xs.iter().map(qv).collect::<Vec<_>>().join(" ")));
        }
        "zset" => {
            let xs: Vec<RV> = redis::cmd("ZRANGE").arg(k).arg(0).arg(-1).arg("WITHSCORES").query_async(&mut mgr).await.map_err(err)?;
            let parts: Vec<String> = xs.chunks(2).map(|c| format!("{} {}", text_of(&c[1]), qv(&c[0]))).collect();
            lines.push(format!("ZADD {qk} {}", parts.join(" ")));
        }
        "none" => return Err(AppError::Msg(format!("Key not found: {key}"))),
        other => lines.push(format!("# {other} key: use DUMP / RESTORE")),
    }
    let ttl: i64 = redis::cmd("PTTL").arg(k).query_async(&mut mgr).await.map_err(err)?;
    if ttl > 0 {
        lines.push(format!("PEXPIRE {qk} {ttl}"));
    }
    Ok(lines.join("\n"))
}

pub async fn export(m: &RedisConn, line: &str, out: &mut crate::transfer::Exporter) -> AppResult<()> {
    let r = run(m, line, 1_000_000).await?;
    out.header(&r.columns.iter().map(|c| c.name.clone()).collect::<Vec<_>>()).await?;
    for row in &r.rows {
        out.row(row).await?;
    }
    Ok(())
}

// ───────── Medis-style key browser API ─────────

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyRow {
    pub key: String,
    #[serde(rename = "type")]
    pub ty: String,
    /// Milliseconds, -1 = no expiry
    pub ttl: i64,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanPage {
    pub cursor: String,
    pub keys: Vec<KeyRow>,
}

/// One SCAN (+ a few more if needed) to get about `count` keys with type·TTL
pub async fn scan_page(m: &RedisConn, cursor: &str, pattern: &str, count: usize, type_filter: Option<&str>) -> AppResult<ScanPage> {
    let nodes = m.scan_nodes();
    // Cursor: single node "123", cluster "node index:cursor"
    let (mut node, mut cur): (usize, u64) = match cursor.split_once(':') {
        Some((n, c)) => (n.parse().unwrap_or(0), c.parse().unwrap_or(0)),
        None => (0, cursor.parse().unwrap_or(0)),
    };
    let mut keys: Vec<Vec<u8>> = Vec::new();
    let pattern = if pattern.trim().is_empty() { "*".to_string() } else { pattern.trim().to_string() };
    // If the pattern matches only rare keys a single SCAN can come back empty, so repeat a few times
    for _ in 0..200 {
        let Some(n) = nodes.get(node) else { break };
        let mut mgr = n.clone();
        let mut c = redis::cmd("SCAN");
        c.arg(cur).arg("MATCH").arg(&pattern).arg("COUNT").arg(count.max(100));
        if let Some(t) = type_filter.filter(|t| !t.is_empty()) {
            c.arg("TYPE").arg(t);
        }
        let (next, ks): (u64, Vec<Vec<u8>>) = c.query_async(&mut mgr).await.map_err(err)?;
        keys.extend(ks);
        cur = next;
        if cur == 0 {
            node += 1;
        }
        if node >= nodes.len() || keys.len() >= count {
            break;
        }
    }
    let meta = per_key(m, &keys, &["TYPE", "PTTL"]).await?;
    let rows = keys
        .iter()
        .enumerate()
        .map(|(i, k)| KeyRow {
            key: String::from_utf8_lossy(k).into_owned(),
            ty: meta.get(i * 2).map(text_of).unwrap_or_default(),
            ttl: match meta.get(i * 2 + 1) {
                Some(RV::Int(t)) => *t,
                _ => -1,
            },
        })
        .collect();
    let cursor = if node >= nodes.len() { "0".to_string() } else if nodes.len() > 1 { format!("{node}:{cur}") } else { cur.to_string() };
    Ok(ScanPage { cursor, keys: rows })
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyDetail {
    #[serde(rename = "type")]
    pub ty: String,
    pub ttl: i64,
    /// Element count (byte length for string)
    pub length: i64,
    pub memory: Option<i64>,
    pub encoding: Option<String>,
    /// string: string / hash: [[field, value]] / list·set: [value] / zset: [[member, score]] / stream: [[id, {field: value}]]
    pub value: Value,
    /// Whether only part was fetched because the value has more than `limit`
    pub truncated: bool,
}

fn raw_text(v: &RV) -> Value {
    match v {
        RV::BulkString(b) => match std::str::from_utf8(b) {
            Ok(s) => Value::from(s),
            Err(_) => Value::from(String::from_utf8_lossy(b).into_owned()),
        },
        other => cell(other),
    }
}

/// Max bytes of a string value shown in key details (only the beginning if over, truncated)
pub const STRING_PREVIEW_MAX: i64 = 1 << 20;

/// Memory used by the key (bytes, 0 if missing). For checking size before reading the value
pub async fn memory_usage(m: &RedisConn, key: &[u8]) -> AppResult<i64> {
    let mut mgr = m.clone();
    let v: Option<i64> = redis::cmd("MEMORY").arg("USAGE").arg(key).query_async(&mut mgr).await.map_err(err)?;
    Ok(v.unwrap_or(0))
}

pub async fn key_detail(m: &RedisConn, key: &str, limit: usize) -> AppResult<KeyDetail> {
    let mut mgr = m.clone();
    let k = key.as_bytes();
    let ty = key_type(&mut mgr, k).await?;
    if ty == "none" {
        return Err(AppError::Msg(format!("Key not found: {key}")));
    }
    let ttl: i64 = redis::cmd("PTTL").arg(k).query_async(&mut mgr).await.map_err(err)?;
    let memory: Option<i64> = redis::cmd("MEMORY").arg("USAGE").arg(k).query_async(&mut mgr).await.ok();
    let encoding: Option<String> = redis::cmd("OBJECT").arg("ENCODING").arg(k).query_async(&mut mgr).await.ok();
    let len_cmd = match ty.as_str() {
        "string" => "STRLEN",
        "hash" => "HLEN",
        "list" => "LLEN",
        "set" => "SCARD",
        "zset" => "ZCARD",
        "stream" => "XLEN",
        _ => "",
    };
    let length: i64 = if len_cmd.is_empty() { 0 } else { redis::cmd(len_cmd).arg(k).query_async(&mut mgr).await.map_err(err)? };
    let lim = limit as i64;
    let value = match ty.as_str() {
        // Only the beginning of large strings (reading it whole would use up MCP · app memory)
        "string" if length > STRING_PREVIEW_MAX => {
            let v: RV = redis::cmd("GETRANGE").arg(k).arg(0).arg(STRING_PREVIEW_MAX - 1).query_async(&mut mgr).await.map_err(err)?;
            raw_text(&v)
        }
        "string" => {
            let v: RV = redis::cmd("GET").arg(k).query_async(&mut mgr).await.map_err(err)?;
            raw_text(&v)
        }
        "hash" => {
            let mut rows = Vec::new();
            let mut cur = 0u64;
            loop {
                let (next, items): (u64, Vec<RV>) = redis::cmd("HSCAN").arg(k).arg(cur).arg("COUNT").arg(500).query_async(&mut mgr).await.map_err(err)?;
                rows.extend(items.chunks(2).map(|c| Value::Array(vec![raw_text(&c[0]), c.get(1).map(raw_text).unwrap_or(Value::Null)])));
                cur = next;
                if cur == 0 || rows.len() >= limit {
                    break;
                }
            }
            rows.truncate(limit);
            rows.sort_by(|a, b| a[0].as_str().cmp(&b[0].as_str()));
            Value::Array(rows)
        }
        "list" => {
            let xs: Vec<RV> = redis::cmd("LRANGE").arg(k).arg(0).arg(lim - 1).query_async(&mut mgr).await.map_err(err)?;
            Value::Array(xs.iter().map(raw_text).collect())
        }
        "set" => {
            let mut rows = Vec::new();
            let mut cur = 0u64;
            loop {
                let (next, items): (u64, Vec<RV>) = redis::cmd("SSCAN").arg(k).arg(cur).arg("COUNT").arg(500).query_async(&mut mgr).await.map_err(err)?;
                rows.extend(items.iter().map(raw_text));
                cur = next;
                if cur == 0 || rows.len() >= limit {
                    break;
                }
            }
            rows.truncate(limit);
            rows.sort_by(|a, b| a.as_str().cmp(&b.as_str()));
            Value::Array(rows)
        }
        "zset" => {
            let xs: Vec<RV> = redis::cmd("ZRANGE").arg(k).arg(0).arg(lim - 1).arg("WITHSCORES").query_async(&mut mgr).await.map_err(err)?;
            Value::Array(xs.chunks(2).map(|c| Value::Array(vec![raw_text(&c[0]), c.get(1).map(raw_text).unwrap_or(Value::Null)])).collect())
        }
        "stream" => {
            let xs: Vec<RV> = redis::cmd("XRANGE").arg(k).arg("-").arg("+").arg("COUNT").arg(lim).query_async(&mut mgr).await.map_err(err)?;
            Value::Array(
                xs.iter()
                    .map(|e| match e {
                        RV::Array(p) if p.len() == 2 => {
                            let f = match &p[1] {
                                RV::Array(f) => Value::Object(f.chunks(2).map(|c| (text_of(&c[0]), c.get(1).map(raw_text).unwrap_or(Value::Null))).collect()),
                                o => cell(o),
                            };
                            Value::Array(vec![raw_text(&p[0]), f])
                        }
                        o => cell(o),
                    })
                    .collect(),
            )
        }
        _ => Value::Null,
    };
    let shown = match &value {
        Value::Array(a) => a.len() as i64,
        _ => length,
    };
    let truncated = (ty_is_collection(&value) && shown < length) || (ty == "string" && length > STRING_PREVIEW_MAX);
    Ok(KeyDetail { ty, ttl, length, memory, encoding, value, truncated })
}
fn ty_is_collection(v: &Value) -> bool {
    v.is_array()
}

/// Run an argument array as is (value editing·terminal). Result as nested JSON
pub async fn exec_args(m: &RedisConn, args: &[String]) -> AppResult<Value> {
    let first = args.first().ok_or_else(|| AppError::Msg("Empty command".into()))?;
    let cmd = first.to_uppercase();
    if cmd == "SELECT" {
        return Err(AppError::Msg("Switch databases with the database selector".into()));
    }
    let mut c = redis::cmd(&cmd);
    for a in &args[1..] {
        c.arg(a.as_str());
    }
    let mut mgr = m.clone();
    let v: RV = c.query_async(&mut mgr).await.map_err(err)?;
    Ok(json_of(&v))
}

fn json_of(v: &RV) -> Value {
    match v {
        RV::Array(xs) | RV::Set(xs) => Value::Array(xs.iter().map(json_of).collect()),
        RV::Map(kv) => Value::Array(kv.iter().flat_map(|(k, v)| [json_of(k), json_of(v)]).collect()),
        RV::Okay => serde_json::json!({ "status": "OK" }),
        RV::SimpleString(s) => serde_json::json!({ "status": s }),
        other => raw_text(other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Pool;
    use crate::store::DbKind;

    /// docker run -d --name dbtool-redis-test -p 16379:6379 redis:7-alpine
    fn cfg(db: &str) -> ConnectionConfig {
        ConnectionConfig {
            id: "rd".into(), name: "rd".into(), kind: DbKind::Redis, host: "127.0.0.1".into(), port: 16379, user: String::new(),
            database: db.into(), color: None, ssh: None, password_saved: false, ssh_secret_saved: false, scope: Default::default(),
            read_only: false, options: Default::default(), ssl: None, redis: None,
        }
    }

    #[test]
    fn parse_args() {
        let a = parse_command(r#"SET "my key" 'it''s' "a\"b\n\x41" plain"#).unwrap();
        let s: Vec<String> = a.iter().map(|x| String::from_utf8_lossy(x).into_owned()).collect();
        assert_eq!(s, ["SET", "my key", "its", "a\"b\nA", "plain"]);
        assert_eq!(parse_command(r#"GET """#).unwrap()[1], b"");
        assert!(parse_command("GET \"x").is_err());
        assert_eq!(quote_arg("a b"), "\"a b\"");
        assert_eq!(quote_arg("user:1"), "user:1");
    }

    /// MCP read: large values are rejected before reading, and key details read only the beginning of large strings
    #[tokio::test]
    #[ignore]
    async fn redis_big_values_are_not_read_whole() {
        let dir = std::env::temp_dir().join(format!("qurry-rbig-{}", uuid::Uuid::new_v4()));
        let st = crate::open_state(Some(dir)).unwrap();
        st.save_connection(cfg("db4"), None, None).await.unwrap();
        let pool = crate::db::connect(&cfg("db4"), None).await.expect("connect");
        let Pool::Redis(m, _) = &pool else { panic!() };
        let mut c = m.clone();
        let big = "x".repeat(17 << 20);
        let _: () = redis::cmd("SET").arg("qurry:big").arg(&big).query_async(&mut c).await.unwrap();
        let _: () = redis::cmd("SET").arg("qurry:small").arg("hi").query_async(&mut c).await.unwrap();
        let e = st.run_read_only("rd::db4".into(), "GET qurry:big".into(), 10).await.unwrap_err().to_string();
        assert!(e.contains("too big"), "{e}");
        assert!(st.run_read_only("rd::db4".into(), "MGET qurry:small qurry:big".into(), 10).await.is_err());
        assert!(st.run_read_only("rd::db4".into(), "GET qurry:small".into(), 10).await.is_ok());
        let d = key_detail(m, "qurry:big", 10).await.unwrap();
        assert!(d.truncated && d.value.as_str().unwrap().len() as i64 == STRING_PREVIEW_MAX);
        let _: () = redis::cmd("DEL").arg("qurry:big").arg("qurry:small").query_async(&mut c).await.unwrap();
    }

    #[tokio::test]
    #[ignore]
    async fn redis_end_to_end() {
        let pool = crate::db::connect(&cfg("db3"), None).await.expect("connect");
        assert!(pool.server_version().await.unwrap().starts_with("Redis 7"));
        let Pool::Redis(m, 3) = &pool else { panic!("db index") };
        let run = |s: &'static str| async move { crate::redisdb::run(m, s, 500).await };
        run("FLUSHDB").await.unwrap();
        for s in [
            "SET user:1:name \"홍 길동\"",
            "HSET user:1 name alice age 30 \"e mail\" a@b.c",
            "RPUSH queue a b c",
            "SADD tags x y",
            "ZADD rank 10 alice 20 bob",
            "XADD events * kind login who alice",
            "SET bin \"\\xff\\x00\"",
            "PEXPIRE user:1:name 600000",
        ] {
            run(s).await.unwrap_or_else(|e| panic!("{s}: {e}"));
        }
        let g = run("GET user:1:name").await.unwrap();
        assert_eq!(g.rows[0][0], "홍 길동");
        let h = run("HGETALL user:1").await.unwrap();
        assert_eq!(h.columns[0].name, "field");
        assert_eq!(h.rows.len(), 3);
        let z = run("ZRANGE rank 0 -1 WITHSCORES").await.unwrap();
        assert_eq!(z.columns.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["member", "score"]);
        let info = run("INFO server").await.unwrap();
        assert!(info.rows.iter().any(|r| r[1] == "redis_version"));
        assert!(run("SELECT 1").await.is_err());
        let ok = run("SET k v").await.unwrap();
        assert_eq!(ok.rows[0][0], "OK");
        let b = run("GET bin").await.unwrap();
        assert_eq!(b.rows[0][0], "0xFF00");
        // View a key (DBTOOL.VIEW)
        let v = run("DBTOOL.VIEW queue").await.unwrap();
        assert_eq!(v.rows.len(), 3);
        let v = run("DBTOOL.VIEW events").await.unwrap();
        assert_eq!(v.columns[0].name, "id");
        // Tree metadata
        let dbs = crate::meta::list_databases(&pool).await.unwrap();
        assert!(dbs.contains(&"db0".into()) && dbs.contains(&"db3".into()), "{dbs:?}");
        let objs = crate::meta::list_objects(&pool, "db3").await.unwrap();
        let k: Vec<_> = objs.relations.iter().map(|r| (r.name.as_str(), r.comment.as_deref().unwrap())).collect();
        assert!(k.contains(&("user:1", "hash")) && k.contains(&("events", "stream")), "{k:?}");
        let ddl = crate::meta::object_ddl(&pool, "db3", "user:1:name", "key").await.unwrap();
        assert!(ddl.starts_with("SET user:1:name \"홍 길동\"\nPEXPIRE user:1:name "), "{ddl}");
        let idx = crate::meta::object_index(&pool).await.unwrap();
        assert!(idx.iter().any(|e| e.name == "rank" && e.kind == "key"));
        // Medis API
        let page = scan_page(m, "0", "user:*", 100, None).await.unwrap();
        assert_eq!(page.keys.len(), 2);
        let nm = page.keys.iter().find(|k| k.key == "user:1:name").unwrap();
        assert!(nm.ty == "string" && nm.ttl > 0);
        let only_hash = scan_page(m, "0", "*", 100, Some("hash")).await.unwrap();
        assert_eq!(only_hash.keys.len(), 1);
        let d = key_detail(m, "user:1", 1000).await.unwrap();
        assert_eq!(d.ty, "hash");
        assert_eq!(d.length, 3);
        assert_eq!(d.value[0][0], "age");
        let d = key_detail(m, "rank", 1).await.unwrap();
        assert!(d.truncated && d.length == 2);
        let r = exec_args(m, &["HSET".into(), "user:1".into(), "age".into(), "31".into()]).await.unwrap();
        assert_eq!(r, 0);
        let r = exec_args(m, &["RENAME".into(), "tags".into(), "tags2".into()]).await.unwrap();
        assert_eq!(r["status"], "OK");
        let r = exec_args(m, &["LRANGE".into(), "queue".into(), "0".into(), "-1".into()]).await.unwrap();
        assert_eq!(r, serde_json::json!(["a", "b", "c"]));
        // Pools for different DBs don't mix
        let p0 = crate::db::connect(&cfg("db0"), None).await.unwrap();
        let Pool::Redis(m0, 0) = &p0 else { panic!() };
        assert!(crate::redisdb::run(m0, "EXISTS rank", 1).await.unwrap().rows[0][0] == 0);
    }

    fn tls_dir() -> String {
        std::env::var("QURRY_TEST_TLS_DIR").unwrap_or_else(|_| "/tmp/dbtool-tls".into())
    }

    /// docker: dbtool-redis-tls (16391, requirepass tlspass, tls-auth-clients optional)
    #[tokio::test]
    #[ignore]
    async fn redis_tls() {
        let d = tls_dir();
        let mut c = cfg("0");
        c.port = 16391;
        // 1) require: encryption only, no certificate check
        c.ssl = Some(crate::store::SslConfig { enabled: true, mode: "require".into(), ..Default::default() });
        let p = crate::db::connect(&c, Some("tlspass")).await.expect("tls require");
        assert!(p.server_version().await.unwrap().starts_with("Redis"));
        // 2) verify-full + CA + client certificate (mTLS)
        c.ssl = Some(crate::store::SslConfig { enabled: true, mode: "verify-full".into(), ca_file: format!("{d}/ca.pem"), cert_file: format!("{d}/client.pem"), key_file: format!("{d}/client-key.pem") });
        let p = crate::db::connect(&c, Some("tlspass")).await.expect("tls verify-full + mtls");
        let Pool::Redis(m, _) = &p else { panic!() };
        crate::redisdb::run(m, "SET tls:k ok", 1).await.unwrap();
        assert_eq!(crate::redisdb::run(m, "GET tls:k", 1).await.unwrap().rows[0][0], "ok");
        // 3) verify-full without the CA (self-signed) must be rejected
        c.ssl = Some(crate::store::SslConfig { enabled: true, mode: "verify-full".into(), ..Default::default() });
        assert!(crate::db::connect(&c, Some("tlspass")).await.is_err(), "self-signed must fail without CA");
        // 4) Plain text connection must fail
        c.ssl = None;
        assert!(crate::db::connect(&c, Some("tlspass")).await.is_err());
    }

    /// docker: dbtool-redis-sentinel (master 16392 requirepass spass, sentinel 26392 master mymaster)
    #[tokio::test]
    #[ignore]
    async fn redis_sentinel() {
        let mut c = cfg("2");
        c.port = 1; // unused: the sentinel node list is used
        c.redis = Some(crate::store::RedisMode { mode: "sentinel".into(), sentinel_master: "mymaster".into(), nodes: "127.0.0.1:1, 127.0.0.1:26392".into() });
        let p = crate::db::connect(&c, Some("spass")).await.expect("sentinel");
        let Pool::Redis(m, 2) = &p else { panic!() };
        crate::redisdb::run(m, "SET s:k viaSentinel", 1).await.unwrap();
        let info = crate::redisdb::run(m, "INFO replication", 50).await.unwrap();
        assert!(info.rows.iter().any(|r| r[1] == "role" && r[2] == "master"));
        let page = scan_page(m, "0", "s:*", 50, None).await.unwrap();
        assert_eq!(page.keys.len(), 1);
        c.redis.as_mut().unwrap().sentinel_master = "nope".into();
        let e = crate::db::connect(&c, Some("spass")).await.err().expect("unknown master").to_string();
        assert!(e.contains("does not know master"), "{e}");
    }

    /// docker: dbtool-redis-cluster (127.0.0.1:17001..17003, 3 masters)
    #[tokio::test]
    #[ignore]
    async fn redis_cluster() {
        let mut c = cfg("0");
        c.port = 17001;
        c.redis = Some(crate::store::RedisMode { mode: "cluster".into(), nodes: "127.0.0.1:17002".into(), ..Default::default() });
        let p = crate::db::connect(&c, None).await.expect("cluster");
        let Pool::Redis(m, _) = &p else { panic!() };
        assert!(m.is_cluster());
        // Several keys in different slots
        for i in 0..60 {
            crate::redisdb::run(m, &format!("SET ck:{i} v{i}"), 1).await.unwrap();
        }
        crate::redisdb::run(m, "HSET ch:1 a 1 b 2", 1).await.unwrap();
        assert_eq!(crate::redisdb::run(m, "GET ck:42", 1).await.unwrap().rows[0][0], "v42");
        // SCAN across all masters (small pages, continuing node:cursor)
        let mut seen = std::collections::HashSet::new();
        let mut cur = "0".to_string();
        let mut pages = 0;
        loop {
            let page = scan_page(m, &cur, "ck:*", 10, None).await.unwrap();
            for k in page.keys {
                assert_eq!(k.ty, "string");
                seen.insert(k.key);
            }
            pages += 1;
            cur = page.cursor;
            if cur == "0" || pages > 100 {
                break;
            }
        }
        assert_eq!(seen.len(), 60, "keys from all masters");
        let objs = crate::meta::list_objects(&p, "db0").await.unwrap();
        assert!(objs.relations.iter().any(|r| r.name == "ch:1" && r.comment.as_deref() == Some("hash")));
        assert_eq!(crate::meta::list_databases(&p).await.unwrap(), vec!["db0".to_string()]);
        let d = key_detail(m, "ch:1", 100).await.unwrap();
        assert_eq!(d.length, 2);
        assert!(crate::meta::object_index(&p).await.unwrap().len() >= 61);
    }

}
