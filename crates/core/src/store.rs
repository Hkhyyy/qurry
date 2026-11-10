//! Connection settings store (JSON file). Passwords go in the encrypted file in secrets.rs.
use crate::error::{AppError, AppResult};
use qurry_shared::theme::{self, CustomTheme, ThemeSettings};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::{fs, path::Path, path::PathBuf};

/// User keyboard shortcut settings: action id → key list (only those differing from the defaults)
pub type Keymap = BTreeMap<String, Vec<String>>;

pub use qurry_shared::kind::DbKind;
pub use crate::import::{ImportPlan, ImportRow};

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
    /// AWS SSM Session Manager tunnel (the other choice next to SSH). Older apps ignore it and drop it when they save, so it is
    /// kept in connection-tunnels.json too (Store::list), and the DB password of an SSM source is saved under a key they don't read
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssm: Option<SsmConfig>,
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
    /// Production: consoles start in Manual transaction mode (#106). None = on: sources saved before this, and by older apps that
    /// drop the field, get it too
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub production_manual: Option<bool>,
    /// A remote MySQL with no SSL setting may connect in plain text if it can't do TLS (old servers · trusted networks).
    /// This and mcp are also saved in connection-access.json, which wins over connections.json (Store::list)
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

/// AWS SSM port forwarding: `aws ssm start-session` to the DB host/port through this instance. No AWS secrets here:
/// the AWS CLI uses the user's own config, SSO login or environment
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct SsmConfig {
    #[serde(default)]
    pub enabled: bool,
    /// AWS profile (empty = the CLI's default / AWS_PROFILE)
    #[serde(default)]
    pub profile: String,
    /// AWS region (empty = the profile's / AWS_REGION)
    #[serde(default)]
    pub region: String,
    /// Target instance id (i-… or a managed node mi-…)
    #[serde(default)]
    pub target: String,
}

fn default_ssh_port() -> u16 {
    22
}

impl ConnectionConfig {
    /// Whether its consoles can be in Manual transaction mode (Session::set_manual): PostgreSQL and MySQL. Redis and the driver
    /// plugins refuse it, so their consoles always start in Auto
    pub fn supports_manual(&self) -> bool {
        matches!(self.kind, DbKind::Postgres | DbKind::Mysql)
    }

    /// Whether a new console starts in Manual transaction mode: the source's Transaction control, or Production with
    /// production_manual on (the default). The GUI has the same rule (src/features/tabs/ops.ts startsManual)
    pub fn starts_manual(&self) -> bool {
        (self.supports_manual() && self.options.tx_manual) || self.production_starts_manual()
    }

    /// Manual only because it is Production (#106): the console says so the first time (errors::PRODUCTION_MANUAL)
    pub fn production_starts_manual(&self) -> bool {
        let o = &self.options;
        self.supports_manual() && !o.tx_manual && o.confirm_changes && o.production_manual != Some(false)
    }

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

    /// Turned on, even without a target: connecting then fails with "enter the instance id" instead of going straight to the host
    pub fn active_ssm(&self) -> Option<&SsmConfig> {
        self.ssm.as_ref().filter(|s| s.enabled)
    }

    /// Where the DB password is saved. Through AWS SSM it is `<id>#ssm`, which older apps (that don't know SSM and would
    /// connect straight to the host) never read: they find no saved password and ask for it
    pub fn password_key(&self) -> String {
        if self.active_ssm().is_some() { format!("{}#ssm", self.id) } else { self.id.clone() }
    }

    /// Whether the connection goes through a tunnel (SSH or AWS SSM): the client connects to a port on this machine
    pub fn tunneled(&self) -> bool {
        self.active_ssh().is_some() || self.active_ssm().is_some()
    }

    /// Whether this is the same SSH server, so the saved SSH secret may be sent
    pub fn same_ssh(&self, other: &ConnectionConfig) -> bool {
        let t = |c: &ConnectionConfig| c.ssh.as_ref().map(|s| (s.host.trim().to_lowercase(), s.port, s.user.trim().to_string()));
        t(self) == t(other)
    }

    /// Whether self's (saved settings) password may be sent to other: same kind · server · user · SSH / AWS SSM tunnel, and TLS not turned off.
    /// So an old password doesn't go to a new server when only the host is changed before Test, or a different server is imported with the same id
    /// Redis Sentinel · Cluster nodes are servers too. Turning SSL off, lowering verification, or changing the CA counts as a different server
    pub fn same_server(&self, other: &ConnectionConfig) -> bool {
        let t = |c: &ConnectionConfig| {
            // Only Redis has nodes; no Redis settings = standalone (the TUI form always fills them in)
            let redis = (c.kind == DbKind::Redis).then(|| {
                let r = c.redis.clone().unwrap_or_default();
                (c.redis_mode().to_string(), r.sentinel_master.trim().to_string(), r.nodes.split([',', ' ']).filter(|n| !n.is_empty()).map(|n| n.to_lowercase()).collect::<Vec<_>>())
            });
            // The SSM target (and the profile · region that pick the AWS account) is where the DB host is resolved, so part of the server
            let ssm = c.active_ssm().map(|s| (s.profile.trim().to_string(), s.region.trim().to_lowercase(), s.target.trim().to_lowercase()));
            (c.kind, c.host.trim().to_lowercase(), c.port, c.user.trim().to_string(), c.active_ssh().map(|s| (s.host.trim().to_lowercase(), s.port)), ssm, redis)
        };
        let rank = |c: &ConnectionConfig| match c.active_ssl().map(|s| s.mode()) {
            None => 0,
            Some("verify-full") => 3,
            Some("verify-ca") => 2,
            Some(_) => 1,
        };
        let ca = |c: &ConnectionConfig| c.active_ssl().map(|s| s.ca_file.trim().to_string()).unwrap_or_default();
        // "Allow unencrypted connection" isn't checked: the destination stays the same (and it's what the TLS error tells the user to turn on)
        t(self) == t(other) && rank(other) >= rank(self) && (rank(self) < 2 || ca(self) == ca(other))
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
    /// Read-only and Production Data Sources have no write: an AI tool's change would skip the confirm a person gets there
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
        if a == McpAccess::Write && (!allow_write || self.read_only || self.options.confirm_changes) { McpAccess::Read } else { a }
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

/// Every key a source's DB password may be saved under (ConnectionConfig::password_key)
pub fn password_keys(id: &str) -> [String; 2] {
    [id.to_string(), format!("{id}#ssm")]
}

/// The data source a secrets key belongs to (DB password, SSH secret, or the DB password of an SSM source)
pub fn secret_owner(key: &str) -> &str {
    key.strip_suffix("#ssh").or_else(|| key.strip_suffix("#ssm")).unwrap_or(key)
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
    /// Temporary data sources (`qurry-tui postgres://…`): listed and found like saved ones, but only in this process. Never written
    /// to connections.json, so the GUI, other TUIs and the MCP server never see them
    temporary: std::sync::Mutex<Vec<ConnectionConfig>>,
}

pub const TEMPORARY_NOT_SAVED: &str = "This data source comes from the qurry-tui command line: it is temporary, so it isn't saved and its form can't change it. To keep it, add a data source with these settings (n in the explorer).";

/// Per-source settings that older apps (0.2.12 and before) don't know. They are kept in connection-access.json too (by connection id),
/// because an older app saving connections.json drops fields it doesn't know — an MCP Off source would be shared again.
/// connections.json still has them, for when the side file is missing or has no entry for a source
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct Access {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    mcp: Option<String>,
    #[serde(default)]
    allow_plaintext: bool,
}

/// The table a table tab shows, which its column widths are kept for (settings.json "columnWidths"): data source id, database
/// (empty: the source's default, which the store names), schema, table
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TableRef {
    pub source: String,
    pub database: String,
    pub schema: String,
    pub table: String,
}

impl TableRef {
    /// A saved "columnWidths" entry is this table's
    fn is(&self, e: &serde_json::Value) -> bool {
        let f = |k: &str| e.get(k).and_then(|v| v.as_str());
        f("source") == Some(&self.source) && f("database") == Some(&self.database) && f("schema") == Some(&self.schema) && f("table") == Some(&self.table)
    }
}

/// A source whose saved passwords an import deletes: (id, DB password, SSH secret)
type Forget = (String, bool, bool);

impl Store {
    pub fn new(dir: PathBuf) -> Self {
        Self {
            path: dir.join("connections.json"),
            temporary: Default::default(),
        }
    }

    /// Saved sources, then the temporary ones of this process
    pub fn list(&self) -> AppResult<Vec<ConnectionConfig>> {
        let mut all = self.saved()?;
        all.extend(self.temporary.lock().unwrap().iter().cloned());
        Ok(all)
    }

    /// A data source only for this process (Store::temporary)
    pub fn add_temporary(&self, config: ConnectionConfig) {
        self.temporary.lock().unwrap().push(config);
    }

    pub fn is_temporary(&self, id: &str) -> bool {
        self.temporary.lock().unwrap().iter().any(|c| c.id == id)
    }

    /// What connections.json (and its side files) hold
    fn saved(&self) -> AppResult<Vec<ConnectionConfig>> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }
        let mut all: Vec<ConnectionConfig> = serde_json::from_str(&fs::read_to_string(&self.path)?)?;
        // The side file wins; without an entry, what connections.json has (written by builds before the side file)
        let access = self.access()?;
        // AWS SSM settings an older app dropped from connections.json come back from their own side file (no older app writes it)
        let tunnels = self.tunnels()?;
        for c in &mut all {
            if let Some(a) = access.get(&c.id) {
                c.options.allow_plaintext = a.allow_plaintext;
                if a.mcp.is_some() {
                    c.options.mcp = a.mcp.clone();
                }
            }
            if c.ssm.is_none() {
                c.ssm = tunnels.get(&c.id).cloned();
            }
        }
        Ok(all)
    }

