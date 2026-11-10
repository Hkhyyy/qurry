use super::*;

/// MCP exposure shown in the form: the access core actually grants (an unknown value is off), since the form saves what it shows
fn mcp_label(c: &ConnectionConfig) -> &'static str {
    c.mcp_access(true).as_str()
}

impl App {
    // ───────── TLS opt-in ─────────

    /// Keys pressed this soon after the opt-in popup opened, or after the last key, were meant for what was on screen before it: they are ignored
    pub(crate) const TLS_ASK_GRACE: std::time::Duration = std::time::Duration::from_millis(400);

    /// A connect error that needs TLS (qurry_shared::errors::tls_opt_in): ask to allow unencrypted for this data source, then `then` (expand
    /// again, or reconnect). Never for Production (the core refuses too): the status line says where the setting is instead
    pub(crate) fn offer_tls_opt_in(&mut self, conn: &str, err: &str, then: impl FnOnce(&mut App) + Send + 'static) {
        if !qurry_shared::errors::tls_opt_in(err) {
            return;
        }
        let Some(c) = self.conn(conn).map(|c| c.config.clone()) else { return };
        // Short enough for the status line next to the key hints on a 120-column terminal
        if c.options.confirm_changes {
            self.status = format!("\"{}\" is Production: turn on \"Allow unencrypted\" yourself in its form (e in the explorer)", c.name);
            return;
        }
        // This comes back from a background load: never replace a popup the user is reading (a y meant for it would allow plaintext)
        if self.popup.is_some() {
            self.status = format!("\"{}\": the server has no TLS — expand it again (← →) or run again to allow unencrypted", c.name);
            return;
        }
        self.popup = Some(Popup::AllowPlaintext {
            opened: std::time::Instant::now(),
            // It can open over another tab or source: name the one y changes
            text: format!("Allow unencrypted for \"{}\"?\n\n⚠ The password will be sent unencrypted to {}. Use only on a network you trust.\nTo undo, turn off \"Allow unencrypted\" in the data source form (e in the explorer).", c.name, c.host),
            on_yes: Box::new(move |a| {
                a.status = format!("Allowing unencrypted for \"{}\"…", c.name);
                let st = a.state.clone();
                a.spawn(async move { st.allow_plaintext(c.id).await.map_err(|e| e.to_string()) }, move |a, r| match r {
                    Ok(cfg) => {
                        a.status = format!("Allowed unencrypted for \"{}\" (to undo, turn off \"Allow unencrypted\" in its data source form)", cfg.name);
                        // The form edits this copy: keep it in step with the store
                        if let Some(v) = a.conns.iter_mut().find(|v| v.config.id == cfg.id) {
                            v.config = cfg;
                        }
                        then(a);
                    }
                    Err(e) => a.status = e,
                });
            }),
        });
    }

    /// After the TLS opt-in under a run error: only reconnect. A console never runs the statement again (it may write); a table tab reloads
    /// once connected, as after any failed connection (reload_conns)
    pub(super) fn reconnect_after_tls(&mut self, conn: String, console: Option<u64>) {
        let st = self.state.clone();
        self.status = format!("Connecting to \"{}\"…", self.conn(&conn).map(|c| c.config.name.as_str()).unwrap_or(&conn));
        let id = conn.clone();
        self.spawn(async move { st.connect(id).await.map_err(|e| e.to_string()) }, move |a, r| {
            let msg = match &r {
                Ok(_) if console.is_some() => "Connected. Run again.".to_string(),
                Ok(_) => "Connected".to_string(),
                Err(e) => format!("Connection failed: {e}"),
            };
            if let Some(TabKind::Console(c)) = console.and_then(|id| a.tab_mut(id)).map(|t| &mut t.kind) {
                c.log(r.is_err(), msg.clone());
            }
            a.status = msg;
            a.reload_conns();
            // A tree that still shows the earlier error under the now connected source loads again (as the GUI does)
            if r.is_ok() && a.tree.failed(&conn) {
                a.reexpand(&conn);
            }
        });
    }

    // ───────── Connections ─────────

    pub fn connect(&mut self, conn: &str) {
        if let Some(n) = self.tree.roots.iter().find(|n| n.conn == conn).cloned() {
            if !self.tree.expanded.contains(&n.key) {
                self.toggle_node(&n);
            }
        }
    }

