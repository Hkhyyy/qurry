//! 접속 설정 저장소 (JSON 파일). 비밀번호는 secrets.rs 의 암호화 파일에 둔다.
use crate::error::AppResult;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::{fs, path::PathBuf};

/// 단축키 사용자 설정: 액션 id → 키 목록 (기본값과 다른 것만)
pub type Keymap = BTreeMap<String, Vec<String>>;


#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DbKind {
    Postgres,
    Mysql,
    Redis,
    // 아래는 드라이버 플러그인 (필요할 때 받는다): crate::plugin
    Sqlite,
    Mssql,
    Clickhouse,
    Duckdb,
    Mongodb,
    Oracle,
    // 벡터 DB (REST)
    Qdrant,
    Chroma,
    Milvus,
}

impl DbKind {
    /// 드라이버 플러그인 이름 (기본으로 들어 있는 PG·MySQL·Redis 는 None)
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

    /// DB 가 곧 스키마 (트리에 스키마 단계가 없다)
    pub fn flat(self) -> bool {
        matches!(self, DbKind::Mysql | DbKind::Redis | DbKind::Sqlite | DbKind::Clickhouse | DbKind::Mongodb | DbKind::Qdrant | DbKind::Chroma | DbKind::Milvus)
    }

    /// 파일 하나가 DB 인 것 (호스트·포트 대신 파일 경로)
    pub fn is_file(self) -> bool {
        matches!(self, DbKind::Sqlite | DbKind::Duckdb)
    }