    fn tunnels_path(&self) -> PathBuf {
        self.path.with_file_name("connection-tunnels.json")
    }

    /// Connection id → AWS SSM settings. Damaged: an error (empty would let an SSM source connect straight to its host)
    fn tunnels(&self) -> AppResult<BTreeMap<String, SsmConfig>> {
        let path = self.tunnels_path();
        match fs::read_to_string(&path) {
            Ok(s) => serde_json::from_str(&s).map_err(|e| crate::error::AppError::Msg(format!("{} is damaged ({e}) — restore it from a backup, or move it away and check the AWS SSM tunnel of your data sources", path.display()))),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(BTreeMap::new()),
            Err(e) => Err(e.into()),
        }
    }

    fn access_path(&self) -> PathBuf {
        self.path.with_file_name("connection-access.json")
    }

    /// Damaged: an error, not empty (empty would share Off sources with MCP again)
    fn access(&self) -> AppResult<BTreeMap<String, Access>> {
        let path = self.access_path();
        match fs::read_to_string(&path) {
            Ok(s) => serde_json::from_str(&s).map_err(|e| crate::error::AppError::Msg(format!("{} is damaged ({e}) — restore it from a backup, or move it away: MCP access and \"Allow unencrypted connection\" are then read from connections.json (check them if an older Qurry version saved your data sources since)", path.display()))),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(BTreeMap::new()),
            Err(e) => Err(e.into()),
        }
    }

    pub fn get(&self, id: &str) -> AppResult<Option<ConnectionConfig>> {
        Ok(self.list()?.into_iter().find(|c| c.id == id))
    }

    pub fn save(&self, config: &ConnectionConfig) -> AppResult<()> {
        self.refuse_temporary(&config.id)?;
        self.update(|all| {
            match all.iter_mut().find(|c| c.id == config.id) {
                Some(existing) => *existing = config.clone(),
                None => all.push(config.clone()),
            }
            Ok(())
        })
    }

    /// Change part of one source (scope, color …) under the lock, so another app's change to it in between isn't reverted. None if it doesn't exist
    /// A temporary source is changed in memory only
    pub fn modify<T>(&self, id: &str, f: impl FnOnce(&mut ConnectionConfig) -> AppResult<T>) -> AppResult<Option<T>> {
        if let Some(c) = self.temporary.lock().unwrap().iter_mut().find(|c| c.id == id) {
            return f(c).map(Some);
        }
        self.update(|all| all.iter_mut().find(|c| c.id == id).map(f).transpose())
    }

    fn refuse_temporary(&self, id: &str) -> AppResult<()> {
        if self.is_temporary(id) { Err(crate::error::AppError::Msg(TEMPORARY_NOT_SAVED.into())) } else { Ok(()) }
    }

    /// Save a whole source, deciding against the saved one under the lock: `f` gets what is saved now (None for a new source)
    /// and the config to save, and may refuse (nothing is written then)
    pub fn save_with(&self, mut config: ConnectionConfig, f: impl FnOnce(Option<&ConnectionConfig>, &mut ConnectionConfig) -> AppResult<()>) -> AppResult<ConnectionConfig> {
        self.refuse_temporary(&config.id)?;
        self.update(|all| {
            let i = all.iter().position(|c| c.id == config.id);
            f(i.map(|i| &all[i]), &mut config)?;
            match i {
                Some(i) => all[i] = config.clone(),
                None => all.push(config.clone()),
            }
            Ok(config)
        })
    }

    pub fn delete(&self, id: &str) -> AppResult<()> {
        if self.is_temporary(id) {
            self.temporary.lock().unwrap().retain(|c| c.id != id);
            return Ok(());
        }
        self.update(|all| {
            all.retain(|c| c.id != id);
            Ok(())
        })
    }

    /// Read · change · write under a file lock shared by GUI · TUI · MCP (so concurrent saves don't lose one side's change)
    fn update<T>(&self, f: impl FnOnce(&mut Vec<ConnectionConfig>) -> AppResult<T>) -> AppResult<T> {
        self.locked(|mut all| {
            let out = f(&mut all)?;
            self.write(&all)?;
            Ok(out)
        })
    }

    /// The sources under the same lock, without writing: `f` never sees a save that is half done (its secrets set, the store not yet written)
    pub fn locked<T>(&self, f: impl FnOnce(Vec<ConnectionConfig>) -> AppResult<T>) -> AppResult<T> {
        if let Some(dir) = self.path.parent() {
            crate::secrets::private_dir(dir)?;
        }
        let lock = fs::OpenOptions::new().create(true).truncate(false).write(true).open(self.path.with_extension("json.lock"))?;
        lock.lock()?;
        f(self.saved()?)
    }

    pub fn export_to(&self, path: &std::path::Path) -> AppResult<usize> {
        let connections = self.saved()?;
        let n = connections.len();
        let file = ExportFile { app: "dbtool".into(), version: 1, connections };
        crate::secrets::write_private(path, serde_json::to_string_pretty(&file)?.as_bytes())?;
        Ok(n)
    }

    /// What importing `path` would do (Qurry export, DataGrip / IntelliJ, DBeaver: crate::import::read). `from` names it in the
    /// question (a found location's label), otherwise it's described from the path
    pub fn import_plan(&self, path: &std::path::Path, home: &std::path::Path, from: Option<&str>) -> AppResult<ImportPlan> {
        let all = self.list()?;
        let shown = crate::import::shown(path, home);
        Ok(match crate::import::read(path, home)? {
            crate::import::Read::Foreign(sources) => crate::import::foreign_plan(&from.map(str::to_string).unwrap_or_else(|| crate::import::describe(path, home)), &shown, &all, sources),
            crate::import::Read::Qurry(incoming) => Self::qurry_plan(from.unwrap_or("a Qurry export"), &shown, &all, incoming),
        })
    }