    pub fn disconnect(&mut self, conn: &str) {
        let n = self.pending_tabs(Some(conn)).len();
        if n > 0 {
            let c = conn.to_string();
            self.popup = Some(Popup::Confirm {
                opened: None,
                text: format!("{n} console(s) on this data source have uncommitted changes. Roll them back and disconnect?"),
                on_yes: Box::new(move |a| a.disconnect_now(&c)),
            });
            return;
        }
        self.disconnect_now(conn);
    }

    pub fn disconnect_now(&mut self, conn: &str) {
        let st = self.state.clone();
        let c = conn.to_string();
        self.spawn(async move { st.disconnect(c).await.map_err(err) }, {
            let conn = conn.to_string();
            move |a, r| {
                a.forget_completion(&conn);
                a.tree.forget(&conn);
                a.tree.expanded.retain(|k| !(k == &conn || k.starts_with(&format!("{conn}|"))));
                a.tree.clamp();
                a.status = match r {
                    Ok(()) => "Disconnected".into(),
                    Err(e) => e,
                };
                a.reload_conns();
            }
        });
    }

    /// After a structure change in the console: re-read the expanded nodes under that database (expansion kept)
    pub fn schema_changed(&mut self, conn: &str, db: &str) {
        self.forget_completion(conn);
        let Some(dbn) = self.tree.children.values().flatten().find(|n| matches!(n.kind, NodeKind::Db) && n.conn == conn && n.db.as_deref() == Some(db)).cloned() else { return };
        let under = |k: &str| k == dbn.key || k.starts_with(&format!("{}|", dbn.key));
        let reload: Vec<Node> = std::iter::once(&dbn)
            .chain(self.tree.children.values().flatten())
            .filter(|n| under(&n.key) && self.tree.expanded.contains(&n.key))
            .cloned()
            .collect();
        self.tree.forget(&dbn.key);
        let mut seen = std::collections::HashSet::new();
        for n in reload {
            if seen.insert(n.key.clone()) {
                self.load_children(n);
            }
        }
    }

    pub fn refresh_node(&mut self, n: &Node) {
        let key = n.key.clone();
        // The schema may have changed, so re-read autocomplete metadata too
        self.forget_completion(&n.conn.clone());
        self.tree.forget(&key);
        if self.tree.expanded.contains(&key) {
            self.load_children(n.clone());
        }
        self.reload_conns();
    }

