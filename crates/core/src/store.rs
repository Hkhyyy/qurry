//! Connection settings store (JSON file). Passwords go in the encrypted file in secrets.rs.
use crate::error::AppResult;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::{fs, path::PathBuf};

/// User keyboard shortcut settings: action id → key list (only those differing from the defaults)
pub type Keymap = BTreeMap<String, Vec<String>>;


#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DbKind {
    Postgres,
    Mysql,
    Redis,
    // Below are driver plugins (downloaded when needed): crate::plugin
    Sqlite,
    Mssql,
    Clickhouse,
    Duckdb,
    Mongodb,
    Oracle,
    // Vector DBs (REST)
    Qdrant,
    Chroma,
    Milvus,
}

impl DbKind {
    /// Driver plugin name (None for the built-in PG·MySQL·Redis)
    pub fn driver(self) -> Option<&'static str> {
        match self {
            DbKind::Postgres | DbKind::Mysql | DbKind::Redis => None,
            DbKind::Sqlite => Some("sqlite"),
            DbKind::Mssql => Some("mssql"),
            DbKind::Clickhouse => Some("clickhouse"),
            DbKind::Duckdb => Some("duckdb"),
            DbKind::Mongodb => Some("mongodb"),
            DbKind::Oracle => Some("oracle"),
            DbKind::Qdrant => Some("qdrant"),
            DbKind::Chroma => Some("chroma"),
            DbKind::Milvus => Some("milvus"),
        }
    }

    /// The DB is the schema (no schema level in the tree)
    pub fn flat(self) -> bool {
        matches!(self, DbKind::Mysql | DbKind::Redis | DbKind::Sqlite | DbKind::Clickhouse | DbKind::Mongodb | DbKind::Qdrant | DbKind::Chroma | DbKind::Milvus)
    }

    /// A single file is the DB (file path instead of host·port)
    pub fn is_file(self) -> bool {
        matches!(self, DbKind::Sqlite | DbKind::Duckdb)
    }

    /// Vector DB: the console is REST (METHOD /path + JSON), not SQL
    pub fn is_vector(self) -> bool {
        matches!(self, DbKind::Qdrant | DbKind::Chroma | DbKind::Milvus)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionConfig {
    #[serde(default)]
    pub id: String,
    pub name: String,
    pub kind: DbKind,
    pub host: String,
    pub port: u16,
    pub user: String,
    #[serde(default)]
    pub database: String,
    /// DBs·schemas shown in the tree (Schemas tab of the DataGrip Data Source properties). Empty means all.
    #[serde(default)]
    pub scope: Scope,
    /// Whether the password was saved in the keychain (for list display — to avoid reading the keychain)
    #[serde(default)]
    pub password_saved: bool,
    #[serde(default)]
    pub ssh_secret_saved: bool,
    /// SSH tunnel (DataGrip's SSH/SSL tab)
    #[serde(default)]
    pub ssh: Option<SshConfig>,
    /// Color-code the connection like DataGrip (e.g. "#5c2b2b"). Default color if absent.
    #[serde(default)]
    pub color: Option<String>,
    /// Read-only from DataGrip's Options tab: open the session as a read-only transaction (the server rejects writes)
    #[serde(default)]
    pub read_only: bool,
    /// The remaining Connection settings from DataGrip's Options tab
    #[serde(default)]
    pub options: ConnOptions,
    /// SSL from the SSH/SSL tab (shared by PostgreSQL·MySQL·Redis)
    #[serde(default)]
    pub ssl: Option<SslConfig>,
    /// Redis connection mode (Standalone / Sentinel / Cluster)
    #[serde(default)]
    pub redis: Option<RedisMode>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct SslConfig {
    #[serde(default)]
    pub enabled: bool,
    /// require (encryption only, no certificate check) | verify-ca | verify-full (host name too)
    #[serde(default)]
    pub mode: String,
    #[serde(default)]
    pub ca_file: String,
    #[serde(default)]
    pub cert_file: String,
    #[serde(default)]
    pub key_file: String,
}

impl SslConfig {
    pub fn mode(&self) -> &str {
        if self.mode.is_empty() { "require" } else { &self.mode }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct RedisMode {
    /// standalone | sentinel | cluster
    #[serde(default)]
    pub mode: String,
    /// Master name watched by Sentinel
    #[serde(default)]
    pub sentinel_master: String,
    /// sentinel: Sentinel nodes / cluster: extra seed nodes ("host:port", separated by commas·spaces)
    #[serde(default)]
    pub nodes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ConnOptions {
    /// Default Tx mode for new consoles (Transaction control: Manual)
    #[serde(default)]
    pub tx_manual: bool,
    /// Default isolation level: "read uncommitted" | "read committed" | "repeatable read" | "serializable" (None = server default)
    #[serde(default)]
    pub isolation: Option<String>,
    /// Switch schema: "auto" | "manual" | "disable" (used by the frontend)
    #[serde(default)]
    pub switch_schema: Option<String>,
    /// Session TimeZone (None = server default)
    #[serde(default)]
    pub time_zone: Option<String>,
    /// Run keep-alive query each N seconds
    #[serde(default)]
    pub keep_alive_secs: Option<u32>,
    /// Auto-disconnect after N seconds (idle)
    #[serde(default)]
    pub auto_disconnect_secs: Option<u32>,
    /// Single database mode: only the default DB is shown and used
    #[serde(default)]
    pub single_db: bool,
    /// Startup script: run right after each connection is made
    #[serde(default)]
    pub startup_script: String,
    /// Production: statements that change data show the affected row count and ask for confirmation
    #[serde(default)]
    pub confirm_changes: bool,
    /// A remote MySQL with no SSL setting may connect in plain text if it can't do TLS (old servers · trusted networks)
    #[serde(default)]
    pub allow_plaintext: bool,
    /// Whether it's visible to the MCP server: "off" | "read" | "write" (None = source created before this was set, ConnectionConfig::mcp_access)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mcp: Option<String>,
    /// Oracle: Instant Client folder (if empty, ORACLE_CLIENT_LIB_DIR · system default)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oracle_client: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Scope {
    /// None = all DBs
    #[serde(default)]
    pub databases: Option<Vec<String>>,
    /// DB name → schemas to show (all if absent)
    #[serde(default)]
    pub schemas: std::collections::HashMap<String, Vec<String>>,
}

pub const SCOPE_ALL: &str = "*";
pub const SCOPE_DEFAULT: &str = "@default";

/// Rules for which DBs·schemas show in the tree (same as dbVisible / schemaVisible in GUI src/lib/api.ts)
impl Scope {
    pub fn db_visible(&self, db: &str, default_db: &str) -> bool {
        match self.databases.as_deref() {
            None | Some([]) => true,
            Some(d) => d.iter().any(|x| x == SCOPE_ALL || x == db || (x == SCOPE_DEFAULT && db == default_db)),
        }
    }
    pub fn schema_visible(&self, db: &str, schema: &str, is_system: bool, show_system: bool) -> bool {
        match self.schemas.get(db).map(|v| v.as_slice()) {
            None | Some([]) => show_system || !is_system,
            Some(s) if s.iter().any(|x| x == SCOPE_ALL) => show_system || !is_system,
            Some(s) => s.iter().any(|x| x == schema || (x == SCOPE_DEFAULT && schema == "public")),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum SshAuth {
    #[default]
    Password,
    Key,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SshConfig {
    #[serde(default)]
    pub enabled: bool,
    pub host: String,
    #[serde(default = "default_ssh_port")]
    pub port: u16,
    pub user: String,
    #[serde(default)]
    pub auth: SshAuth,
    #[serde(default)]
    pub key_path: String,
}

fn default_ssh_port() -> u16 {
    22
}

impl ConnectionConfig {
    pub fn active_ssl(&self) -> Option<&SslConfig> {
        self.ssl.as_ref().filter(|s| s.enabled)
    }

    pub fn redis_mode(&self) -> &str {
        match self.redis.as_ref().map(|r| r.mode.as_str()) {
            Some("sentinel") => "sentinel",
            Some("cluster") => "cluster",
            _ => "standalone",
        }
    }

    pub fn active_ssh(&self) -> Option<&SshConfig> {
        self.ssh.as_ref().filter(|s| s.enabled && !s.host.is_empty())
    }

    /// Whether this is the same SSH server, so the saved SSH secret may be sent
    pub fn same_ssh(&self, other: &ConnectionConfig) -> bool {
        let t = |c: &ConnectionConfig| c.ssh.as_ref().map(|s| (s.host.trim().to_lowercase(), s.port, s.user.clone()));
        t(self) == t(other)
    }

    /// Whether self's (saved settings) password may be sent to other: same kind · server · user · SSH tunnel, and TLS not turned off.
    /// So an old password doesn't go to a new server when only the host is changed before Test, or a different server is imported with the same id
    /// Redis Sentinel · Cluster nodes are servers too. Turning SSL off, lowering verification, or changing the CA counts as a different server
    pub fn same_server(&self, other: &ConnectionConfig) -> bool {
        let t = |c: &ConnectionConfig| {
            // Only Redis has nodes; no Redis settings = standalone (the TUI form always fills them in)
            let redis = (c.kind == DbKind::Redis).then(|| {
                let r = c.redis.clone().unwrap_or_default();
                (c.redis_mode().to_string(), r.sentinel_master.trim().to_string(), r.nodes.split([',', ' ']).filter(|n| !n.is_empty()).map(|n| n.to_lowercase()).collect::<Vec<_>>())
            });
            (c.kind, c.host.trim().to_lowercase(), c.port, c.user.clone(), c.active_ssh().map(|s| (s.host.trim().to_lowercase(), s.port)), redis)
        };
        let rank = |c: &ConnectionConfig| match c.active_ssl().map(|s| s.mode()) {
            None => 0,
            Some("verify-full") => 3,
            Some("verify-ca") => 2,
            Some(_) => 1,
        };
        let ca = |c: &ConnectionConfig| c.active_ssl().map(|s| s.ca_file.trim().to_string()).unwrap_or_default();
        // Allowing plain text where it wasn't allowed is a weaker connection too
        t(self) == t(other) && rank(other) >= rank(self) && (rank(self) < 2 || ca(self) == ca(other)) && (self.options.allow_plaintext || !other.options.allow_plaintext)
    }
}

/// What the MCP server may do with this Data Source
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum McpAccess {
    /// Not listed, and its connection·password are not used
    Off,
    Read,
    /// Also execute, when started with --allow-write
    Write,
}

impl McpAccess {
    /// The options.mcp value that stores this access
    pub fn as_str(self) -> &'static str {
        match self {
            McpAccess::Off => "off",
            McpAccess::Read => "read",
            McpAccess::Write => "write",
        }
    }
}

impl ConnectionConfig {
    /// MCP exposure. Old sources without a setting are hidden if Production, otherwise as before (read, write with --allow-write).
    /// Read-only Data Sources have no write
    pub fn mcp_access(&self, allow_write: bool) -> McpAccess {
        let a = match self.options.mcp.as_deref() {
            Some("off") => McpAccess::Off,
            Some("read") => McpAccess::Read,
            Some("write") => McpAccess::Write,
            // Any other value (a typo, "On", "") is Off — never more than the user picked
            Some(_) => McpAccess::Off,
            None if self.options.confirm_changes => McpAccess::Off,
            None => McpAccess::Write,
        };
        if a == McpAccess::Write && (!allow_write || self.read_only) { McpAccess::Read } else { a }
    }
}

/// Strip user:pass@ from URIs like mongodb://user:pass@host (so passwords don't end up in the AI conversation or an error message)
pub fn without_userinfo(host: &str) -> String {
    match host.split_once("://") {
        // The password may contain raw / ? #, so treat everything up to the last @ as user info
        Some((scheme, rest)) => match rest.rfind('@') {
            Some(at) => format!("{scheme}://{}", &rest[at + 1..]),
            None => host.to_string(),
        },
        None => host.rsplit_once('@').map(|(_, h)| h.to_string()).unwrap_or_else(|| host.to_string()),
    }
}

/// Storage key for the SSH password/key passphrase
pub fn ssh_secret_key(id: &str) -> String {
    format!("{id}#ssh")
}

/// Export file format. Passwords are not included.
#[derive(Debug, Serialize, Deserialize)]
pub struct ExportFile {
    pub app: String,
    pub version: u32,
    pub connections: Vec<ConnectionConfig>,
}

pub struct Store {
    path: PathBuf,
}

impl Store {
    pub fn new(dir: PathBuf) -> Self {
        Self {
            path: dir.join("connections.json"),
        }
    }

    pub fn list(&self) -> AppResult<Vec<ConnectionConfig>> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }
        Ok(serde_json::from_str(&fs::read_to_string(&self.path)?)?)
    }

    pub fn get(&self, id: &str) -> AppResult<Option<ConnectionConfig>> {
        Ok(self.list()?.into_iter().find(|c| c.id == id))
    }

    pub fn save(&self, config: &ConnectionConfig) -> AppResult<()> {
        let mut all = self.list()?;
        match all.iter_mut().find(|c| c.id == config.id) {
            Some(existing) => *existing = config.clone(),
            None => all.push(config.clone()),
        }
        self.write(&all)
    }

    pub fn delete(&self, id: &str) -> AppResult<()> {
        let all: Vec<_> = self.list()?.into_iter().filter(|c| c.id != id).collect();
        self.write(&all)
    }

    pub fn export_to(&self, path: &std::path::Path) -> AppResult<usize> {
        let connections = self.list()?;
        let n = connections.len();
        let file = ExportFile { app: "dbtool".into(), version: 1, connections };
        crate::secrets::write_private(path, serde_json::to_string_pretty(&file)?.as_bytes())?;
        Ok(n)
    }

    /// Overwrite the same id, add new ids. Returns: (added, updated)
    fn read_import(path: &std::path::Path) -> AppResult<Vec<ConnectionConfig>> {
        let text = fs::read_to_string(path)?;
        Ok(match serde_json::from_str::<ExportFile>(&text) {
            Ok(f) => f.connections,
            Err(_) => serde_json::from_str(&text)?,
        })
    }

    /// What changes before importing (a file from someone else can turn off Read-only or add SQL to run on connect)
    pub fn import_preview(&self, path: &std::path::Path) -> AppResult<Vec<String>> {
        let all = self.list()?;
        let mut out = vec![];
        for c in Self::read_import(path)? {
            let script = |c: &ConnectionConfig| c.options.startup_script.trim().to_string();
            match all.iter().find(|e| !c.id.is_empty() && e.id == c.id) {
                None => {
                    let mut line = format!("+ add \"{}\" ({}@{}:{})", c.name, c.user, c.host, c.port);
                    let mut notes = vec![];
                    if !script(&c).is_empty() {
                        notes.push("runs a startup script on connect".to_string());
                    }
                    // What the import will really allow (unset becomes Off, see import_from)
                    let mut as_saved = c.clone();
                    as_saved.options.mcp.get_or_insert_with(|| "off".into());
                    match as_saved.mcp_access(true) {
                        McpAccess::Off => {}
                        McpAccess::Read => notes.push("shared with MCP (read)".into()),
                        McpAccess::Write => notes.push("shared with MCP (read & write)".into()),
                    }
                    if c.active_ssl().is_some_and(|s| s.mode() == "require") {
                        notes.push("SSL without certificate checks".into());
                    }
                    if c.options.allow_plaintext {
                        notes.push("allows unencrypted connections".into());
                    }
                    if c.options.oracle_client.as_deref().is_some_and(|p| !p.trim().is_empty()) {
                        notes.push("loads Oracle client libraries from a folder".into());
                    }
                    if !notes.is_empty() {
                        line += &format!(" — {}", notes.join(", "));
                    }
                    out.push(line);
                }
                Some(e) => {
                    let mut ch = vec![];
                    if (e.host.as_str(), e.port, e.user.as_str()) != (c.host.as_str(), c.port, c.user.as_str()) {
                        ch.push(format!("server {}@{}:{} → {}@{}:{}", e.user, e.host, e.port, c.user, c.host, c.port));
                    }
                    if e.kind != c.kind {
                        ch.push(format!("changes the type {:?} → {:?}", e.kind, c.kind));
                    }
                    if e.read_only && !c.read_only {
                        ch.push("turns Read-only OFF".into());
                    }
                    if e.active_ssl().is_some() && c.active_ssl().is_none() {
                        ch.push("turns SSL OFF (the password would be sent unencrypted)".into());
                    } else if let (Some(a), Some(b)) = (e.active_ssl(), c.active_ssl())
                        && a.mode() != b.mode()
                    {
                        ch.push(format!("SSL {} → {}", a.mode(), b.mode()));
                    }
                    let files = |x: &ConnectionConfig| x.ssl.as_ref().map(|s| (s.ca_file.clone(), s.cert_file.clone(), s.key_file.clone()));
                    if files(e) != files(&c) {
                        ch.push("changes the SSL certificate files".into());
                    }
                    if e.redis.as_ref().map(|r| (&r.mode, &r.nodes, &r.sentinel_master)) != c.redis.as_ref().map(|r| (&r.mode, &r.nodes, &r.sentinel_master)) {
                        ch.push("changes the Redis nodes".into());
                    }
                    if !e.options.allow_plaintext && c.options.allow_plaintext {
                        ch.push("allows unencrypted connections (the password may travel in plain text)".into());
                    }
                    if e.options.oracle_client != c.options.oracle_client {
                        ch.push("changes the Oracle client library folder".into());
                    }
                    if e.options.confirm_changes && !c.options.confirm_changes {
                        ch.push("turns Production OFF".into());
                    }
                    let mut as_saved = c.clone();
                    as_saved.options.mcp.get_or_insert_with(|| e.mcp_access(true).as_str().into());
                    if e.mcp_access(true) < as_saved.mcp_access(true) {
                        ch.push("shares more with MCP (AI tools)".into());
                    }
                    if !e.same_server(&c) && e.password_saved {
                        ch.push("the saved password will be removed".into());
                    }
                    if e.active_ssh().map(|s| (&s.host, s.port)) != c.active_ssh().map(|s| (&s.host, s.port)) {
                        ch.push("changes the SSH tunnel".into());
                    }
                    if script(e) != script(&c) {
                        ch.push("changes the startup script".into());
                    }
                    out.push(format!("~ update \"{}\"{}", e.name, if ch.is_empty() { String::new() } else { format!(": {}", ch.join(", ")) }));
                }
            }
        }
        Ok(out)
    }

    /// (added, changed, sources whose passwords to delete: (id, DB password, SSH secret)). The saved flags follow the current values, not the file
    pub fn import_from(&self, path: &std::path::Path) -> AppResult<(usize, usize, Vec<(String, bool, bool)>)> {
        let incoming = Self::read_import(path)?;
        let mut all = self.list()?;
        let (mut added, mut updated, mut forget) = (0, 0, vec![]);
        for mut c in incoming {
            if c.id.is_empty() {
                c.id = uuid::Uuid::new_v4().to_string();
            }
            match all.iter_mut().find(|e| e.id == c.id) {
                Some(e) => {
                    // A file without an MCP setting keeps what the source has now (not the legacy default for unset)
                    if c.options.mcp.is_none() {
                        c.options.mcp = Some(e.mcp_access(true).as_str().into());
                    }
                    // Regardless of the flags (including entries whose flag alone was cleared by an old import), a different server goes on the delete list
                    let (pw, ssh) = (!e.same_server(&c), !e.same_ssh(&c));
                    c.password_saved = e.password_saved && !pw;
                    c.ssh_secret_saved = e.ssh_secret_saved && !ssh;
                    if pw || ssh {
                        forget.push((c.id.clone(), pw, ssh));
                    }
                    *e = c;
                    updated += 1;
                }
                None => {
                    // New sources have no saved password, and MCP is Off unless the file sets it
                    c.password_saved = false;
                    c.ssh_secret_saved = false;
                    if c.options.mcp.is_none() {
                        c.options.mcp = Some("off".into());
                    }
                    all.push(c);
                    added += 1;
                }
            }
        }
        self.write(&all)?;
        Ok((added, updated, forget))
    }

    /// Config folder (where connections.json lives)
    pub fn dir(&self) -> PathBuf {
        self.path.parent().map(|p| p.to_path_buf()).unwrap_or_default()
    }

    fn keymap_path(&self) -> PathBuf {
        self.path.with_file_name("keymap.json")
    }

    /// One section of keymap.json ("gui" / "tui"). Empty settings if the file doesn't exist
    pub fn keymap(&self, section: &str) -> AppResult<Keymap> {
        let path = self.keymap_path();
        if !path.exists() {
            return Ok(Keymap::new());
        }
        let mut all: BTreeMap<String, Keymap> = serde_json::from_str(&fs::read_to_string(path)?)?;
        Ok(all.remove(section).unwrap_or_default())
    }

    /// Change only one section and leave the other (GUI ↔ TUI) as is
    pub fn save_keymap(&self, section: &str, map: &Keymap) -> AppResult<()> {
        let path = self.keymap_path();
        let mut all: BTreeMap<String, Keymap> = if path.exists() { serde_json::from_str(&fs::read_to_string(&path)?)? } else { BTreeMap::new() };
        all.insert(section.to_string(), map.clone());
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        crate::secrets::write_private(&path, serde_json::to_string_pretty(&all)?.as_bytes())?;
        Ok(())
    }

    /// Open tabs·split layout (restored on restart, like DataGrip). client = "gui" / "tui", in separate files (so concurrent writes don't overwrite each other).
    /// 0600 since it contains console SQL. The content is up to the client
    /// A file in the config folder
    pub(crate) fn file(&self, name: &str) -> PathBuf {
        self.path.with_file_name(name)
    }

    pub fn workspace(&self, client: &str) -> Option<serde_json::Value> {
        let s = fs::read_to_string(self.path.with_file_name(format!("workspace-{client}.json"))).ok()?;
        serde_json::from_str(&s).ok()
    }

    pub fn save_workspace(&self, client: &str, v: &serde_json::Value) -> AppResult<()> {
        crate::secrets::write_private(&self.path.with_file_name(format!("workspace-{client}.json")), serde_json::to_string(v)?.as_bytes())
    }

    fn write(&self, all: &[ConnectionConfig]) -> AppResult<()> {
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir)?;
        }
        // Contains host·user·SSH key paths → owner-only read
        crate::secrets::write_private(&self.path, serde_json::to_string_pretty(all)?.as_bytes())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_secrets_follow_the_server() {
        let a: ConnectionConfig = serde_json::from_value(serde_json::json!({ "id": "x", "name": "a", "kind": "postgres", "host": "db.internal", "port": 5432, "user": "app",
            "ssl": { "enabled": true, "mode": "verify-full" }, "ssh": { "enabled": true, "host": "bastion", "port": 22, "user": "me" } })).unwrap();
        let mut b = a.clone();
        b.name = "renamed".into();
        b.color = Some("#f00".into());
        assert!(a.same_server(&b) && a.same_ssh(&b));
        b.host = "DB.internal ".into();
        assert!(a.same_server(&b));
        let changes: [fn(&mut ConnectionConfig); 9] = [|c| c.host = "attacker.example".into(), |c| c.port = 5433, |c| c.user = "root".into(), |c| c.kind = DbKind::Mysql,
            |c| c.ssl.as_mut().unwrap().enabled = false, |c| c.ssh.as_mut().unwrap().host = "evil".into(), |c| c.ssl.as_mut().unwrap().mode = "require".into(),
            |c| c.ssl.as_mut().unwrap().ca_file = "/tmp/evil-ca.pem".into(), |c| c.options.allow_plaintext = true];
        for f in changes {
            let mut c = a.clone();
            f(&mut c);
            assert!(!a.same_server(&c), "{c:?}");
        }
        let mut c = a.clone();
        c.ssh.as_mut().unwrap().user = "other".into();
        assert!(!a.same_ssh(&c));
        // Redis: Sentinel / Cluster nodes are servers too; no settings = standalone
        let r: ConnectionConfig = serde_json::from_value(serde_json::json!({ "id": "r", "name": "r", "kind": "redis", "host": "cache", "port": 6379, "user": "" })).unwrap();
        let mut same = r.clone();
        same.redis = Some(RedisMode { mode: "standalone".into(), sentinel_master: String::new(), nodes: String::new() });
        assert!(r.same_server(&same), "the TUI form's filled-in standalone settings are the same server");
        let mut moved = r.clone();
        moved.redis = Some(RedisMode { mode: "sentinel".into(), sentinel_master: "m".into(), nodes: "evil:26379".into() });
        assert!(!r.same_server(&moved));
    }

    #[test]
    fn mcp_access_defaults() {
        let mut c: ConnectionConfig = serde_json::from_value(serde_json::json!({ "name": "a", "kind": "postgres", "host": "h", "port": 1, "user": "u" })).unwrap();
        // Old source: as before (read, write with --allow-write)
        assert_eq!(c.mcp_access(false), McpAccess::Read);
        assert_eq!(c.mcp_access(true), McpAccess::Write);
        // Production stays hidden until set
        c.options.confirm_changes = true;
        assert_eq!(c.mcp_access(true), McpAccess::Off);
        c.options.mcp = Some("write".into());
        assert_eq!(c.mcp_access(true), McpAccess::Write);
        assert_eq!(c.mcp_access(false), McpAccess::Read);
        c.read_only = true;
        assert_eq!(c.mcp_access(true), McpAccess::Read);
        c.options.mcp = Some("off".into());
        assert_eq!(c.mcp_access(true), McpAccess::Off);
        for odd in ["On", "", "WRITE", "yes"] {
            c.options.mcp = Some(odd.into());
            assert_eq!(c.mcp_access(true), McpAccess::Off, "{odd}");
        }
    }

    #[test]
    fn scope_rules_match_gui() {
        let sc = Scope {
            databases: Some(vec!["sales".into(), "billing".into()]),
            schemas: [("billing".to_string(), vec!["public".to_string(), "crm".to_string()])].into_iter().collect(),
        };
        assert!(!sc.db_visible("postgres", "postgres"), "default DB hidden when not selected");
        assert!(sc.db_visible("billing", "postgres"));
        assert!(sc.schema_visible("billing", "crm", false, false));
        assert!(!sc.schema_visible("billing", "audit", false, false));
        assert!(sc.schema_visible("sales", "anything", false, false));
        assert!(!sc.schema_visible("sales", "pg_catalog", true, false));
        let def = Scope { databases: Some(vec![SCOPE_DEFAULT.into()]), ..Default::default() };
        assert!(def.db_visible("postgres", "postgres") && !def.db_visible("x", "postgres"));
        assert!(Scope::default().db_visible("anything", "postgres"));
    }

    fn conn(id: &str, name: &str) -> ConnectionConfig {
        ConnectionConfig {
            id: id.into(),
            name: name.into(),
            kind: DbKind::Postgres,
            host: "localhost".into(),
            port: 5432,
            user: "u".into(),
            database: String::new(),
            color: Some("#5c2b2b".into()),
            ssh: None,
            password_saved: false,
            ssh_secret_saved: false,
            scope: Default::default(), read_only: false, options: Default::default(), ssl: None, redis: None,
        }
    }

    #[test]
    fn export_import_roundtrip() {
        let base = std::env::temp_dir().join(format!("dbtool-store-{}", uuid::Uuid::new_v4()));
        let a = Store::new(base.join("a"));
        a.save(&conn("1", "DEV")).unwrap();
        a.save(&conn("2", "PROD")).unwrap();
        let file = base.join("export.json");
        std::fs::create_dir_all(&base).unwrap();
        assert_eq!(a.export_to(&file).unwrap(), 2);

        let b = Store::new(base.join("b"));
        b.save(&conn("2", "old name")).unwrap();
        assert_eq!(b.import_from(&file).unwrap(), (1, 1, vec![]));
        // Newly imported sources get MCP Off (unless the file sets it)
        assert_eq!(b.get("1").unwrap().unwrap().options.mcp.as_deref(), Some("off"));
        let names: Vec<_> = b.list().unwrap().into_iter().map(|c| c.name).collect();
        assert_eq!(names, vec!["PROD", "DEV"]);
        assert_eq!(b.get("1").unwrap().unwrap().color.as_deref(), Some("#5c2b2b"));
        let _ = std::fs::remove_dir_all(&base);
    }

    /// Importing a different server with the same id doesn't use the saved password (delete list · preview warning)
    #[test]
    fn import_to_another_server_forgets_the_password() {
        let base = std::env::temp_dir().join(format!("dbtool-store-{}", uuid::Uuid::new_v4()));
        let b = Store::new(base.join("b"));
        let mut mine = conn("2", "PROD");
        mine.password_saved = true;
        mine.ssl = Some(SslConfig { enabled: true, mode: "verify-full".into(), ..Default::default() });
        b.save(&mine).unwrap();
        let mut theirs = mine.clone();
        theirs.host = "attacker.example".into();
        theirs.ssl = None;
        let file = base.join("in.json");
        std::fs::write(&file, serde_json::to_string(&vec![theirs]).unwrap()).unwrap();
        let p = b.import_preview(&file).unwrap().join("\n");
        assert!(p.contains("attacker.example") && p.contains("SSL OFF") && p.contains("password will be removed"), "{p}");
        assert_eq!(b.import_from(&file).unwrap(), (0, 1, vec![("2".to_string(), true, false)]));
        assert!(!b.get("2").unwrap().unwrap().password_saved);
        let _ = std::fs::remove_dir_all(&base);
    }

    /// An update without an MCP setting keeps the current access; turning on plain text shows in the preview
    #[test]
    fn import_update_keeps_mcp_and_shows_plaintext() {
        let base = std::env::temp_dir().join(format!("dbtool-store-{}", uuid::Uuid::new_v4()));
        let b = Store::new(base.join("b"));
        let mut mine = conn("3", "db");
        mine.options.mcp = Some("off".into());
        b.save(&mine).unwrap();
        let mut theirs = mine.clone();
        theirs.options.mcp = None;
        theirs.options.allow_plaintext = true;
        let file = base.join("in.json");
        std::fs::write(&file, serde_json::to_string(&vec![theirs]).unwrap()).unwrap();
        let p = b.import_preview(&file).unwrap().join("\n");
        assert!(p.contains("unencrypted") && !p.contains("MCP"), "{p}");
        b.import_from(&file).unwrap();
        assert_eq!(b.get("3").unwrap().unwrap().mcp_access(true), McpAccess::Off);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn keymap_sections_are_independent() {
        let base = std::env::temp_dir().join(format!("dbtool-keymap-{}", uuid::Uuid::new_v4()));
        let s = Store::new(base.clone());
        assert!(s.keymap("tui").unwrap().is_empty());
        let tui: Keymap = [("NewConsole".to_string(), vec!["Ctrl+N".to_string()])].into_iter().collect();
        let gui: Keymap = [("console.new".to_string(), vec!["Mod+T".to_string()])].into_iter().collect();
        s.save_keymap("tui", &tui).unwrap();
        s.save_keymap("gui", &gui).unwrap();
        assert_eq!(s.keymap("tui").unwrap(), tui);
        assert_eq!(s.keymap("gui").unwrap(), gui);
        let _ = std::fs::remove_dir_all(&base);
    }
}