    /// A Qurry export: what changes before importing (a file from someone else can turn off Read-only or add SQL to run on connect)
    fn qurry_plan(from: &str, shown: &str, all: &[ConnectionConfig], incoming: Vec<ConnectionConfig>) -> ImportPlan {
        let mut rows = vec![];
        for c in &incoming {
            let kind = crate::import::kind_name(c.kind, "");
            let script = |c: &ConnectionConfig| c.options.startup_script.trim().to_string();
            match all.iter().find(|e| !c.id.is_empty() && e.id == c.id) {
                None => {
                    let mut notes = vec![];
                    if !script(c).is_empty() {
                        notes.push("runs a startup script on connect".to_string());
                    }
                    // What the import will really allow (unset becomes Off, see merge_import)
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
                    let (value, tone) = if notes.is_empty() { ("password on connect".to_string(), None) } else { (notes.join(", "), Some("warn")) };
                    rows.push(ImportRow::new(format!("+ add \"{}\" ({})", c.name, crate::import::server(c)), kind, value, tone, true));
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
                    if files(e) != files(c) {
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
                    if Self::import_moves_server(e, c) && e.password_saved {
                        ch.push("the saved password will be removed and asked again".into());
                    }
                    if e.active_ssh().map(|s| (&s.host, s.port)) != c.active_ssh().map(|s| (&s.host, s.port)) {
                        ch.push("changes the SSH tunnel".into());
                    }
                    if e.active_ssm() != c.active_ssm() {
                        ch.push("changes the AWS SSM tunnel".into());
                    }
                    if script(e) != script(c) {
                        ch.push("changes the startup script".into());
                    }
                    let note = if ch.is_empty() { kind } else { format!("{kind} · {}", ch.join(", ")) };
                    let tone = (!ch.is_empty()).then_some("warn");
                    rows.push(ImportRow::new(format!("~ update \"{}\"", e.name), note, "update".into(), tone, true));
                }
            }
        }
        let added = rows.iter().filter(|r| r.text.starts_with('+')).count();
        let updated = rows.len() - added;
        let message = if rows.is_empty() {
            format!("Nothing to import: {from} has no data sources.\n{shown}")
        } else {
            // An update keeps the saved password unless its row says it will be removed
            let pw = match (added, updated) {
                (_, 0) => "Passwords aren't in the file: each asks on its first connect.",
                (0, _) => "Passwords aren't in the file: an updated data source keeps its saved password unless its row says otherwise.",
                _ => "Passwords aren't in the file: new data sources ask on their first connect, updated ones keep their saved password unless their row says otherwise.",
            };
            format!("Import these data sources from {from}?\n{shown}\n\n{added} to add, {updated} to update. {pw}")
        };
        ImportPlan { title: crate::import::TITLE.into(), message: crate::import::clean_message(message), importable: rows.len(), rows, configs: incoming.into_iter().map(Some).collect() }
    }

    /// Import the checked rows of `plan` (`picked[i]` for row i; missing = checked). Sources from another tool that Qurry has
    /// meanwhile got (another app imported them) are left out.
    /// (added, changed, sources whose passwords to delete: (id, DB password, SSH secret)). The saved flags follow the current values, not the file.
    /// `forget` deletes those passwords before the store is written (under its lock): if it fails, nothing is imported — otherwise the store would
    /// say "no password" (or allow plain text) while the old password stayed in the file
    pub fn import_apply(&self, plan: &ImportPlan, picked: &[bool], forget: impl FnOnce(&[Forget]) -> AppResult<()>) -> AppResult<(usize, usize, Vec<Forget>)> {
        let chosen: Vec<ConnectionConfig> = plan.configs.iter().enumerate().filter(|(i, _)| picked.get(*i).copied().unwrap_or(true)).filter_map(|(_, c)| c.clone()).collect();
        self.update(|all| {
            let incoming = chosen.into_iter().filter(|c| !c.id.is_empty() || !all.iter().any(|e| crate::import::same_source(e, c))).collect();
            let r = Self::merge_import(all, incoming)?;
            forget(&r.2)?;
            Ok(r)
        })
    }

    /// Like a server change for the saved password: same_server, plus a file turning on "Allow unencrypted"
    /// (which the user may tick in the dialog, but an imported file could use to have the password sent without TLS)
    fn import_moves_server(e: &ConnectionConfig, c: &ConnectionConfig) -> bool {
        !e.same_server(c) || (!e.options.allow_plaintext && c.options.allow_plaintext)
    }

    fn merge_import(all: &mut Vec<ConnectionConfig>, incoming: Vec<ConnectionConfig>) -> AppResult<(usize, usize, Vec<Forget>)> {
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
                    let (pw, ssh) = (Self::import_moves_server(e, &c), !e.same_ssh(&c));
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

    /// settings.json: app settings the GUI and the TUI share. Keys this version doesn't know are kept
    fn settings(&self) -> serde_json::Map<String, serde_json::Value> {
        fs::read_to_string(self.file("settings.json")).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
    }

    /// Update channel (stable unless beta was picked; an unknown value is stable too)
    pub fn update_channel(&self) -> crate::update::Channel {
        self.settings().remove("updateChannel").and_then(|v| serde_json::from_value(v).ok()).unwrap_or_default()
    }

    pub fn set_update_channel(&self, channel: crate::update::Channel) -> AppResult<()> {
        self.set_setting("updateChannel", serde_json::to_value(channel)?)
    }

    /// Keyword case as you type and in Reformat Code, for the GUI console and the TUI editor (Off unless picked; an unknown value is Off too)
    pub fn keyword_case(&self) -> crate::format::KeywordCase {
        self.settings().remove("keywordCase").and_then(|v| serde_json::from_value(v).ok()).unwrap_or_default()
    }

    pub fn set_keyword_case(&self, case: crate::format::KeywordCase) -> AppResult<()> {
        self.set_setting("keywordCase", serde_json::to_value(case)?)
    }

    /// Grid alignment per value class: only the classes the user changed (GUI Settings › Appearance › Alignment; the TUI reads it)
    pub fn grid_align(&self) -> crate::format::Alignments {
        self.settings().get("gridAlign").map(crate::format::alignments).unwrap_or_default()
    }

    /// Change one class, on what the file holds now (the GUI and the TUI may both have changed it); its default drops the override,
    /// so a later default applies. Returns what is saved
    pub fn set_grid_align(&self, class: crate::format::ValueClass, align: crate::format::Align) -> AppResult<crate::format::Alignments> {
        let mut next = self.grid_align();
        if align == class.default_align() {
            next.remove(&class);
        } else {
            next.insert(class, align);
        }
        self.set_setting("gridAlign", serde_json::to_value(&next)?)?;
        Ok(next)
    }

    /// The status bar items on (GUI status bar and Settings › Appearance, TUI ⌃k › Status Bar Items…): the defaults until changed
    pub fn status_bar_items(&self) -> Vec<crate::statusbar::Item> {
        crate::statusbar::items(self.settings().get("statusBarItems"))
    }

    /// Turn one item on or off, on what the file holds now (the GUI and the TUI may both have changed it). Returns what is saved
    pub fn set_status_bar_item(&self, item: crate::statusbar::Item, on: bool) -> AppResult<Vec<crate::statusbar::Item>> {
        let next = crate::statusbar::toggled(&self.status_bar_items(), item, on);
        self.set_setting("statusBarItems", serde_json::to_value(&next)?)?;
        Ok(next)
    }

    /// Restore Defaults: the key goes, so a later version's defaults apply
    pub fn reset_status_bar_items(&self) -> AppResult<Vec<crate::statusbar::Item>> {
        self.remove_setting("statusBarItems")?;
        Ok(self.status_bar_items())
    }

    /// The settings.json "columnWidths" entries: one per table that has set widths
    fn column_width_entries(&self) -> Vec<serde_json::Value> {
        match self.settings().remove("columnWidths") {
            Some(serde_json::Value::Array(v)) => v,
            _ => vec![],
        }
    }

    /// `t` with its database named: a tab on the source's default database may name none (the GUI's), or the default (the TUI's)
    fn table_ref(&self, t: &TableRef) -> TableRef {
        match self.get(&t.source) {
            Ok(Some(c)) if t.database.trim().is_empty() => TableRef { database: crate::sql::default_db(&c), ..t.clone() },
            _ => t.clone(),
        }
    }

    /// The widths a user gave a table tab's columns (GUI and TUI), empty when none; the others keep their computed widths
    pub fn column_widths(&self, t: &TableRef) -> crate::format::ColumnWidths {
        let t = self.table_ref(t);
        self.column_width_entries().iter().find(|e| t.is(e)).and_then(|e| e.get("widths")).map(crate::format::column_widths).unwrap_or_default()
    }

    /// Set some of a table's column widths, on what the file holds now (the GUI and the TUI may both have changed it); returns what is saved.
    /// A width of 0 forgets that column's (it goes back to its computed width: a second fit's way back)
    pub fn set_column_widths(&self, t: &TableRef, widths: &crate::format::ColumnWidths) -> AppResult<crate::format::ColumnWidths> {
        let mut next = self.column_widths(t);
        for (c, &w) in widths {
            if w == 0 {
                next.remove(c);
            } else {
                next.insert(c.clone(), crate::format::column_width(w.into(), 0));
            }
        }
        self.save_column_widths(t, &next)?;
        Ok(next)
    }

    /// Reset Widths: the table's columns go back to their computed widths
    pub fn reset_column_widths(&self, t: &TableRef) -> AppResult<()> {
        self.save_column_widths(t, &Default::default())
    }

    fn save_column_widths(&self, t: &TableRef, widths: &crate::format::ColumnWidths) -> AppResult<()> {
        let t = self.table_ref(t);
        let mut all = self.column_width_entries();
        all.retain(|e| !t.is(e));
        if !widths.is_empty() {
            all.push(serde_json::json!({ "source": t.source, "database": t.database, "schema": t.schema, "table": t.table, "widths": widths }));
        }
        self.set_setting("columnWidths", all.into())
    }

    /// The theme settings the GUI and the TUI share: "theme" (a theme id or "system"), "themeSync" and "customThemes". Before
    /// 0.3.2 only the TUI kept its theme here ("tuiTheme": auto · dark · light, auto being Sync with OS); it stands in until a
    /// theme is picked. A custom theme this version can't read is left out
    pub fn theme_settings(&self) -> ThemeSettings {
        let mut all = self.settings();
        let str_of = |v: Option<serde_json::Value>| v.and_then(|v| v.as_str().map(String::from));
        let theme = str_of(all.remove("theme")).or_else(|| str_of(all.remove("tuiTheme")).map(|t| if t == "auto" { theme::SYSTEM.into() } else { t }));
        let sync = all.remove("themeSync").and_then(|v| serde_json::from_value(v).ok()).unwrap_or_default();
        let custom = match all.remove("customThemes") {
            Some(serde_json::Value::Array(v)) => v.into_iter().filter_map(|t| serde_json::from_value::<CustomTheme>(t).ok()).filter(|t| theme::check(t).is_ok()).collect(),
            _ => vec![],
        };
        ThemeSettings { theme, sync, custom }
    }

    /// Pick a theme for both apps: a built-in or custom theme id, or "system"
    pub fn set_theme(&self, id: &str) -> AppResult<()> {
        if id != theme::SYSTEM && self.theme_settings().find(id).is_none() {
            return Err(AppError::Msg(format!("There is no theme \"{id}\"")));
        }
        self.set_setting("theme", id.into())
    }

    /// Sync with OS: the themes for a dark and a light appearance
    pub fn set_theme_sync(&self, sync: &theme::ThemeSync) -> AppResult<()> {
        let s = self.theme_settings();
        if let Some(id) = [&sync.dark, &sync.light].into_iter().find(|id| s.find(id).is_none()) {
            return Err(AppError::Msg(format!("There is no theme \"{id}\"")));
        }
        self.set_setting("themeSync", serde_json::to_value(sync)?)
    }

    /// Add a custom theme (an empty id gets a new one) or replace the one with its id, after checking it: a known base, known
    /// colors, #rrggbb only. Returns the saved theme
    pub fn save_custom_theme(&self, t: &CustomTheme) -> AppResult<CustomTheme> {
        theme::check(t).map_err(AppError::Msg)?;
        let mut t = CustomTheme { name: t.name.trim().to_string(), ..t.clone() };
        let mut list = self.theme_settings().custom;
        if t.id.is_empty() {
            t.id = (1..).map(|n| format!("custom-{n}")).find(|id| !list.iter().any(|c| &c.id == id)).unwrap_or_default();
        } else if !t.id.starts_with("custom-") {
            return Err(AppError::Msg(format!("A custom theme's id starts with \"custom-\", not \"{}\"", t.id)));
        }
        match list.iter_mut().find(|c| c.id == t.id) {
            Some(c) => *c = t.clone(),
            None => list.push(t.clone()),
        }
        self.set_setting("customThemes", serde_json::to_value(&list)?)?;
        Ok(t)
    }

    /// Delete a custom theme; where it is picked (the theme, or a Sync with OS side) its base takes its place
    pub fn delete_custom_theme(&self, id: &str) -> AppResult<()> {
        let s = self.theme_settings();
        let Some(gone) = s.custom.iter().find(|c| c.id == id) else { return Ok(()) };
        let list: Vec<_> = s.custom.iter().filter(|c| c.id != id).collect();
        self.set_setting("customThemes", serde_json::to_value(&list)?)?;
        if s.theme.as_deref() == Some(id) {
            self.set_setting("theme", gone.base.clone().into())?;
        }
        if s.sync.dark == id || s.sync.light == id {
            let side = |v: &String| if v == id { gone.base.clone() } else { v.clone() };
            self.set_setting("themeSync", serde_json::to_value(theme::ThemeSync { dark: side(&s.sync.dark), light: side(&s.sync.light) })?)?;
        }
        Ok(())
    }

    /// Settings › Appearance › Theme › Import…: the file read and checked (a large one is refused unread), not saved yet
    pub fn read_theme_file(&self, path: &Path) -> AppResult<CustomTheme> {
        let size = fs::metadata(path)?.len();
        if size > theme::FILE_MAX as u64 {
            return Err(AppError::Msg(format!("Not a Qurry theme: the file is {} KB, a theme file is at most {} KB", size.div_ceil(1024), theme::FILE_MAX / 1024)));
        }
        theme::parse_file(&fs::read_to_string(path)?).map_err(AppError::Msg)
    }

    /// Export…: the theme as a file (its name, base and changed colors)
    pub fn write_theme_file(&self, path: &Path, t: &CustomTheme) -> AppResult<()> {
        theme::check(t).map_err(AppError::Msg)?;
        Ok(fs::write(path, theme::file_text(t))?)
    }

    /// Change one key of settings.json, keeping the others
    fn set_setting(&self, key: &str, value: serde_json::Value) -> AppResult<()> {
        let mut all = self.settings();
        all.insert(key.into(), value);
        self.write_settings(&all)
    }

    fn remove_setting(&self, key: &str) -> AppResult<()> {
        let mut all = self.settings();
        all.remove(key);
        self.write_settings(&all)
    }

    fn write_settings(&self, all: &serde_json::Map<String, serde_json::Value>) -> AppResult<()> {
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir)?;
        }
        crate::secrets::write_private(&self.file("settings.json"), serde_json::to_string_pretty(all)?.as_bytes())
    }