    /// 벡터 DB: 콘솔이 SQL 이 아니라 REST (METHOD /path + JSON)
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
    /// 트리에 보일 DB·스키마 (DataGrip Data Source 속성의 Schemas 탭). 비어 있으면 전부.
    #[serde(default)]
    pub scope: Scope,
    /// 키체인에 비밀번호를 저장했는지 (목록 표시용 — 키체인을 읽지 않기 위해)
    #[serde(default)]
    pub password_saved: bool,
    #[serde(default)]
    pub ssh_secret_saved: bool,
    /// SSH 터널 (DataGrip의 SSH/SSL 탭)
    #[serde(default)]
    pub ssh: Option<SshConfig>,
    /// DataGrip처럼 접속을 색으로 구분 (예: "#5c2b2b"). 없으면 기본색.
    #[serde(default)]
    pub color: Option<String>,
    /// DataGrip Options 탭의 Read-only: 세션을 읽기 전용 트랜잭션으로 연다 (서버가 쓰기를 거부)
    #[serde(default)]
    pub read_only: bool,
    /// DataGrip Options 탭의 나머지 Connection 설정
    #[serde(default)]
    pub options: ConnOptions,
    /// SSH/SSL 탭의 SSL (PostgreSQL·MySQL·Redis 공통)
    #[serde(default)]
    pub ssl: Option<SslConfig>,
    /// Redis 연결 방식 (Standalone / Sentinel / Cluster)
    #[serde(default)]
    pub redis: Option<RedisMode>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct SslConfig {
    #[serde(default)]
    pub enabled: bool,
    /// require (암호화만, 인증서 검증 안 함) | verify-ca | verify-full (호스트 이름까지)
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
    /// Sentinel 이 감시하는 마스터 이름
    #[serde(default)]
    pub sentinel_master: String,
    /// sentinel: Sentinel 노드들 / cluster: 추가 시드 노드들 ("host:port", 쉼표·공백 구분)
    #[serde(default)]
    pub nodes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ConnOptions {
    /// 새 콘솔의 기본 Tx 모드 (Transaction control: Manual)
    #[serde(default)]
    pub tx_manual: bool,
    /// 기본 격리 수준: "read uncommitted" | "read committed" | "repeatable read" | "serializable" (None = 서버 기본)
    #[serde(default)]
    pub isolation: Option<String>,
    /// Switch schema: "auto" | "manual" | "disable" (프론트에서 사용)
    #[serde(default)]
    pub switch_schema: Option<String>,
    /// 세션 TimeZone (None = 서버 기본)
    #[serde(default)]
    pub time_zone: Option<String>,
    /// Run keep-alive query each N seconds
    #[serde(default)]
    pub keep_alive_secs: Option<u32>,
    /// Auto-disconnect after N seconds (유휴)
    #[serde(default)]
    pub auto_disconnect_secs: Option<u32>,
    /// Single database mode: 기본 DB만 보이고 쓴다
    #[serde(default)]
    pub single_db: bool,
    /// Startup script: 연결마다 접속 직후 실행
    #[serde(default)]
    pub startup_script: String,
    /// Production: 데이터를 바꾸는 문장은 바뀔 행 수를 보여 주고 확인을 받는다
    #[serde(default)]
    pub confirm_changes: bool,
    /// MCP 서버에 보이는지: "off" | "read" | "write" (None = 정하기 전에 만든 소스, ConnectionConfig::mcp_access)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mcp: Option<String>,
    /// Oracle: Instant Client 폴더 (비면 ORACLE_CLIENT_LIB_DIR · 시스템 기본)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oracle_client: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Scope {
    /// None = 모든 DB
    #[serde(default)]
    pub databases: Option<Vec<String>>,
    /// DB 이름 → 보일 스키마 (없으면 전부)
    #[serde(default)]
    pub schemas: std::collections::HashMap<String, Vec<String>>,
}

pub const SCOPE_ALL: &str = "*";
pub const SCOPE_DEFAULT: &str = "@default";

/// 트리에 보일 DB·스키마 규칙 (GUI src/lib/api.ts 의 dbVisible / schemaVisible 와 같다)
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

    /// 저장된 SSH 비밀을 보내도 되는 같은 SSH 서버인지
    pub fn same_ssh(&self, other: &ConnectionConfig) -> bool {
        let t = |c: &ConnectionConfig| c.ssh.as_ref().map(|s| (s.host.trim().to_lowercase(), s.port, s.user.clone()));
        t(self) == t(other)
    }

    /// self(저장된 설정) 의 비밀번호를 other 로 보내도 되는지: 같은 종류 · 서버 · 사용자 · SSH 터널이고, TLS 를 끄지 않았다.
    /// 호스트만 바꿔 Test 를 누르거나 같은 id 로 다른 서버를 가져와도 예전 비밀번호가 새 서버로 가지 않게
    pub fn same_server(&self, other: &ConnectionConfig) -> bool {
        let t = |c: &ConnectionConfig| (c.kind, c.host.trim().to_lowercase(), c.port, c.user.clone(), c.active_ssh().map(|s| (s.host.trim().to_lowercase(), s.port)));
        t(self) == t(other) && !(self.active_ssl().is_some() && other.active_ssl().is_none())
    }
}

/// SSH 비밀번호/키 passphrase 의 저장 키
/// MCP 서버가 이 Data Source 로 할 수 있는 것
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum McpAccess {
    /// 목록에도 없고 접속·비밀번호도 쓰지 않는다
    Off,
    Read,
    /// --allow-write 로 띄웠을 때 execute 도
    Write,
}

impl ConnectionConfig {
    /// MCP 노출. 정하지 않은 예전 소스는 Production 이면 숨기고, 아니면 예전처럼 (읽기, --allow-write 면 쓰기).
    /// Read-only Data Source 는 쓰기가 없다
    pub fn mcp_access(&self, allow_write: bool) -> McpAccess {
        let a = match self.options.mcp.as_deref() {
            Some("off") => McpAccess::Off,
            Some("read") => McpAccess::Read,
            Some("write") => McpAccess::Write,
            _ if self.options.confirm_changes => McpAccess::Off,
            _ => McpAccess::Write,
        };
        if a == McpAccess::Write && (!allow_write || self.read_only) { McpAccess::Read } else { a }
    }
}

pub fn ssh_secret_key(id: &str) -> String {
    format!("{id}#ssh")
}

/// Export 파일 형식. 비밀번호는 포함하지 않는다.
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

    /// 같은 id는 덮어쓰고 새 id는 추가한다. 반환값: (추가, 갱신)
    fn read_import(path: &std::path::Path) -> AppResult<Vec<ConnectionConfig>> {
        let text = fs::read_to_string(path)?;
        Ok(match serde_json::from_str::<ExportFile>(&text) {
            Ok(f) => f.connections,
            Err(_) => serde_json::from_str(&text)?,
        })
    }

