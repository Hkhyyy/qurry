use super::*;

impl AppState {
    /// Opens a temporary data source (`qurry-tui postgres://…`) for this process: it is never saved, so no other app (MCP included) sees it
    pub fn open_temporary(&self, t: crate::dsn::Temporary) -> ConnectionConfig {
        let mut config = t.config;
        config.id = format!("tmp-{}", uuid::Uuid::new_v4());
        config.password_saved = t.password.is_some();
        config.ssh_secret_saved = false;
        if let Some(p) = t.password {
            self.temporary.lock().unwrap().insert(config.password_key(), p);
        }
        self.store.add_temporary(config.clone());
        config
    }

    /// Saved password or SSH secret for a key, read from the secrets file each time (nothing is cached in memory).
    pub fn secret(&self, key: &str) -> AppResult<Option<String>> {
        if self.store.is_temporary(store::secret_owner(key)) {
            return Ok(self.temporary.lock().unwrap().get(key).cloned());
        }
        // MCP server: never reads passwords of Data Sources hidden from MCP (so it cannot connect either)
        if self.mcp {
            let id = store::secret_owner(key);
            if let Some(c) = self.store.get(id)?.filter(|c| c.mcp_access(false) == store::McpAccess::Off) {
                return Err(AppError::Msg(format!("Data source \"{}\" is not shared with MCP (turn it on in the data source's Options)", c.name)));
            }
        }
        // No in-memory cache: another app (the GUI) may have changed the server and its password since; the file is cheap to read
        // Passwords live only in the encrypted file (the keychain holds at most the master key: crate::secrets)
        self.secret_file.get(key)
    }

    /// Where the password file's master key is kept (shown in Settings)
    pub fn secret_key_location(&self) -> crate::secrets::KeyLocation {
        crate::secrets::key_location(&self.store.dir())
    }

    /// Move the master key to the OS keychain / to a file. The key is the same, so the open store stays in use
    pub fn set_secret_key_location(&self, keychain: bool) -> AppResult<crate::secrets::KeyLocation> {
        let to = if keychain { crate::secrets::KeyLocation::Keychain } else { crate::secrets::KeyLocation::File };
        self.secret_file.ensure_key()?;
        crate::secrets::move_key(&self.store.dir(), to, &crate::secrets::OsKeychain)?;
        Ok(self.secret_key_location())
    }

    /// The secret only for a source that has one (flag, or an entry in the file): sources without a password still connect when secrets.json is damaged
    pub(crate) fn saved_secret(&self, key: &str, saved: bool) -> AppResult<Option<String>> {
        if saved || self.secret_file.contains(key)? { self.secret(key) } else { Ok(None) }
    }

    pub fn set_secret(&self, key: &str, value: Option<&str>) -> AppResult<()> {
        if self.store.is_temporary(store::secret_owner(key)) {
            let mut m = self.temporary.lock().unwrap();
            match value {
                Some(v) => m.insert(key.to_string(), v.to_string()),
                None => m.remove(key),
            };
            return Ok(());
        }
        self.secret_file.set(key, value)?;
        Ok(())
    }