    pub fn conn_form(&mut self, edit: Option<ConnectionConfig>) {
        let c = edit.clone().unwrap_or(ConnectionConfig {
            id: String::new(),
            name: String::new(),
            kind: DbKind::Postgres,
            host: "localhost".into(),
            port: 5432,
            user: String::new(),
            database: String::new(),
            scope: Default::default(),
            password_saved: false,
            ssh_secret_saved: false,
            ssh: None,
            color: None,
            read_only: false,
            options: Default::default(), ssl: None, redis: None, ssm: None,
        });
        let ssh = c.ssh.clone().unwrap_or(SshConfig { enabled: false, host: String::new(), port: 22, user: String::new(), auth: SshAuth::Key, key_path: "~/.ssh/id_ed25519".into() });
        let ssm = c.ssm.clone().unwrap_or_default();
        let tunnel = if ssm.enabled { "aws ssm" } else if ssh.enabled { "ssh" } else { "none" };
        let kind = serde_json::to_value(c.kind).ok().and_then(|v| v.as_str().map(String::from)).unwrap_or_else(|| "postgres".into());
        let f = |label, value: String, kind| Field { label, value, kind };
        let ch = |xs: &[&str]| FieldKind::Choice(xs.iter().map(|x| x.to_string()).collect());
        let saved = |b: bool| if b { "<saved>".to_string() } else { String::new() };
        let color = crate::ui::color_name(c.color.as_deref());
        let mut colors: Vec<&'static str> = std::iter::once("none").chain(crate::ui::COLOR_PRESETS.iter().map(|p| p.0)).collect();
        if color == "custom" {
            colors.push("custom");
        }
        self.popup = Some(Popup::ConnForm(ConnForm {
            fields: vec![
                f("Name", c.name.clone(), FieldKind::Text),
                f("Color", color.into(), ch(&colors)),
                // Workspaces where this Data Source is shown (moved on save if changed)
                f("Workspace", self.ws_label(edit.as_ref().map(|e| self.ws_of_conn(&e.id)).unwrap_or(self.cur_ws)), FieldKind::Choice((0..self.workspaces.len()).map(|i| self.ws_label(i)).collect())),
                f("Type", kind, ch(&["postgres", "mysql", "redis", "sqlite", "duckdb", "mssql", "clickhouse", "mongodb", "oracle", "qdrant", "chroma", "milvus"])),
                f("Host", c.host.clone(), FieldKind::Text),
                f("Port", c.port.to_string(), FieldKind::Text),
                f("User", c.user.clone(), FieldKind::Text),
                f("Password", saved(c.password_saved), FieldKind::Secret),
                f("Database", c.database.clone(), FieldKind::Text),
                f("Read-only", if c.read_only { "on" } else { "off" }.into(), FieldKind::Toggle),
                f("Production", if c.options.confirm_changes { "on" } else { "off" }.into(), FieldKind::Toggle),
                // Production: new consoles start in Manual transaction mode (#106; unset = on)
                f("Production Manual", if c.options.production_manual != Some(false) { "on" } else { "off" }.into(), FieldKind::Toggle),
                // MySQL: whether plaintext is allowed for old remote servers that can't do TLS
                f("Allow unencrypted", if c.options.allow_plaintext { "on" } else { "off" }.into(), FieldKind::Toggle),
                // Whether it's visible to qurry-mcp (AI tools). New sources start off
                f("MCP", if edit.is_some() { mcp_label(&c) } else { "off" }.into(), ch(&["off", "read", "write"])),
                // Production: AI tools may read its Database Admin reports (MCP dba_report, values masked); ConnForm::disabled elsewhere
                f("Admin reports", if c.options.mcp_dba_reports { "on" } else { "off" }.into(), FieldKind::Toggle),
                f("Tunnel", tunnel.into(), ch(&["none", "ssh", "aws ssm"])),
                f("SSH host", ssh.host.clone(), FieldKind::Text),
                f("SSH port", ssh.port.to_string(), FieldKind::Text),
                f("SSH user", ssh.user.clone(), FieldKind::Text),
                f("SSH auth", if ssh.auth == SshAuth::Key { "key" } else { "password" }.into(), ch(&["key", "password"])),
                f("SSH key file", ssh.key_path.clone(), FieldKind::Text),
                f("SSH secret", saved(c.ssh_secret_saved), FieldKind::Secret),
                // AWS SSM: the AWS CLI signs in with the user's own AWS config / SSO; nothing secret is kept here
                f("AWS profile", ssm.profile.clone(), FieldKind::Text),
                f("AWS region", ssm.region.clone(), FieldKind::Text),
                f("SSM target", ssm.target.clone(), FieldKind::Text),
                f("SSL", if c.active_ssl().is_some() { "on" } else { "off" }.into(), FieldKind::Toggle),
                // Newly enabled SSL is require: verifying needs a CA the built-in list knows, or a CA file (AWS RDS, a company CA, SSH tunnels)
                f("SSL mode", c.ssl.as_ref().map(|x| x.mode().to_string()).unwrap_or_else(|| "require".into()), ch(&["require", "verify-ca", "verify-full"])),
                f("CA file", c.ssl.as_ref().map(|x| x.ca_file.clone()).unwrap_or_default(), FieldKind::Text),
                f("Client cert", c.ssl.as_ref().map(|x| x.cert_file.clone()).unwrap_or_default(), FieldKind::Text),
                f("Client key", c.ssl.as_ref().map(|x| x.key_file.clone()).unwrap_or_default(), FieldKind::Text),
                f("Redis mode", c.redis_mode().into(), ch(&["standalone", "sentinel", "cluster"])),
                f("Sentinel master", c.redis.as_ref().map(|x| x.sentinel_master.clone()).unwrap_or_default(), FieldKind::Text),
                f("Redis nodes", c.redis.as_ref().map(|x| x.nodes.clone()).unwrap_or_default(), FieldKind::Text),
            ],
            edit,
            sel: 0,
            message: None,
            testing: None,
        }));
    }