    /// 가져오기 전에 무엇이 바뀌는지 (남이 준 파일이 Read-only 를 끄거나 접속 시 실행할 SQL 을 넣을 수 있다)
    pub fn import_preview(&self, path: &std::path::Path) -> AppResult<Vec<String>> {
        let all = self.list()?;
        let mut out = vec![];
        for c in Self::read_import(path)? {
            let script = |c: &ConnectionConfig| c.options.startup_script.trim().to_string();
            match all.iter().find(|e| !c.id.is_empty() && e.id == c.id) {
                None => {
                    let mut line = format!("+ add \"{}\" ({}@{}:{})", c.name, c.user, c.host, c.port);
                    if !script(&c).is_empty() {
                        line += " — runs a startup script on connect";
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
                    if e.options.confirm_changes && !c.options.confirm_changes {
                        ch.push("turns Production OFF".into());
                    }
                    if e.mcp_access(true) < c.mcp_access(true) {
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

    /// (추가, 변경, 비밀번호를 지울 소스: (id, DB 비밀번호, SSH 비밀)). 저장 여부 플래그는 파일이 아니라 지금 값을 따른다
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
                    let (pw, ssh) = (e.password_saved && !e.same_server(&c), e.ssh_secret_saved && !e.same_ssh(&c));
                    c.password_saved = e.password_saved && !pw;
                    c.ssh_secret_saved = e.ssh_secret_saved && !ssh;
                    if pw || ssh {
                        forget.push((c.id.clone(), pw, ssh));
                    }
                    *e = c;
                    updated += 1;
                }
                None => {
                    // 새 소스에는 저장된 비밀번호가 없다
                    c.password_saved = false;
                    c.ssh_secret_saved = false;
                    all.push(c);
                    added += 1;
                }
            }
        }
        self.write(&all)?;
        Ok((added, updated, forget))
    }

    /// 설정 폴더 (connections.json 이 있는 곳)
    pub fn dir(&self) -> PathBuf {
        self.path.parent().map(|p| p.to_path_buf()).unwrap_or_default()
    }

    fn keymap_path(&self) -> PathBuf {
        self.path.with_file_name("keymap.json")
    }

    /// keymap.json 의 한 구역 ("gui" / "tui"). 파일이 없으면 빈 설정
    pub fn keymap(&self, section: &str) -> AppResult<Keymap> {
        let path = self.keymap_path();
        if !path.exists() {
            return Ok(Keymap::new());
        }
        let mut all: BTreeMap<String, Keymap> = serde_json::from_str(&fs::read_to_string(path)?)?;
        Ok(all.remove(section).unwrap_or_default())
    }

    /// 한 구역만 바꾸고 다른 구역(GUI ↔ TUI)은 그대로 둔다
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

    /// 열어 둔 탭·화면 분할 (DataGrip 처럼 다시 켜면 그대로). client = "gui" / "tui", 파일을 따로 둔다 (동시에 써도 서로 덮지 않게).
    /// 콘솔 SQL 이 들어가므로 0600. 내용은 클라이언트가 정한다
    /// 설정 폴더 안의 파일
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
        // 호스트·사용자·SSH 키 경로가 들어 있다 → 본인만 읽게
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
        let changes: [fn(&mut ConnectionConfig); 6] = [|c| c.host = "attacker.example".into(), |c| c.port = 5433, |c| c.user = "root".into(), |c| c.kind = DbKind::Mysql,
            |c| c.ssl.as_mut().unwrap().enabled = false, |c| c.ssh.as_mut().unwrap().host = "evil".into()];
        for f in changes {
            let mut c = a.clone();
            f(&mut c);
            assert!(!a.same_server(&c), "{c:?}");
        }
        let mut c = a.clone();
        c.ssh.as_mut().unwrap().user = "other".into();
        assert!(!a.same_ssh(&c));
    }

    #[test]
    fn mcp_access_defaults() {
        let mut c: ConnectionConfig = serde_json::from_value(serde_json::json!({ "name": "a", "kind": "postgres", "host": "h", "port": 1, "user": "u" })).unwrap();
        // 예전 소스: 그대로 (읽기, --allow-write 면 쓰기)
        assert_eq!(c.mcp_access(false), McpAccess::Read);
        assert_eq!(c.mcp_access(true), McpAccess::Write);
        // Production 은 정해 주기 전에는 숨긴다
        c.options.confirm_changes = true;
        assert_eq!(c.mcp_access(true), McpAccess::Off);
        c.options.mcp = Some("write".into());
        assert_eq!(c.mcp_access(true), McpAccess::Write);
        assert_eq!(c.mcp_access(false), McpAccess::Read);
        c.read_only = true;
        assert_eq!(c.mcp_access(true), McpAccess::Read);
        c.options.mcp = Some("off".into());
        assert_eq!(c.mcp_access(true), McpAccess::Off);
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
        let names: Vec<_> = b.list().unwrap().into_iter().map(|c| c.name).collect();
        assert_eq!(names, vec!["PROD", "DEV"]);
        assert_eq!(b.get("1").unwrap().unwrap().color.as_deref(), Some("#5c2b2b"));
        let _ = std::fs::remove_dir_all(&base);
    }

    /// 같은 id 로 다른 서버를 가져오면 저장된 비밀번호를 쓰지 않는다 (지울 목록 · 미리보기 경고)
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