    fn write(&self, all: &[ConnectionConfig]) -> AppResult<()> {
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir)?;
        }
        let access: BTreeMap<_, _> = all.iter().map(|c| (c.id.clone(), Access { mcp: c.options.mcp.clone(), allow_plaintext: c.options.allow_plaintext })).collect();
        // Before connections.json, so the SSM setting can't be lost in between. A source whose SSM settings are removed loses its entry
        let tunnels: BTreeMap<_, _> = all.iter().filter_map(|c| Some((c.id.clone(), c.ssm.clone()?))).collect();
        if !tunnels.is_empty() || self.tunnels_path().exists() {
            crate::secrets::write_private(&self.tunnels_path(), serde_json::to_string_pretty(&tunnels)?.as_bytes())?;
        }
        // The side file first, keeping entries of removed sources: if connections.json can't be written, the side file still matches either version
        let mut merged = self.access()?;
        merged.extend(access.clone());
        crate::secrets::write_private(&self.access_path(), serde_json::to_string_pretty(&merged)?.as_bytes())?;
        // Contains host·user·SSH key paths → owner-only read
        crate::secrets::write_private(&self.path, serde_json::to_string_pretty(all)?.as_bytes())?;
        // Only now drop the entries of removed sources
        if merged.len() != access.len() {
            crate::secrets::write_private(&self.access_path(), serde_json::to_string_pretty(&access)?.as_bytes())?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The update channel is stable until beta is picked, and saving it keeps the other settings in settings.json
    #[test]
    fn update_channel_setting() {
        use crate::update::Channel;
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path().join("cfg"));
        assert_eq!(store.update_channel(), Channel::Stable);
        store.set_update_channel(Channel::Beta).unwrap();
        assert_eq!(store.update_channel(), Channel::Beta);
        let path = dir.path().join("cfg/settings.json");
        fs::write(&path, r#"{"updateChannel": "nightly", "later": 1}"#).unwrap();
        assert_eq!(store.update_channel(), Channel::Stable, "an unknown channel is stable");
        store.set_update_channel(Channel::Beta).unwrap();
        let saved: serde_json::Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(saved, serde_json::json!({ "updateChannel": "beta", "later": 1 }));
    }

    /// Theme, Sync with OS and custom themes round trip through settings.json for both apps; the TUI's older "tuiTheme" stands in
    /// until a theme is picked; deleting the picked custom theme leaves its base picked; theme files are checked
    #[test]
    fn theme_settings_round_trip() {
        use qurry_shared::theme::{ThemeSync, SYSTEM};
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path().join("cfg"));
        let path = dir.path().join("cfg/settings.json");
        assert_eq!(store.theme_settings(), ThemeSettings::default());
        fs::create_dir_all(dir.path().join("cfg")).unwrap();
        fs::write(&path, r#"{"tuiTheme": "auto", "later": 1}"#).unwrap();
        assert_eq!(store.theme_settings().theme.as_deref(), Some(SYSTEM), "the TUI's Auto is Sync with OS");
        store.set_theme("dracula").unwrap();
        assert!(store.set_theme("monokai").unwrap_err().to_string().contains("no theme"));
        let mine = CustomTheme { id: String::new(), name: " Night ".into(), base: "dracula".into(), colors: [("bg".into(), "#101010".into())].into() };
        let saved = store.save_custom_theme(&mine).unwrap();
        assert_eq!((saved.id.as_str(), saved.name.as_str()), ("custom-1", "Night"));
        assert_eq!(store.save_custom_theme(&mine).unwrap().id, "custom-2", "Save as new theme twice: two themes");
        store.save_custom_theme(&CustomTheme { name: "Night 2".into(), ..saved.clone() }).unwrap();
        assert!(store.save_custom_theme(&CustomTheme { colors: [("bg".into(), "#10101".into())].into(), ..mine.clone() }).is_err());
        assert!(store.save_custom_theme(&CustomTheme { id: "dark".into(), ..mine.clone() }).is_err(), "not over a built-in id");
        store.set_theme("custom-1").unwrap();
        store.set_theme_sync(&ThemeSync { dark: "custom-1".into(), light: "solarized-light".into() }).unwrap();
        assert!(store.set_theme_sync(&ThemeSync { dark: "custom-9".into(), light: "light".into() }).is_err());
        let next = Store::new(dir.path().join("cfg")).theme_settings();
        assert_eq!(next.theme.as_deref(), Some("custom-1"));
        assert_eq!(next.sync, ThemeSync { dark: "custom-1".into(), light: "solarized-light".into() });
        assert_eq!(next.custom.iter().map(|c| (c.id.as_str(), c.name.as_str())).collect::<Vec<_>>(), [("custom-1", "Night 2"), ("custom-2", "Night")]);
        assert_eq!(next.resolve("dark", true).color("bg"), "#101010");
        let saved: serde_json::Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(saved["later"], 1, "other settings are kept");
        store.delete_custom_theme("custom-1").unwrap();
        let s = store.theme_settings();
        assert_eq!((s.theme.as_deref(), s.sync.dark.as_str(), s.custom.len()), (Some("dracula"), "dracula", 1));
        // Theme files: written and read back; a bad or large file is refused with why
        let file = dir.path().join("t.json");
        store.write_theme_file(&file, &s.custom[0]).unwrap();
        assert_eq!(store.read_theme_file(&file).unwrap().name, "Night");
        fs::write(&file, "x".repeat(theme::FILE_MAX + 1)).unwrap();
        assert!(store.read_theme_file(&file).unwrap_err().to_string().contains("at most 16 KB"));
        fs::write(&file, r##"{"qurryTheme": 1, "name": "A", "base": "dark", "colors": {"bg": "#12"}}"##).unwrap();
        assert!(store.read_theme_file(&file).unwrap_err().to_string().contains("#rrggbb"));
    }

    /// Keyword case: Off until picked, saved as "keywordCase" next to the other settings, read back by another app (GUI ↔ TUI);
    /// a value this version doesn't know is Off
    #[test]
    fn keyword_case_setting() {
        use crate::format::KeywordCase;
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path().join("cfg"));
        assert_eq!(store.keyword_case(), KeywordCase::Off);
        store.set_update_channel(crate::update::Channel::Beta).unwrap();
        store.set_keyword_case(KeywordCase::Lower).unwrap();
        let other = Store::new(dir.path().join("cfg"));
        assert_eq!(other.keyword_case(), KeywordCase::Lower);
        assert_eq!(other.update_channel(), crate::update::Channel::Beta);
        other.set_keyword_case(KeywordCase::Upper).unwrap();
        assert_eq!(store.keyword_case(), KeywordCase::Upper);
        let saved: serde_json::Value = serde_json::from_str(&fs::read_to_string(dir.path().join("cfg/settings.json")).unwrap()).unwrap();
        assert_eq!(saved["keywordCase"], "upper");
        fs::write(dir.path().join("cfg/settings.json"), r#"{"keywordCase": "title"}"#).unwrap();
        assert_eq!(store.keyword_case(), KeywordCase::Off);
    }

    /// Grid alignment: defaults until a class is changed, other settings kept, a value this version doesn't know skipped; a change
    /// keeps what another app saved meanwhile, and the default drops the override
    #[test]
    fn grid_align_setting() {
        use crate::format::{Align, Alignments, ValueClass};
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path().join("cfg"));
        assert!(store.grid_align().is_empty());
        store.set_update_channel(crate::update::Channel::Beta).unwrap();
        assert_eq!(store.set_grid_align(ValueClass::Null, Align::Left).unwrap(), Alignments::from([(ValueClass::Null, Align::Left)]));
        assert_eq!(store.grid_align(), Alignments::from([(ValueClass::Null, Align::Left)]));
        assert_eq!(store.update_channel(), crate::update::Channel::Beta);
        // Another app (the TUI) changed Boolean meanwhile: changing Number keeps it
        let other = Store::new(dir.path().join("cfg"));
        other.set_grid_align(ValueClass::Boolean, Align::Center).unwrap();
        let both = Alignments::from([(ValueClass::Null, Align::Left), (ValueClass::Boolean, Align::Center), (ValueClass::Number, Align::Center)]);
        assert_eq!(store.set_grid_align(ValueClass::Number, Align::Center).unwrap(), both);
        assert_eq!(store.set_grid_align(ValueClass::Number, Align::Right).unwrap(), Alignments::from([(ValueClass::Null, Align::Left), (ValueClass::Boolean, Align::Center)]));
        let path = dir.path().join("cfg/settings.json");
        fs::write(&path, r#"{"gridAlign": {"null": "left", "number": "justify"}}"#).unwrap();
        assert_eq!(store.grid_align(), Alignments::from([(ValueClass::Null, Align::Left)]));
    }

    /// Status bar items: the defaults until changed, a change keeps what another app saved meanwhile, all off stays off, Restore
    /// Defaults removes the key (other settings kept)
    #[test]
    fn status_bar_items_setting() {
        use crate::statusbar::{Item, DEFAULTS};
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path().join("cfg"));
        assert_eq!(store.status_bar_items(), DEFAULTS);
        store.set_update_channel(crate::update::Channel::Beta).unwrap();
        assert_eq!(store.set_status_bar_item(Item::Sum, true).unwrap(), [Item::Sum, Item::ShownColumns, Item::Position]);
        // The other app turned Size on meanwhile: turning Position off keeps it
        Store::new(dir.path().join("cfg")).set_status_bar_item(Item::Size, true).unwrap();
        assert_eq!(store.set_status_bar_item(Item::Position, false).unwrap(), [Item::Sum, Item::Size, Item::ShownColumns]);
        let path = dir.path().join("cfg/settings.json");
        assert!(fs::read_to_string(&path).unwrap().contains(r#""statusBarItems": [
    "sum",
    "size",
    "shownColumns"
  ]"#));
        for i in [Item::Sum, Item::Size, Item::ShownColumns] {
            store.set_status_bar_item(i, false).unwrap();
        }
        assert!(store.status_bar_items().is_empty(), "all off stays off");
        assert_eq!(store.reset_status_bar_items().unwrap(), DEFAULTS);
        assert!(!fs::read_to_string(&path).unwrap().contains("statusBarItems"));
        assert_eq!(store.update_channel(), crate::update::Channel::Beta);
    }

    /// Column widths per table: none until set, merged with what another app saved meanwhile, kept in range, other tables and
    /// settings untouched; Reset Widths removes the table's entry
    #[test]
    fn column_widths_setting() {
        use crate::format::ColumnWidths;
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path().join("cfg"));
        let t = |table: &str| TableRef { source: "pg".into(), database: "shop".into(), schema: "public".into(), table: table.into() };
        assert!(store.column_widths(&t("orders")).is_empty());
        store.set_update_channel(crate::update::Channel::Beta).unwrap();
        assert_eq!(store.set_column_widths(&t("orders"), &ColumnWidths::from([("memo".into(), 53)])).unwrap(), ColumnWidths::from([("memo".into(), 53)]));
        store.set_column_widths(&t("users"), &ColumnWidths::from([("name".into(), 9000)])).unwrap();
        // Another app (the TUI) widened order_no meanwhile: setting memo keeps it
        Store::new(dir.path().join("cfg")).set_column_widths(&t("orders"), &ColumnWidths::from([("order_no".into(), 20)])).unwrap();
        let both = ColumnWidths::from([("memo".into(), 28), ("order_no".into(), 20)]);
        assert_eq!(store.set_column_widths(&t("orders"), &ColumnWidths::from([("memo".into(), 28)])).unwrap(), both);
        assert_eq!(store.column_widths(&t("orders")), both);
        // 0 forgets one column's width (a second fit), the others stay
        assert_eq!(store.set_column_widths(&t("orders"), &ColumnWidths::from([("memo".into(), 0)])).unwrap(), ColumnWidths::from([("order_no".into(), 20)]));
        store.set_column_widths(&t("orders"), &ColumnWidths::from([("memo".into(), 28)])).unwrap();
        assert_eq!(store.column_widths(&t("users")), ColumnWidths::from([("name".into(), crate::format::COLUMN_WIDTH_MAX)]));
        let other_db = TableRef { database: "postgres".into(), ..t("orders") };
        assert!(store.column_widths(&other_db).is_empty(), "another database's table of the same name keeps its own widths");
        // A tab naming no database is on the source's default (the GUI's tab on the default keeps none)
        let pg: ConnectionConfig = serde_json::from_value(serde_json::json!({ "id": "pg", "name": "pg", "kind": "postgres", "host": "h", "port": 5432, "user": "u", "database": "shop" })).unwrap();
        store.save(&pg).unwrap();
        assert_eq!(store.column_widths(&TableRef { database: String::new(), ..t("orders") }), both);
        store.reset_column_widths(&t("orders")).unwrap();
        assert!(store.column_widths(&t("orders")).is_empty());
        assert_eq!(store.column_widths(&t("users")).len(), 1);
        assert_eq!(store.update_channel(), crate::update::Channel::Beta);
        // A file an older or broken app wrote: what can't be read is skipped
        fs::write(dir.path().join("cfg/settings.json"), r#"{"columnWidths": [{"source": "pg"}, 3, {"source": "pg", "database": "shop", "schema": "public", "table": "orders", "widths": {"memo": "x", "id": 6}}]}"#).unwrap();
        assert_eq!(store.column_widths(&t("orders")), ColumnWidths::from([("id".into(), 6)]));
        fs::write(dir.path().join("cfg/settings.json"), r#"{"columnWidths": {"not": "a list"}}"#).unwrap();
        assert!(store.column_widths(&t("orders")).is_empty());
        store.set_column_widths(&t("orders"), &ColumnWidths::from([("id".into(), 6)])).unwrap();
        assert_eq!(store.column_widths(&t("orders")).len(), 1);
    }

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
        let changes: [fn(&mut ConnectionConfig); 8] = [|c| c.host = "attacker.example".into(), |c| c.port = 5433, |c| c.user = "root".into(), |c| c.kind = DbKind::Mysql,
            |c| c.ssl.as_mut().unwrap().enabled = false, |c| c.ssh.as_mut().unwrap().host = "evil".into(), |c| c.ssl.as_mut().unwrap().mode = "require".into(),
            |c| c.ssl.as_mut().unwrap().ca_file = "/tmp/evil-ca.pem".into()];
        for f in changes {
            let mut c = a.clone();
            f(&mut c);
            assert!(!a.same_server(&c), "{c:?}");
        }
        // Same destination: allowing plain text (what the TLS error suggests) and stray spaces around the user keep the password
        let mut c = a.clone();
        c.options.allow_plaintext = true;
        c.user = " app ".into();
        c.ssh.as_mut().unwrap().user = "me ".into();
        assert!(a.same_server(&c) && a.same_ssh(&c));
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

    /// The AWS SSM target, profile and region pick where the DB host is resolved: changing them asks for the password again.
    /// Old files (no ssm) load as before, and a source without SSM is written without the key (older apps read the same file)
    #[test]
    fn ssm_target_is_part_of_the_server() {
        let old: ConnectionConfig = serde_json::from_value(serde_json::json!({ "id": "x", "name": "a", "kind": "postgres", "host": "db.internal", "port": 5432, "user": "app" })).unwrap();
        assert_eq!(old.ssm, None);
        assert!(!serde_json::to_string(&old).unwrap().contains("ssm"));
        let a: ConnectionConfig = serde_json::from_value(serde_json::json!({ "id": "x", "name": "a", "kind": "postgres", "host": "db.internal", "port": 5432, "user": "app",
            "ssm": { "enabled": true, "profile": "prod", "region": "ap-northeast-2", "target": "i-0123456789abcdef0" } })).unwrap();
        assert!(a.tunneled() && a.active_ssh().is_none());
        assert!(!old.same_server(&a), "turning SSM on moves the connection");
        let mut b = a.clone();
        b.ssm.as_mut().unwrap().target = " I-0123456789ABCDEF0 ".into();
        assert!(a.same_server(&b));
        let changes: [fn(&mut ConnectionConfig); 4] =
            [|c| c.ssm.as_mut().unwrap().target = "i-0fffffffffffffff0".into(), |c| c.ssm.as_mut().unwrap().profile = "other".into(), |c| c.ssm.as_mut().unwrap().region = "us-east-1".into(), |c| c.ssm.as_mut().unwrap().enabled = false];
        for f in changes {
            let mut c = a.clone();
            f(&mut c);
            assert!(!a.same_server(&c), "{c:?}");
        }
        // An empty instance id is still the SSM choice (connecting asks for it), never a direct connection
        let mut c = a.clone();
        c.ssm.as_mut().unwrap().target = " ".into();
        assert!(c.tunneled() && c.password_key().ends_with("#ssm"));
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
        // Production: never write, even set to write and run with --allow-write
        c.options.mcp = Some("write".into());
        assert_eq!(c.mcp_access(true), McpAccess::Read);
        c.options.confirm_changes = false;
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
            scope: Default::default(), read_only: false, options: Default::default(), ssl: None, redis: None, ssm: None,
        }
    }

    /// Production consoles start in Manual by default (#106), but only where a session can be in Manual: a Redis or driver plugin
    /// console that asked for it would refuse every statement
    #[test]
    fn production_starts_manual_where_manual_works() {
        let mut c = conn("p", "prod");
        c.options.confirm_changes = true;
        for kind in [DbKind::Postgres, DbKind::Mysql] {
            c.kind = kind;
            assert!(c.starts_manual() && c.production_starts_manual(), "{kind:?}");
        }
        for kind in [DbKind::Redis, DbKind::Sqlite, DbKind::Mssql, DbKind::Oracle, DbKind::Clickhouse, DbKind::Duckdb] {
            c.kind = kind;
            c.options.tx_manual = false;
            assert!(!c.starts_manual() && !c.production_starts_manual(), "{kind:?}");
            c.options.tx_manual = true;
            assert!(!c.starts_manual(), "{kind:?}");
        }
        c.kind = DbKind::Postgres;
        assert!(c.starts_manual() && !c.production_starts_manual(), "the source's own Transaction control");
        c.options.tx_manual = false;
        c.options.production_manual = Some(false);
        assert!(!c.starts_manual());
    }

    /// Import a whole file, as the confirm with every row checked
    fn import_all(s: &Store, file: &std::path::Path) -> (usize, usize, Vec<Forget>) {
        let plan = s.import_plan(file, std::path::Path::new("/nonexistent-home"), None).unwrap();
        s.import_apply(&plan, &[], |_| Ok(())).unwrap()
    }

    /// The confirm's rows as text
    fn preview(s: &Store, file: &std::path::Path) -> String {
        let plan = s.import_plan(file, std::path::Path::new("/nonexistent-home"), None).unwrap();
        plan.rows.iter().map(|r| format!("{} {} {}", r.text, r.note, r.value)).collect::<Vec<_>>().join("\n")
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
        assert_eq!(import_all(&b, &file), (1, 1, vec![]));
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
        let p = preview(&b, &file);
        assert!(p.contains("attacker.example") && p.contains("SSL OFF") && p.contains("password will be removed"), "{p}");
        assert_eq!(import_all(&b, &file), (0, 1, vec![("2".to_string(), true, false)]));
        assert!(!b.get("2").unwrap().unwrap().password_saved);
        let _ = std::fs::remove_dir_all(&base);
    }

    /// An imported file turning on "Allow unencrypted" doesn't keep the saved password (someone in the middle could strip TLS and read it)
    #[test]
    fn import_allowing_plaintext_forgets_the_password() {
        let base = std::env::temp_dir().join(format!("dbtool-store-{}", uuid::Uuid::new_v4()));
        let b = Store::new(base.join("b"));
        let mut mine = conn("4", "db");
        mine.kind = DbKind::Mysql;
        mine.password_saved = true;
        b.save(&mine).unwrap();
        let mut theirs = mine.clone();
        theirs.options.allow_plaintext = true;
        let file = base.join("in.json");
        std::fs::write(&file, serde_json::to_string(&vec![theirs]).unwrap()).unwrap();
        let p = preview(&b, &file);
        assert!(p.contains("unencrypted") && p.contains("password will be removed and asked again"), "{p}");
        assert_eq!(import_all(&b, &file), (0, 1, vec![("4".to_string(), true, false)]));
        let got = b.get("4").unwrap().unwrap();
        assert!(got.options.allow_plaintext && !got.password_saved);
        // Already allowed: importing the same setting keeps it
        let mut again = got.clone();
        again.password_saved = true;
        b.save(&again).unwrap();
        assert_eq!(import_all(&b, &file), (0, 1, vec![]));
        assert!(b.get("4").unwrap().unwrap().password_saved);
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
        let p = preview(&b, &file);
        assert!(p.contains("unencrypted") && !p.contains("MCP"), "{p}");
        // An update keeps the saved password: the question doesn't say it's asked again
        let m = b.import_plan(&file, std::path::Path::new("/nonexistent-home"), None).unwrap().message;
        assert!(m.ends_with("0 to add, 1 to update. Passwords aren't in the file: an updated data source keeps its saved password unless its row says otherwise."), "{m}");
        import_all(&b, &file);
        assert_eq!(b.get("3").unwrap().unwrap().mcp_access(true), McpAccess::Off);
        let _ = std::fs::remove_dir_all(&base);
    }

    /// An older app rewriting connections.json without the newer fields doesn't lose MCP Off or "Allow unencrypted"
    #[test]
    fn older_app_rewrite_keeps_access_settings() {
        let base = std::env::temp_dir().join(format!("dbtool-store-{}", uuid::Uuid::new_v4()));
        let s = Store::new(base.clone());
        let mut c = conn("1", "DEV");
        c.options.mcp = Some("off".into());
        c.options.allow_plaintext = true;
        s.save(&c).unwrap();
        s.save(&conn("2", "other")).unwrap();
        let text = std::fs::read_to_string(base.join("connections.json")).unwrap();
        // Also in connections.json (so a missing side file doesn't share Off sources again)
        assert!(text.contains("allowPlaintext") && text.contains("\"mcp\""), "{text}");
        // What an older app writes back: the fields it doesn't know are gone
        let mut old: serde_json::Value = serde_json::from_str(&text).unwrap();
        old[0]["name"] = "renamed by an older app".into();
        for c in old.as_array_mut().unwrap() {
            let o = c["options"].as_object_mut().unwrap();
            o.remove("mcp");
            o.remove("allowPlaintext");
        }
        std::fs::write(base.join("connections.json"), old.to_string()).unwrap();
        let got = s.get("1").unwrap().unwrap();
        assert_eq!(got.name, "renamed by an older app");
        assert_eq!(got.mcp_access(true), McpAccess::Off);
        assert!(got.options.allow_plaintext);
        // Data saved by builds before the side file: connections.json is used when the side file has no entry
        std::fs::remove_file(base.join("connection-access.json")).unwrap();
        old[0]["options"]["mcp"] = "read".into();
        old[0]["options"]["allowPlaintext"] = true.into();
        std::fs::write(base.join("connections.json"), old.to_string()).unwrap();
        let got = s.get("1").unwrap().unwrap();
        assert_eq!((got.options.mcp.as_deref(), got.options.allow_plaintext), (Some("read"), true));
        // Deleting a source removes its entry
        s.save(&got).unwrap();
        s.delete("1").unwrap();
        let side = std::fs::read_to_string(base.join("connection-access.json")).unwrap();
        assert!(!side.contains("\"1\"") && side.contains("\"2\""), "{side}");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(base.join("connection-access.json")).unwrap().permissions().mode() & 0o777, 0o600);
        }
        let _ = std::fs::remove_dir_all(&base);
    }

    /// An older app (no AWS SSM) rewriting connections.json, or its connection-access.json, doesn't lose the SSM tunnel
    /// (that source would then connect straight to its host with the saved password)
    #[test]
    fn older_app_rewrite_keeps_the_ssm_tunnel() {
        let dir = tempfile::tempdir().unwrap();
        let s = Store::new(dir.path().to_path_buf());
        let mut c = conn("1", "PROD");
        let ssm = SsmConfig { enabled: true, profile: "prod".into(), region: "ap-northeast-2".into(), target: "i-0123456789abcdef0".into() };
        c.ssm = Some(ssm.clone());
        s.save(&c).unwrap();
        s.save(&conn("2", "other")).unwrap();
        // What an older app writes back: no ssm in either file it knows
        let mut old: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.path().join("connections.json")).unwrap()).unwrap();
        old[0].as_object_mut().unwrap().remove("ssm");
        old[0]["color"] = "#f00".into();
        std::fs::write(dir.path().join("connections.json"), old.to_string()).unwrap();
        std::fs::write(dir.path().join("connection-access.json"), r#"{"1":{"mcp":"off","allowPlaintext":false}}"#).unwrap();
        let got = s.get("1").unwrap().unwrap();
        assert_eq!(got.color.as_deref(), Some("#f00"));
        assert_eq!(got.ssm.as_ref(), Some(&ssm));
        assert_eq!(got.password_key(), "1#ssm");
        // The next save by this version writes it back to connections.json; turning SSM off stays off
        s.save(&got).unwrap();
        assert!(std::fs::read_to_string(dir.path().join("connections.json")).unwrap().contains("i-0123456789abcdef0"));
        let mut off = got.clone();
        off.ssm.as_mut().unwrap().enabled = false;
        s.save(&off).unwrap();
        assert!(s.get("1").unwrap().unwrap().active_ssm().is_none());
        // A damaged side file is an error, not "no tunnel"
        std::fs::write(dir.path().join("connection-tunnels.json"), "{").unwrap();
        assert!(s.get("1").unwrap_err().to_string().contains("connection-tunnels.json is damaged"));
    }

    /// Without the side file (only connections.json restored or synced), MCP Off and "Allow unencrypted" stay as saved
    #[test]
    fn missing_side_file_keeps_access_settings() {
        let base = std::env::temp_dir().join(format!("dbtool-store-{}", uuid::Uuid::new_v4()));
        let s = Store::new(base.clone());
        let mut c = conn("1", "DEV");
        c.options.mcp = Some("off".into());
        c.options.allow_plaintext = true;
        s.save(&c).unwrap();
        std::fs::remove_file(base.join("connection-access.json")).unwrap();
        let got = s.get("1").unwrap().unwrap();
        assert_eq!(got.mcp_access(true), McpAccess::Off);
        assert!(got.options.allow_plaintext);
        let _ = std::fs::remove_dir_all(&base);
    }

    /// Two apps saving at once: A writes the side file, B writes both files from its older view, A writes connections.json.
    /// A's new source has no side entry, and still stays MCP Off
    #[test]
    fn interleaved_saves_keep_mcp_off() {
        let base = std::env::temp_dir().join(format!("dbtool-store-{}", uuid::Uuid::new_v4()));
        let s = Store::new(base.clone());
        s.save(&conn("old", "shared")).unwrap();
        let b_side = std::fs::read_to_string(base.join("connection-access.json")).unwrap();
        let mut c = conn("new", "hidden");
        c.options.mcp = Some("off".into());
        s.save(&c).unwrap();
        // B's side file (written from its view without the new source) lands after A's
        std::fs::write(base.join("connection-access.json"), b_side).unwrap();
        assert_eq!(s.get("new").unwrap().unwrap().mcp_access(true), McpAccess::Off);
        let _ = std::fs::remove_dir_all(&base);
    }

    /// A failed connections.json write doesn't drop the side entry of the source it still lists
    #[test]
    fn side_entries_are_pruned_only_after_connections_are_written() {
        let base = std::env::temp_dir().join(format!("dbtool-store-{}", uuid::Uuid::new_v4()));
        let s = Store::new(base.clone());
        let mut c = conn("1", "DEV");
        c.options.mcp = Some("off".into());
        s.save(&c).unwrap();
        // write_private's temp file for connections.json can't be created
        let blocker = base.join(format!("connections.tmp-{}", std::process::id()));
        std::fs::create_dir(&blocker).unwrap();
        assert!(s.delete("1").is_err());
        std::fs::remove_dir(&blocker).unwrap();
        let side = std::fs::read_to_string(base.join("connection-access.json")).unwrap();
        assert!(side.contains("\"1\""), "{side}");
        assert_eq!(s.get("1").unwrap().unwrap().mcp_access(true), McpAccess::Off);
        let _ = std::fs::remove_dir_all(&base);
    }

    /// A damaged side file is still an error, and its message says moving it away is safe now
    #[test]
    fn damaged_side_file_is_an_error() {
        let base = std::env::temp_dir().join(format!("dbtool-store-{}", uuid::Uuid::new_v4()));
        let s = Store::new(base.clone());
        let mut c = conn("1", "DEV");
        c.options.mcp = Some("off".into());
        s.save(&c).unwrap();
        std::fs::write(base.join("connection-access.json"), "{ \"1\": ").unwrap();
        let e = s.list().unwrap_err().to_string();
        assert!(e.contains("damaged") && e.contains("connections.json"), "{e}");
        assert!(s.save(&conn("2", "x")).is_err());
        // Following the advice keeps Off
        std::fs::remove_file(base.join("connection-access.json")).unwrap();
        assert_eq!(s.get("1").unwrap().unwrap().mcp_access(true), McpAccess::Off);
        let _ = std::fs::remove_dir_all(&base);
    }

    /// Saves from several apps at once (GUI · TUI · MCP) don't lose each other's sources
    #[test]
    fn concurrent_saves_keep_every_source() {
        let base = std::env::temp_dir().join(format!("dbtool-store-{}", uuid::Uuid::new_v4()));
        std::thread::scope(|sc| {
            for t in 0..4 {
                let base = base.clone();
                sc.spawn(move || {
                    let s = Store::new(base);
                    for i in 0..10 {
                        let mut c = conn(&format!("{t}-{i}"), "x");
                        c.options.mcp = Some("off".into());
                        s.save(&c).unwrap();
                    }
                });
            }
        });
        let all = Store::new(base.clone()).list().unwrap();
        assert_eq!(all.len(), 40);
        assert!(all.iter().all(|c| c.mcp_access(true) == McpAccess::Off));
        let _ = std::fs::remove_dir_all(&base);
    }

    /// Changing one part of a source (scope, color …) from several apps at once keeps every change: read · change · write under the lock
    #[test]
    fn concurrent_modifies_keep_every_change() {
        let base = std::env::temp_dir().join(format!("dbtool-store-{}", uuid::Uuid::new_v4()));
        Store::new(base.clone()).save(&conn("a", "x")).unwrap();
        std::thread::scope(|sc| {
            for t in 0..4 {
                let base = base.clone();
                sc.spawn(move || {
                    let s = Store::new(base);
                    for i in 0..10 {
                        s.modify("a", |c| {
                            c.scope.schemas.insert(format!("{t}-{i}"), vec![]);
                            Ok(())
                        })
                        .unwrap()
                        .unwrap();
                    }
                });
            }
        });
        let s = Store::new(base.clone());
        assert_eq!(s.get("a").unwrap().unwrap().scope.schemas.len(), 40);
        assert_eq!(s.modify("missing", |_| Ok(())).unwrap(), None);
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