    /// Form → (config, password, SSH secret)
    pub fn form_config(form: &ConnForm) -> Result<(ConnectionConfig, Option<String>, Option<String>), String> {
        let v = |l: &str| form.fields.iter().find(|f| f.label == l).map(|f| f.value.clone()).unwrap_or_default();
        let mut c = form.edit.clone().unwrap_or(ConnectionConfig {
            id: String::new(),
            name: String::new(),
            kind: DbKind::Postgres,
            host: String::new(),
            port: 0,
            user: String::new(),
            database: String::new(),
            scope: Default::default(),
            password_saved: false,
            ssh_secret_saved: false,
            ssh: None,
            color: None,
            read_only: false,
            options: Default::default(), ssl: None, redis: None, ssm: None,
        });
        c.kind = serde_json::from_value(serde_json::Value::String(v("Type"))).unwrap_or(DbKind::Postgres);
        c.host = v("Host").trim().into();
        c.port = v("Port").trim().parse().map_err(|_| "Port must be a number".to_string())?;
        c.user = v("User").trim().into();
        c.database = v("Database").trim().into();
        c.name = if v("Name").trim().is_empty() { format!("{}@{}", c.user, c.host) } else { v("Name").trim().into() };
        c.read_only = v("Read-only") == "on";
        c.options.confirm_changes = v("Production") == "on";
        // On is the default: kept unset, as older apps that drop the field read it
        c.options.production_manual = if v("Production Manual") == "on" { None } else { Some(false) };
        c.options.allow_plaintext = v("Allow unencrypted") == "on";
        // Save the shown value as is (leaving it unset would reopen it automatically after turning Production off)
        // Read-only and Production sources have no MCP write (ConnectionConfig::mcp_access): saved as read
        c.options.mcp = Some(if v("MCP") == "write" && (c.read_only || c.options.confirm_changes) { "read".into() } else { v("MCP") });
        // Disabled (not Production, or MCP Off): the stored value stays
        if form.disabled("Admin reports").is_none() {
            c.options.mcp_dba_reports = v("Admin reports") == "on";
        }
        // custom = a color outside the presets picked in the GUI: leave it
        match v("Color").as_str() {
            "custom" => {}
            name => c.color = crate::ui::COLOR_PRESETS.iter().find(|p| p.0 == name).map(|p| p.1.to_string()),
        }
        c.ssh = Some(SshConfig {
            enabled: v("Tunnel") == "ssh",
            host: v("SSH host").trim().into(),
            port: v("SSH port").trim().parse().unwrap_or(22),
            user: v("SSH user").trim().into(),
            auth: if v("SSH auth") == "password" { SshAuth::Password } else { SshAuth::Key },
            key_path: v("SSH key file").trim().into(),
        });
        // Kept while off (like SSH) so switching back finds the fields filled; an untouched source gets no ssm key
        let ssm = SsmConfig { enabled: v("Tunnel") == "aws ssm", profile: v("AWS profile").trim().into(), region: v("AWS region").trim().into(), target: v("SSM target").trim().into() };
        c.ssm = (c.ssm.is_some() || ssm != SsmConfig::default()).then_some(ssm);
        c.ssl = Some(SslConfig { enabled: v("SSL") == "on", mode: v("SSL mode"), ca_file: v("CA file").trim().into(), cert_file: v("Client cert").trim().into(), key_file: v("Client key").trim().into() });
        c.redis = if c.kind == DbKind::Redis { Some(RedisMode { mode: v("Redis mode"), sentinel_master: v("Sentinel master").trim().into(), nodes: v("Redis nodes").trim().into() }) } else { None };
        let secret = |s: String| if s == "<saved>" { None } else { Some(s) };
        Ok((c, secret(v("Password")), secret(v("SSH secret"))))
    }

    /// Master key for the password file: config folder ↔ OS keychain (asks, then moves. Runs separately since a keychain popup may appear)
    pub(super) fn password_key(&mut self) {
        use qurry_core::secrets::KeyLocation;
        let to_keychain = self.state.secret_key_location() == KeyLocation::File;
        let text = if to_keychain {
            "The password key is in the config folder (secret.key).\n\nMove it into the system keychain?\n\nThe GUI and the MCP server use the same key, so each asks for keychain access once (on macOS, again after updates while Qurry isn't code-signed).\n\nUpdate every Qurry app first (GUI, qurry-tui, qurry-mcp): older versions can't read the key there."
        } else {
            "The password key is in the system keychain.\n\nMove it back to the config folder (no keychain prompts)?"
        };
        let st = self.state.clone();
        self.popup = Some(Popup::Confirm {
            opened: None,
            text: text.into(),
            on_yes: Box::new(move |a| {
                a.status = "Waiting for the system keychain…".into();
                a.spawn(async move { tokio::task::spawn_blocking(move || st.set_secret_key_location(to_keychain)).await.map_err(|e| e.to_string())?.map_err(|e| e.to_string()) }, |a, r| {
                    a.status = match r {
                        Ok(KeyLocation::Keychain) => "The password key is now in the system keychain".into(),
                        Ok(KeyLocation::File) => "The password key is now in the config folder".into(),
                        Err(e) => e,
                    };
                });
            }),
        });
    }
}