    /// Forgets DB passwords saved under `<id>#ssm` whose data source is gone: an older app that deletes an AWS SSM source removes
    /// only `<id>` and `<id>#ssh`. Only `#ssm` entries: older apps never write them, so a source an older app is saving right now
    /// can't lose its password. Under the store lock, so a new source this version is saving (its `#ssm` set, connections.json
    /// not written yet) isn't taken for a deleted one. Nothing is removed while connections.json can't be read
    pub fn forget_orphan_ssm_passwords(&self) -> AppResult<()> {
        self.store.locked(|all| {
            let ids: std::collections::HashSet<String> = all.into_iter().map(|c| c.id).collect();
            for key in self.secret_file.names()? {
                if key.ends_with("#ssm") && !ids.contains(store::secret_owner(&key)) {
                    self.secret_file.set(&key, None)?;
                }
            }
            Ok(())
        })
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionView {
    #[serde(flatten)]
    pub config: ConnectionConfig,
    pub has_password: bool,
    pub connected: bool,
    /// From the qurry-tui command line, only in this process (Store::temporary)
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub temporary: bool,
}

/// DBs shown in the tree (scope from the Data Source › Schemas tab, system DBs hidden). Shared by GUI, TUI and the search cache
pub fn visible_dbs(cfg: &ConnectionConfig, dbs: Vec<String>, def: &str) -> Vec<String> {
    let hidden = qurry_shared::kind::system_databases(cfg.kind);
    let scoped = cfg.scope.databases.as_ref().map(|d| !d.is_empty()).unwrap_or(false);
    dbs.into_iter()
        .filter(|d| scoped || d == def || !hidden.contains(&d.as_str()))
        .filter(|d| !cfg.options.single_db || d == def)
        .filter(|d| cfg.options.single_db || cfg.scope.db_visible(d, def))
        .collect()
}

const SERVER_CHANGED: &str = "The server, port, user, type, SSH / AWS SSM tunnel or Redis nodes changed, or SSL was turned off, its verification lowered or its CA file changed — enter the password again. The saved one is only sent to the server it was saved for.";

const SSH_CHANGED: &str = "The SSH server changed — enter the SSH password / passphrase again. The saved one is only sent to the server it was saved for.";

#[derive(Serialize)]
pub struct ImportResult {
    pub added: usize,
    pub updated: usize,
    /// For the status bar (crate::import::ImportPlan::done_message)
    pub message: String,
}

impl AppState {
    pub async fn list_connections(&self) -> AppResult<Vec<ConnectionView>> {
        crate::log!("[qurry] list_connections");
        let pools = self.pools.read().await;
        self
            .store
            .list()?
            .into_iter()
            .map(|c| {
                Ok(ConnectionView {
                    has_password: c.password_saved || self.secret_file.contains(&c.password_key()).unwrap_or(true),
                    connected: pools.keys().any(|k| belongs(k, &c.id)),
                    temporary: self.store.is_temporary(&c.id),
                    config: c,
                })
            })
            .collect()
    }

    /// `password`: None keeps the existing password, an empty string deletes it.
    pub async fn save_connection(&self, mut config: ConnectionConfig, password: Option<String>, ssh_secret: Option<String>) -> AppResult<ConnectionConfig> {
        crate::log!("[qurry] save_connection {}", if config.id.is_empty() { "(new)" } else { &config.id });
        if config.id.is_empty() {
            config.id = uuid::Uuid::new_v4().to_string();
            // New sources (including duplicates) default to MCP Off unless set
            if config.options.mcp.is_none() {
                config.options.mcp = Some("off".into());
            }
        }
        // Read the key now: in keychain mode that may show a prompt, and other apps would wait on the store lock meanwhile
        if [&password, &ssh_secret].iter().any(|s| s.as_deref().is_some_and(|s| !s.is_empty())) {
            self.secret_file.ensure_key()?;
        }
        // Check against the saved source, set the secrets and write under the store lock: another app's save in between
        // (another server, a password saved or removed) can't be decided on stale settings
        let config = self.store.save_with(config, |prev, config| {
            // If the server changed, do not use the saved password for the new server: ask for it again
            // "Saved" means the password actually exists, not the flag (old imports left entries with only the flag turned off).
            // A damaged secrets.json that may hold it counts as saved
            if let Some(p) = prev {
                if password.is_none() && self.secret_file.contains(&p.password_key()).unwrap_or(true) && !p.same_server(config) {
                    return Err(AppError::Msg(SERVER_CHANGED.into()));
                }
                if ssh_secret.is_none() && self.secret_file.contains(&store::ssh_secret_key(&p.id)).unwrap_or(true) && config.active_ssh().is_some() && !p.same_ssh(config) {
                    return Err(AppError::Msg(SSH_CHANGED.into()));
                }
            }
            // The saved-password flag is managed on the backend (None = keep the existing value)
            config.password_saved = prev.is_some_and(|p| p.password_saved);
            config.ssh_secret_saved = prev.is_some_and(|p| p.ssh_secret_saved);
            match password.as_deref() {
                Some("") => {
                    for k in store::password_keys(&config.id) {
                        self.set_secret(&k, None)?;
                    }
                    config.password_saved = false;
                }
                Some(p) => {
                    // Only under the key for this setup (switching to or from AWS SSM moves it)
                    let key = config.password_key();
                    for k in store::password_keys(&config.id).iter().filter(|k| **k != key) {
                        self.set_secret(k, None)?;
                    }
                    self.set_secret(&key, Some(p))?;
                    config.password_saved = true;
                }
                None => {}
            }
            let sk = store::ssh_secret_key(&config.id);
            match ssh_secret.as_deref() {
                Some("") => {
                    self.set_secret(&sk, None)?;
                    config.ssh_secret_saved = false;
                }
                Some(p) => {
                    self.set_secret(&sk, Some(p))?;
                    config.ssh_secret_saved = true;
                }
                None => {}
            }
            Ok(())
        })?;
        drop_pool_logged(self, &config.id, crate::audit::DISCONNECTED).await; // The settings may have changed, so force a reconnect
        Ok(config)
    }

    /// Explorer menu: change only the color (another app's change to the source since the list was read stays; no reconnect)
    pub async fn set_connection_color(&self, id: String, color: Option<String>) -> AppResult<ConnectionConfig> {
        self.modify_connection(&id, |c| {
            c.color = color;
            Ok(())
        })
    }

    /// Explorer menu: change only the shown databases and schemas
    pub async fn set_connection_scope(&self, id: String, scope: store::Scope) -> AppResult<ConnectionConfig> {
        self.modify_connection(&id, |c| {
            c.scope = scope;
            Ok(())
        })
    }

    /// The answer to a NEEDS_TLS / DROPPED_TLS connect error: turn on "Allow unencrypted connection" for this source and reconnect.
    /// The saved password stays (same server, and the user chose this right at the error). Decided on the source as saved now, under the store lock
    pub async fn allow_plaintext(&self, id: String) -> AppResult<ConnectionConfig> {
        let c = self.modify_connection(&id, |c| {
            if !matches!(c.kind, store::DbKind::Postgres | store::DbKind::Mysql) {
                return Err(AppError::Msg("\"Allow unencrypted connection\" is only for PostgreSQL and MySQL data sources.".into()));
            }
            if c.options.confirm_changes {
                return Err(AppError::Msg(format!("\"{}\" is a Production data source, so Qurry won't turn off encryption for it from here. If the server really has no TLS and the network is trusted, turn on \"Allow unencrypted connection\" yourself (GUI: data source Options tab · TUI: data source form, \"Allow unencrypted\").", c.name)));
            }
            if c.active_ssl().is_some() {
                return Err(AppError::Msg(format!("\"{}\" has SSL turned on, so it always requires TLS. Check its SSL settings (GUI: SSH/SSL tab · TUI: data source form), or turn SSL off there first if the server has no TLS.", c.name)));
            }
            c.options.allow_plaintext = true;
            Ok(())
        })?;
        drop_pool(self, &id).await; // The next connect uses the new setting
        Ok(c)
    }

    fn modify_connection(&self, id: &str, f: impl FnOnce(&mut ConnectionConfig) -> AppResult<()>) -> AppResult<ConnectionConfig> {
        self.store
            .modify(id, |c| {
                f(c)?;
                Ok(c.clone())
            })?
            .ok_or_else(|| AppError::Msg("Data source not found".into()))
    }

    pub async fn delete_connection(&self, id: String) -> AppResult<()> {
        drop_pool_logged(self, &id, crate::audit::DISCONNECTED).await;
        // A damaged secrets.json (left untouched) doesn't keep the data source: it is removed, and the error says the password stayed
        let forgot = store::password_keys(&id).iter().chain([&store::ssh_secret_key(&id)]).try_for_each(|k| self.set_secret(k, None));
        crate::search_cache::remove_conn(&self.store.dir(), &id);
        self.store.delete(&id)?;
        self.connecting.lock().unwrap().remove(&id);
        forgot.map_err(|e| AppError::Msg(format!("The data source was removed, but its saved password couldn't be: {e}")))
    }

    /// Whether a password or SSH secret may be stored for this key (a temporary source's lives in memory, never in the secrets file)
    fn holds_secret(&self, id: &str, key: &str) -> bool {
        if self.store.is_temporary(id) {
            return self.temporary.lock().unwrap().contains_key(key);
        }
        self.secret_file.contains(key).unwrap_or(true)
    }

    /// The saved password, if it may be sent to config (only when it is the same server as the saved settings)
    fn saved_password_for(&self, config: &ConnectionConfig) -> AppResult<Option<String>> {
        match self.store.get(&config.id)? {
            Some(p) if self.holds_secret(&p.id, &p.password_key()) && !p.same_server(config) => Err(AppError::Msg(SERVER_CHANGED.into())),
            Some(p) => self.saved_secret(&p.password_key(), p.password_saved),
            None => Ok(None),
        }
    }

    fn saved_ssh_secret_for(&self, config: &ConnectionConfig) -> AppResult<Option<String>> {
        match self.store.get(&config.id)? {
            Some(p) if self.holds_secret(&p.id, &store::ssh_secret_key(&p.id)) && !p.same_ssh(config) => Err(AppError::Msg(SSH_CHANGED.into())),
            Some(p) => self.saved_secret(&store::ssh_secret_key(&config.id), p.ssh_secret_saved),
            None => Ok(None),
        }
    }

    /// Connect with a form's settings that aren't saved (Test Connection, the Schemas tab of a new data source): no pool or tunnel is
    /// kept, the caller closes both. If `password` is None, use the saved password (only when it is the same server as the saved settings).
    async fn connect_form(&self, config: &ConnectionConfig, password: Option<String>, ssh_secret: Option<String>) -> AppResult<(Pool, Option<crate::tunnel::Tunnel>)> {
        let password = match password {
            Some(p) => Some(p),
            None if !config.id.is_empty() => self.saved_password_for(config)?,
            None => None,
        };
        let ssh_secret = match ssh_secret {
            Some(p) => Some(p),
            None if !config.id.is_empty() && config.active_ssh().is_some() => self.saved_ssh_secret_for(config)?,
            None => None,
        };
        connect_config(config, password.as_deref(), ssh_secret.as_deref()).await
    }

    /// Test the connection without saving. If `password` is None, use the saved password (only when it is the same server as the saved settings).
    pub async fn test_connection(&self, config: ConnectionConfig, password: Option<String>, ssh_secret: Option<String>) -> AppResult<String> {
        let (pool, tunnel) = self.connect_form(&config, password, ssh_secret).await?;
        let version = pool.server_version().await;
        let plain = dispatch!(pool, b => b.plaintext_note(&config).await);
        pool.close().await;
        drop(tunnel);
        let via = match (config.active_ssh(), config.active_ssm()) {
            (Some(s), _) => format!("  (via SSH {}@{})", s.user, s.host),
            (_, Some(s)) => format!("  (via AWS SSM {})", s.target.trim()),
            _ => String::new(),
        };
        version.map(|v| {
            let notes: Vec<String> = db::version_note(config.kind, &v).into_iter().chain(plain).collect();
            if notes.is_empty() { format!("{v}{via}") } else { format!("{v}{via}\n{}", notes.join("\n")) }
        })
    }

    /// Connect to list the databases or schemas of a form's data source (form_databases): read-only, so a SQLite / DuckDB file that
    /// isn't there fails ("unable to open database file") instead of being created. An empty path is an in-memory database, which
    /// can't open read-only
    async fn connect_form_to_list(&self, mut config: ConnectionConfig, password: Option<String>, ssh_secret: Option<String>) -> AppResult<(Pool, Option<crate::tunnel::Tunnel>)> {
        config.read_only = !(config.kind.is_file() && config.database.trim().is_empty());
        self.connect_form(&config, password, ssh_secret).await
    }

    /// The Schemas tab of a data source that isn't saved yet: its databases, read with the form's settings (as Test Connection connects)
    pub async fn form_databases(&self, config: ConnectionConfig, password: Option<String>, ssh_secret: Option<String>) -> AppResult<Vec<String>> {
        let (pool, tunnel) = self.connect_form_to_list(config, password, ssh_secret).await?;
        let r = crate::meta::list_databases(&pool).await;
        pool.close().await;
        drop(tunnel);
        r
    }

    /// The schemas of one database of a data source that isn't saved yet (form_databases)
    pub async fn form_schemas(&self, mut config: ConnectionConfig, password: Option<String>, ssh_secret: Option<String>, database: String) -> AppResult<Vec<crate::meta::SchemaInfo>> {
        // For file DBs (SQLite, DuckDB) database is the file path (open_pool)
        if !config.kind.is_file() {
            config.database = database;
        }
        let (pool, tunnel) = self.connect_form_to_list(config, password, ssh_secret).await?;
        let r = crate::meta::list_schemas(&pool).await;
        pool.close().await;
        drop(tunnel);
        r
    }

    /// Test Connection on the SSH/SSL tab: checks only the SSH login.
    pub async fn test_ssh(&self, config: ConnectionConfig, ssh_secret: Option<String>) -> AppResult<String> {
        let ssh = config.ssh.as_ref().filter(|s| !s.host.is_empty()).ok_or_else(|| AppError::Msg("Enter the SSH host".into()))?;
        let secret = match ssh_secret {
            Some(p) => Some(p),
            None if !config.id.is_empty() => self.saved_ssh_secret_for(&config)?,
            None => None,
        };
        crate::tunnel::test_login(ssh, secret.as_deref()).await
    }

    pub async fn connect(&self, id: String) -> AppResult<String> {
        crate::log!("[qurry] connect {id}");
        let started = std::time::Instant::now();
        let r = with_pool(self, &id, |p| async move { p.server_version().await.map(|v| (p, v)) }).await;
        log_connect("connect", &id, started, &r);
        let (pool, v) = r?;
        // If connected in plain text, say so next to the version (status bar · TUI status)
        let cfg = self.source(&id)?;
        let note = dispatch!(pool, b => b.plaintext_note(&cfg).await);
        Ok(match note {
            Some(n) => format!("{v} — {n}"),
            None => v,
        })
    }

    pub async fn disconnect(&self, id: String) -> AppResult<()> {
        drop_pool_logged(self, &id, crate::audit::DISCONNECTED).await;
        Ok(())
    }

    pub async fn export_connections(&self, path: String) -> AppResult<usize> {
        self.store.export_to(std::path::Path::new(&path))
    }

    /// The DataGrip / IntelliJ and DBeaver locations on this computer (`home`: the user's home folder)
    pub fn import_locations(&self, home: &std::path::Path) -> Vec<crate::import::Location> {
        crate::import::locations(home)
    }

    /// Import preview of a picked file or folder: Data Sources to be added, changed or skipped, and risky changes
    /// (Read-only turned off, server/SSH/startup script changed)
    pub fn import_plan(&self, path: &std::path::Path, home: &std::path::Path) -> AppResult<store::ImportPlan> {
        self.store.import_plan(path, home, None)
    }

    /// Import preview of a found location: only one `import_locations` lists (so a caller can't name any path), named by its label
    pub fn import_location(&self, path: &str, home: &std::path::Path) -> AppResult<store::ImportPlan> {
        let found = crate::import::locations(home).into_iter().find(|l| l.path == path);
        let l = found.ok_or_else(|| AppError::Msg(format!("Can't import: {} isn't there anymore (moved or deleted).", crate::import::shown(std::path::Path::new(path), home))))?;
        self.store.import_plan(std::path::Path::new(&l.path), home, Some(&l.label))
    }

    /// The user confirms and trusts an unknown SSH server key (added to known_hosts)
    pub fn trust_ssh_host(&self, host: String, port: u16) -> AppResult<String> {
        crate::tunnel::trust_host_key(&host, port)
    }

    /// Import the checked rows of a plan (`picked[i]` for row i)
    pub async fn import_apply(&self, plan: &store::ImportPlan, picked: &[bool]) -> AppResult<ImportResult> {
        // Importing a different server under the same id deletes the old password (so it is not sent to the new server) —
        // before the store is written, so a failure (damaged secrets.json) imports nothing
        let (added, updated, forget) = self.store.import_apply(plan, picked, |forget| {
            for (id, password, ssh) in forget {
                if *password {
                    for k in store::password_keys(id) {
                        self.set_secret(&k, None)?;
                    }
                }
                if *ssh {
                    self.set_secret(&store::ssh_secret_key(id), None)?;
                }
            }
            Ok(())
        })?;
        for (id, _, _) in forget {
            drop_pool_logged(self, &id, crate::audit::DISCONNECTED).await;
        }
        Ok(ImportResult { added, updated, message: crate::import::ImportPlan::done_message(added, updated) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tests::*;

    /// A temporary source (qurry-tui URL) is listed and connectable in this process only: never in connections.json or the secrets
    /// file, so another app on the same folder (GUI, MCP) never sees it. Its form can't save it; read-only is enforced as for saved ones
    #[tokio::test]
    async fn temporary_sources_stay_in_this_process() {
        let dir = tempfile::tempdir().unwrap();
        let st = crate::open_state(Some(dir.path().to_path_buf())).unwrap();
        let saved = st.save_connection(silent_source("", 1, None), Some("savedpw".into()), None).await.unwrap();
        let var = |k: &str| (k == "USER").then(|| "me".to_string());
        let t = crate::dsn::from_url("postgres://u:s3cret-pw@db.example/app", true, &crate::dsn::Env { var: &var, home: None }).unwrap();
        let c = st.open_temporary(t);
        assert!(c.id.starts_with("tmp-") && c.password_saved);
        let views = st.list_connections().await.unwrap();
        assert!(views.iter().any(|v| v.config.id == c.id && v.temporary && v.has_password));
        assert!(views.iter().any(|v| v.config.id == saved.id && !v.temporary));
        assert_eq!(st.secret(&c.id).unwrap().as_deref(), Some("s3cret-pw"));
        // Changes to it stay in memory; saving its form is refused
        st.set_connection_color(c.id.clone(), Some("#5c2b2b".into())).await.unwrap();
        assert_eq!(st.store.get(&c.id).unwrap().unwrap().color.as_deref(), Some("#5c2b2b"));
        let e = st.save_connection(c.clone(), None, None).await.unwrap_err().to_string();
        assert!(e.contains("temporary"), "{e}");
        // Its form tested with another host never sends its password there
        let mut moved = c.clone();
        moved.host = "evil.example".into();
        assert_eq!(st.saved_password_for(&moved).unwrap_err().to_string(), SERVER_CHANGED);
        assert_eq!(st.saved_password_for(&c).unwrap().as_deref(), Some("s3cret-pw"));
        // Saving another source writes connections.json without it
        st.save_connection(silent_source("", 2, None), None, None).await.unwrap();
        let file = std::fs::read_to_string(dir.path().join("connections.json")).unwrap();
        assert!(!file.contains(&c.id) && !file.contains("db.example"), "{file}");
        assert!(!st.secret_file.names().unwrap().iter().any(|k| k.contains("tmp-")));
        let other = crate::open_state(Some(dir.path().to_path_buf())).unwrap();
        assert!(other.store.get(&c.id).unwrap().is_none());
        assert_eq!(other.list_connections().await.unwrap().len(), 2);
        // Read-only
        let e = st.check_read_only(&c.id, "delete from t").unwrap_err().to_string();
        assert!(e.contains("read-only"), "{e}");
        assert!(st.check_read_only(&c.id, "select 1").is_ok());
        // Deleting it forgets it and its password, and leaves the saved ones alone
        st.delete_connection(c.id.clone()).await.unwrap();
        assert!(st.store.get(&c.id).unwrap().is_none());
        assert_eq!(st.secret(&c.id).unwrap(), None);
        assert_eq!(st.secret(&saved.id).unwrap().as_deref(), Some("savedpw"));
    }

    /// Changing only the host and doing Test / Save does not send the saved password to the new server; it asks for it again (before connecting)
    #[tokio::test]
    async fn saved_password_is_not_sent_to_a_changed_server() {
        let dir = std::env::temp_dir().join(format!("qurry-pw-{}", uuid::Uuid::new_v4()));
        let st = AppState::new(Store::new(dir.clone()), crate::secrets::SecretFile::open(&dir).unwrap());
        let cfg = ConnectionConfig {
            id: "a".into(), name: "a".into(), kind: crate::store::DbKind::Postgres, host: "127.0.0.1".into(), port: 1,
            user: "u".into(), database: "d".into(), color: None, ssh: None, password_saved: false,
            ssh_secret_saved: false, scope: Default::default(), read_only: false, options: Default::default(), ssl: None, redis: None, ssm: None,
        };
        let saved = st.save_connection(cfg, Some("pw".into()), None).await.unwrap();
        let mut moved = saved.clone();
        moved.host = "attacker.example".into();
        let e = st.test_connection(moved.clone(), None, None).await.unwrap_err().to_string();
        assert!(e.contains("enter the password again"), "{e}");
        let e = st.save_connection(moved.clone(), None, None).await.unwrap_err().to_string();
        assert!(e.contains("enter the password again"), "{e}");
        // Keep the saved password even if only the flag is off (old imports)
        let mut flag_off = saved.clone();
        flag_off.password_saved = false;
        st.store.save(&flag_off).unwrap();
        let e = st.test_connection(moved.clone(), None, None).await.unwrap_err().to_string();
        assert!(e.contains("enter the password again"), "{e}");
        // Giving a new password saves it · renaming only leaves it as is
        st.save_connection(moved, Some("pw2".into()), None).await.unwrap();
        let mut renamed = st.store.get("a").unwrap().unwrap();
        renamed.name = "b".into();
        st.save_connection(renamed, None, None).await.unwrap();
        assert_eq!(st.secret("a").unwrap().as_deref(), Some("pw2"));
        let _ = std::fs::remove_dir_all(dir);
    }

    /// New or duplicated sources are MCP Off. The MCP server does not connect to hidden sources (even without a password)
    #[tokio::test]
    async fn new_sources_start_hidden_from_mcp() {
        let dir = std::env::temp_dir().join(format!("qurry-mcpnew-{}", uuid::Uuid::new_v4()));
        let mut st = AppState::new(Store::new(dir.clone()), crate::secrets::SecretFile::open(&dir).unwrap());
        let cfg = ConnectionConfig {
            id: String::new(), name: "trust".into(), kind: crate::store::DbKind::Postgres, host: "127.0.0.1".into(), port: 1,
            user: "u".into(), database: "d".into(), color: None, ssh: None, password_saved: false,
            ssh_secret_saved: false, scope: Default::default(), read_only: false, options: Default::default(), ssl: None, redis: None, ssm: None,
        };
        let saved = st.save_connection(cfg, None, None).await.unwrap();
        assert_eq!(saved.options.mcp.as_deref(), Some("off"));
        st.mcp = true;
        let e = st.connect(saved.id.clone()).await.unwrap_err().to_string();
        assert!(e.contains("not shared with MCP"), "{e}");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// An SSM source's DB password is saved under `<id>#ssm`, which older apps (that would connect straight to the host) don't read.
    /// Switching the tunnel moves it; deleting removes both; MCP locks it like the others
    #[tokio::test]
    async fn ssm_password_is_kept_from_older_apps() {
        let dir = tempfile::tempdir().unwrap();
        let mut st = AppState::new(Store::new(dir.path().to_path_buf()), crate::secrets::SecretFile::open(dir.path()).unwrap());
        let mut cfg: ConnectionConfig = serde_json::from_value(serde_json::json!({ "name": "prod", "kind": "postgres", "host": "localhost", "port": 5432, "user": "app",
            "ssm": { "enabled": true, "profile": "prod", "region": "ap-northeast-2", "target": "i-0123456789abcdef0" } })).unwrap();
        cfg = st.save_connection(cfg, Some("pw".into()), None).await.unwrap();
        let id = cfg.id.clone();
        assert!(!st.secret_file.contains(&id).unwrap(), "an older app finds no password under the id");
        assert_eq!(st.secret(&format!("{id}#ssm")).unwrap().as_deref(), Some("pw"));
        assert!(st.list_connections().await.unwrap()[0].has_password);
        assert_eq!(st.saved_password_for(&cfg).unwrap().as_deref(), Some("pw"));
        // Turning SSM off is another server: the password is asked again, and then saved under the id only
        cfg.ssm.as_mut().unwrap().enabled = false;
        assert!(st.save_connection(cfg.clone(), None, None).await.is_err());
        cfg = st.save_connection(cfg, Some("pw2".into()), None).await.unwrap();
        assert_eq!(st.secret(&id).unwrap().as_deref(), Some("pw2"));
        assert!(!st.secret_file.contains(&format!("{id}#ssm")).unwrap());
        cfg.ssm.as_mut().unwrap().enabled = true;
        cfg = st.save_connection(cfg, Some("pw3".into()), None).await.unwrap();
        assert!(!st.secret_file.contains(&id).unwrap());
        st.mcp = true;
        assert!(st.secret(&format!("{id}#ssm")).unwrap_err().to_string().contains("not shared with MCP"));
        st.mcp = false;
        st.delete_connection(cfg.id).await.unwrap();
        assert!(!st.secret_file.contains(&format!("{id}#ssm")).unwrap());
    }

    /// An older app deleting an SSM source leaves `<id>#ssm` behind (it removes `<id>` and `<id>#ssh` only): the next start forgets it
    #[tokio::test]
    async fn ssm_password_of_a_source_deleted_by_an_older_app_is_forgotten() {
        let dir = tempfile::tempdir().unwrap();
        let st = crate::open_state(Some(dir.path().to_path_buf())).unwrap();
        let src = |name: &str| -> ConnectionConfig {
            serde_json::from_value(serde_json::json!({ "name": name, "kind": "postgres", "host": "localhost", "port": 5432, "user": "app",
                "ssm": { "enabled": true, "target": "i-0123456789abcdef0" } })).unwrap()
        };
        let gone = st.save_connection(src("gone"), Some("pw".into()), None).await.unwrap().id;
        let kept = st.save_connection(src("kept"), Some("pw".into()), None).await.unwrap().id;
        st.set_secret("api-key", Some("x")).unwrap();
        // What the older app does
        st.store.delete(&gone).unwrap();
        st.set_secret(&gone, None).unwrap();
        st.set_secret(&format!("{gone}#ssh"), None).unwrap();
        drop(st);
        let st = crate::open_state(Some(dir.path().to_path_buf())).unwrap();
        assert!(!st.secret_file.contains(&format!("{gone}#ssm")).unwrap());
        assert_eq!(st.secret(&format!("{kept}#ssm")).unwrap().as_deref(), Some("pw"));
        assert!(st.secret_file.contains("api-key").unwrap());
        // connections.json can't be read: nothing is removed
        std::fs::write(dir.path().join("connections.json"), "{damaged").unwrap();
        drop(st);
        let st = crate::open_state(Some(dir.path().to_path_buf())).unwrap();
        assert!(st.secret_file.contains(&format!("{kept}#ssm")).unwrap());
    }

    /// Another app starting while this one saves a new SSM source (its `#ssm` password set, connections.json not written yet)
    /// waits for the save instead of taking the new source for a deleted one
    #[test]
    fn starting_during_a_save_keeps_the_new_ssm_password() {
        let dir = tempfile::tempdir().unwrap();
        let st = crate::open_state(Some(dir.path().to_path_buf())).unwrap();
        let cfg: ConnectionConfig = serde_json::from_value(serde_json::json!({ "id": "new", "name": "new", "kind": "postgres", "host": "localhost", "port": 5432, "user": "app",
            "ssm": { "enabled": true, "target": "i-0123456789abcdef0" } })).unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::scope(|s| {
            s.spawn(|| {
                st.store
                    .save_with(cfg, |_, c| {
                        st.set_secret(&c.password_key(), Some("pw"))?;
                        tx.send(()).unwrap();
                        std::thread::sleep(std::time::Duration::from_millis(300));
                        Ok(())
                    })
                    .unwrap();
            });
            rx.recv().unwrap();
            drop(crate::open_state(Some(dir.path().to_path_buf())).unwrap());
        });
        assert_eq!(st.secret("new#ssm").unwrap().as_deref(), Some("pw"));
    }

    /// The MCP server does not read passwords of Data Sources hidden from MCP. The app is unaffected
    #[test]
    fn mcp_hidden_source_secrets_stay_locked() {
        let dir = std::env::temp_dir().join(format!("qurry-mcp-{}", uuid::Uuid::new_v4()));
        let mut st = AppState::new(Store::new(dir.clone()), crate::secrets::SecretFile::open(&dir).unwrap());
        let mut cfg = ConnectionConfig {
            id: "prod".into(), name: "prod".into(), kind: crate::store::DbKind::Postgres, host: "db".into(), port: 5432,
            user: "u".into(), database: "d".into(), color: None, ssh: None, password_saved: true,
            ssh_secret_saved: false, scope: Default::default(), read_only: false, options: Default::default(), ssl: None, redis: None, ssm: None,
        };
        cfg.options.confirm_changes = true;
        st.store.save(&cfg).unwrap();
        st.set_secret("prod", Some("pw")).unwrap();
        st.set_secret("prod#ssh", Some("sshpw")).unwrap();
        assert_eq!(st.secret("prod").unwrap().as_deref(), Some("pw"));
        st.mcp = true;
        assert!(st.secret("prod").is_err() && st.secret("prod#ssh").is_err());
        cfg.options.mcp = Some("read".into());
        st.store.save(&cfg).unwrap();
        assert_eq!(st.secret("prod").unwrap().as_deref(), Some("pw"));
        let _ = std::fs::remove_dir_all(dir);
    }

    /// A damaged secrets.json only blocks sources that have a saved password; deleting still removes the source (the file is left as is)
    #[tokio::test]
    async fn damaged_secrets_only_block_sources_with_a_password() {
        let dir = std::env::temp_dir().join(format!("qurry-dmg-{}", uuid::Uuid::new_v4()));
        let st = AppState::new(Store::new(dir.clone()), crate::secrets::SecretFile::open(&dir).unwrap());
        let mk = |id: &str| ConnectionConfig {
            id: id.into(), name: id.into(), kind: crate::store::DbKind::Postgres, host: "db".into(), port: 5432,
            user: "u".into(), database: "d".into(), color: None, password_saved: false,
            ssh: Some(crate::store::SshConfig { enabled: true, host: "127.0.0.1".into(), port: 1, user: "me".into(), auth: Default::default(), key_path: String::new() }),
            ssh_secret_saved: false, scope: Default::default(), read_only: false, options: Default::default(), ssl: None, redis: None, ssm: None,
        };
        st.save_connection(mk("pw"), Some("secret".into()), None).await.unwrap();
        st.save_connection(mk("trust"), None, None).await.unwrap();
        // A password in the file whose flag was cleared (old imports did that)
        st.save_connection(mk("flagless"), Some("secret".into()), None).await.unwrap();
        st.store.save(&mk("flagless")).unwrap();
        let damaged = "{ \"flagless\": \"x\", \"pw\": \"tru";
        std::fs::write(dir.join("secrets.json"), damaged).unwrap();
        let e = st.connect("pw".into()).await.unwrap_err().to_string();
        assert!(e.contains("damaged"), "{e}");
        let e = st.connect("flagless".into()).await.unwrap_err().to_string();
        assert!(e.contains("damaged"), "{e}");
        // No saved password or SSH secret: tries the SSH server (nothing listens on port 1), not the secrets file
        let e = st.connect("trust".into()).await.unwrap_err().to_string();
        assert!(!e.contains("damaged"), "{e}");
        let e = st.test_connection(mk("trust"), None, None).await.unwrap_err().to_string();
        assert!(!e.contains("damaged"), "{e}");
        let e = st.delete_connection("pw".into()).await.unwrap_err().to_string();
        assert!(e.contains("was removed") && e.contains("damaged"), "{e}");
        assert!(st.store.get("pw").unwrap().is_none());
        assert_eq!(std::fs::read_to_string(dir.join("secrets.json")).unwrap(), damaged);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The explorer's color and scope menus change only that part: another app's change since the list was read stays
    #[tokio::test]
    async fn color_and_scope_change_only_their_part() {
        let dir = std::env::temp_dir().join(format!("qurry-part-{}", uuid::Uuid::new_v4()));
        let st = AppState::new(Store::new(dir.clone()), crate::secrets::SecretFile::open(&dir).unwrap());
        let cfg = ConnectionConfig {
            id: "p".into(), name: "p".into(), kind: crate::store::DbKind::Postgres, host: "db".into(), port: 5432,
            user: "u".into(), database: "d".into(), color: None, ssh: None, password_saved: false,
            ssh_secret_saved: false, scope: Default::default(), read_only: false, options: Default::default(), ssl: None, redis: None, ssm: None,
        };
        st.save_connection(cfg, Some("pw".into()), None).await.unwrap();
        // Another app turns on Read-only
        let other = Store::new(dir.clone());
        let mut c = other.get("p").unwrap().unwrap();
        c.read_only = true;
        other.save(&c).unwrap();
        st.set_connection_color("p".into(), Some("#f00".into())).await.unwrap();
        let scope = crate::store::Scope { databases: Some(vec!["d".into()]), schemas: Default::default() };
        st.set_connection_scope("p".into(), scope).await.unwrap();
        let c = st.store.get("p").unwrap().unwrap();
        assert!(c.read_only && c.password_saved, "{c:?}");
        assert_eq!((c.color.as_deref(), c.scope.databases.as_deref()), (Some("#f00"), Some(&["d".to_string()][..])));
        assert!(st.set_connection_color("missing".into(), None).await.is_err());
        let _ = std::fs::remove_dir_all(dir);
    }

    /// "Allow unencrypted" from the TLS error: keeps the saved password and another app's change, writes the side file, reconnects;
    /// Production, SSL-configured and non-SQL sources are refused (nothing written)
    #[tokio::test]
    async fn allow_plaintext_keeps_the_password_and_reconnects() {
        let dir = std::env::temp_dir().join(format!("qurry-plain-{}", uuid::Uuid::new_v4()));
        let st = AppState::new(Store::new(dir.clone()), crate::secrets::SecretFile::open(&dir).unwrap());
        let mk = |id: &str, kind| ConnectionConfig {
            id: id.into(), name: id.into(), kind, host: "db.example".into(), port: 5432,
            user: "u".into(), database: "d".into(), color: None, ssh: None, password_saved: false,
            ssh_secret_saved: false, scope: Default::default(), read_only: false, options: Default::default(), ssl: None, redis: None, ssm: None,
        };
        st.save_connection(mk("p", crate::store::DbKind::Postgres), Some("pw".into()), None).await.unwrap();
        // A pool that is open now (lazy: never connects)
        let lazy = sqlx::postgres::PgPoolOptions::new().connect_lazy_with(sqlx::postgres::PgConnectOptions::new().host("db.example"));
        st.pools.write().await.insert("p".into(), Pool::Pg(lazy));
        // Another app turns on Read-only in between
        let other = Store::new(dir.clone());
        let mut c = other.get("p").unwrap().unwrap();
        c.read_only = true;
        other.save(&c).unwrap();
        let got = st.allow_plaintext("p".into()).await.unwrap();
        assert!(got.options.allow_plaintext && got.password_saved && got.read_only, "{got:?}");
        let saved = st.store.get("p").unwrap().unwrap();
        assert!(saved.options.allow_plaintext && saved.password_saved && saved.read_only, "{saved:?}");
        assert_eq!(st.secret("p").unwrap().as_deref(), Some("pw"));
        let side: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.join("connection-access.json")).unwrap()).unwrap();
        assert_eq!(side["p"]["allowPlaintext"], true);
        assert!(st.pools.read().await.is_empty(), "the pool was dropped");

        let mut prod = mk("prod", crate::store::DbKind::Postgres);
        prod.options.confirm_changes = true;
        let mut ssl = mk("ssl", crate::store::DbKind::Mysql);
        ssl.ssl = Some(serde_json::from_value(serde_json::json!({ "enabled": true, "mode": "require" })).unwrap());
        for (c, want) in [(prod, "Production"), (ssl, "SSL turned on"), (mk("r", crate::store::DbKind::Redis), "only for PostgreSQL and MySQL")] {
            let id = c.id.clone();
            st.save_connection(c, Some("pw".into()), None).await.unwrap();
            let e = st.allow_plaintext(id.clone()).await.unwrap_err().to_string();
            assert!(e.contains(want), "{e}");
            let c = st.store.get(&id).unwrap().unwrap();
            assert!(!c.options.allow_plaintext && c.password_saved, "{c:?}");
        }
        assert!(st.allow_plaintext("missing".into()).await.is_err());
        let _ = std::fs::remove_dir_all(dir);
    }

    /// A form save (password kept) racing with another app saving or removing the password keeps the "password saved" flag
    /// matching the file: the saved source is read under the store lock, not before
    #[test]
    fn form_save_racing_a_password_save_keeps_the_flag() {
        for round in 0..4 {
            form_save_race(round % 2 == 0);
        }
    }

    fn form_save_race(end_with_password: bool) {
        let dir = std::env::temp_dir().join(format!("qurry-race-{}", uuid::Uuid::new_v4()));
        let cfg = ConnectionConfig {
            id: "r".into(), name: "r".into(), kind: crate::store::DbKind::Postgres, host: "db".into(), port: 5432,
            user: "u".into(), database: "d".into(), color: None, ssh: None, password_saved: false,
            ssh_secret_saved: false, scope: Default::default(), read_only: false, options: Default::default(), ssl: None, redis: None, ssm: None,
        };
        Store::new(dir.clone()).save(&cfg).unwrap();
        // Create the key before the threads open the file
        crate::secrets::SecretFile::open(&dir).unwrap();
        let run = |pw: bool| {
            let (dir, cfg) = (dir.clone(), cfg.clone());
            move || {
                let st = AppState::new(Store::new(dir.clone()), crate::secrets::SecretFile::open(&dir).unwrap());
                let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
                // The password side sets and removes it in turn, ending on `end_with_password`
                let n = if pw { 20 + usize::from(end_with_password) } else { 40 };
                for i in 0..n {
                    let mut c = cfg.clone();
                    c.name = format!("r{i}");
                    let p = pw.then(|| if i % 2 == 0 { format!("pw{i}") } else { String::new() });
                    rt.block_on(st.save_connection(c, p, None)).unwrap();
                }
            }
        };
        std::thread::scope(|sc| {
            sc.spawn(run(false));
            sc.spawn(run(true));
        });
        let st = AppState::new(Store::new(dir.clone()), crate::secrets::SecretFile::open(&dir).unwrap());
        assert_eq!(st.store.get("r").unwrap().unwrap().password_saved, st.secret_file.contains("r").unwrap());
        assert_eq!(st.secret_file.contains("r").unwrap(), end_with_password);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// An import that turns on "Allow unencrypted" while secrets.json is damaged fails as a whole: the store isn't changed,
    /// so after the file is repaired the old password isn't sent on the plaintext path
    #[tokio::test]
    async fn import_needing_a_password_removed_fails_before_writing() {
        let dir = std::env::temp_dir().join(format!("qurry-impdmg-{}", uuid::Uuid::new_v4()));
        let st = AppState::new(Store::new(dir.clone()), crate::secrets::SecretFile::open(&dir).unwrap());
        let cfg = ConnectionConfig {
            id: "m".into(), name: "m".into(), kind: crate::store::DbKind::Mysql, host: "db.example".into(), port: 3306,
            user: "u".into(), database: "d".into(), color: None, ssh: None, password_saved: false,
            ssh_secret_saved: false, scope: Default::default(), read_only: false, options: Default::default(), ssl: None, redis: None, ssm: None,
        };
        st.save_connection(cfg, Some("pw".into()), None).await.unwrap();
        let file = dir.join("import.json");
        st.export_connections(file.display().to_string()).await.unwrap();
        let mut v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
        v["connections"][0]["options"]["allowPlaintext"] = true.into();
        std::fs::write(&file, v.to_string()).unwrap();
        let good = std::fs::read(dir.join("secrets.json")).unwrap();
        std::fs::write(dir.join("secrets.json"), b"{ \"m\": \"tru").unwrap();
        let plan = st.import_plan(&file, &dir).unwrap();
        let e = st.import_apply(&plan, &[]).await.err().expect("import must fail").to_string();
        assert!(e.contains("damaged"), "{e}");
        let c = st.store.get("m").unwrap().unwrap();
        assert!(!c.options.allow_plaintext && c.password_saved, "{c:?}");
        // Repaired: the import goes through and the password is gone
        std::fs::write(dir.join("secrets.json"), good).unwrap();
        st.import_apply(&plan, &[]).await.unwrap();
        let c = st.store.get("m").unwrap().unwrap();
        assert!(c.options.allow_plaintext && !c.password_saved);
        assert_eq!(st.saved_secret("m", c.password_saved).unwrap(), None);
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The Schemas tab of a new data source lists with the form's settings, connecting as Test Connection does: the same errors
    /// (here both tunnels turned on, refused before connecting), and nothing kept (no pool, no tunnel)
    #[tokio::test]
    async fn form_listing_connects_like_test_connection() {
        let dir = tempfile::tempdir().unwrap();
        let st = crate::open_state(Some(dir.path().to_path_buf())).unwrap();
        let mut c: ConnectionConfig = serde_json::from_value(serde_json::json!({ "name": "a", "kind": "postgres", "host": "db.internal", "port": 5432, "user": "u",
            "ssh": { "enabled": true, "host": "bastion", "user": "me" }, "ssm": { "enabled": true, "profile": "", "region": "", "target": "i-1" } })).unwrap();
        c.id = String::new();
        let test = st.test_connection(c.clone(), Some("pw".into()), None).await.unwrap_err().to_string();
        assert!(test.contains("choose one"), "{test}");
        assert_eq!(st.form_databases(c.clone(), Some("pw".into()), None).await.unwrap_err().to_string(), test);
        assert_eq!(st.form_schemas(c, Some("pw".into()), None, "other".into()).await.err().unwrap().to_string(), test);
        assert!(st.pools.read().await.is_empty() && st.tunnels.read().await.is_empty() && st.store.list().unwrap().is_empty());
    }

    /// Real servers: the databases and a database's schemas of a data source that isn't saved, and no pool kept.
    /// QURRY_TEST_PG=127.0.0.1:55433:postgres:postgres QURRY_TEST_MY=127.0.0.1:13306:root:dbtool:shop cargo test -p qurry-core form_listing -- --ignored
    async fn form_listing(kind: crate::store::DbKind) {
        let Some(crate::testenv::Server { mut cfg, password }) = crate::testenv::server(kind) else { return };
        cfg.id = String::new();
        let dir = tempfile::tempdir().unwrap();
        let st = crate::open_state(Some(dir.path().to_path_buf())).unwrap();
        let dbs = st.form_databases(cfg.clone(), password.clone(), None).await.unwrap();
        assert!(dbs.contains(&cfg.database), "{dbs:?}");
        let sch = st.form_schemas(cfg.clone(), password, None, cfg.database.clone()).await.unwrap();
        assert!(!sch.is_empty());
        assert!(st.pools.read().await.is_empty() && st.store.list().unwrap().is_empty(), "nothing kept or saved");
    }

    /// Listing a new SQLite data source never creates its file: a path that isn't there fails, an existing file lists.
    /// cargo build --manifest-path drivers/sqlite/Cargo.toml --target-dir target && cargo test -p qurry-core form_listing_never_creates -- --ignored
    #[tokio::test]
    #[ignore]
    async fn form_listing_never_creates_a_file_db() {
        // As the plugin conformance tests: skipped where the driver isn't built (CI's compat job runs ignored tests without it)
        if crate::plugin::driver_path("sqlite").is_none() {
            eprintln!("skip: build the SQLite driver first: cargo build --manifest-path drivers/sqlite/Cargo.toml --target-dir target");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let st = crate::open_state(Some(dir.path().to_path_buf())).unwrap();
        let path = dir.path().join("missing.db");
        let c: ConnectionConfig = serde_json::from_value(serde_json::json!({ "name": "f", "kind": "sqlite", "host": "", "port": 0, "user": "", "database": &path })).unwrap();
        let e = st.form_databases(c.clone(), None, None).await.unwrap_err().to_string();
        assert!(e.contains("unable to open database file"), "{e}");
        let e = st.form_schemas(c.clone(), None, None, "main".into()).await.err().unwrap().to_string();
        assert!(e.contains("unable to open database file"), "{e}");
        assert!(!path.exists(), "the file was created");
        // An empty file is an empty SQLite database
        std::fs::write(&path, b"").unwrap();
        assert!(!st.form_databases(c, None, None).await.unwrap().is_empty());
    }

    #[tokio::test]
    #[ignore]
    async fn form_listing_pg() {
        form_listing(crate::store::DbKind::Postgres).await;
    }

    #[tokio::test]
    #[ignore]
    async fn form_listing_mysql() {
        form_listing(crate::store::DbKind::Mysql).await;
    }
}
