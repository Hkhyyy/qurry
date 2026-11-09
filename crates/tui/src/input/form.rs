use super::*;

/// ⌃t: Test Connection in the background. The form shows the elapsed time; Esc or ⌃t cancels it
pub(super) fn test_form(app: &mut App, form: &mut crate::app::ConnForm) {
    let (cfg, pw, ssh) = match App::form_config(form) {
        Ok(x) => x,
        Err(e) => return form.message = Some((false, e)),
    };
    let st = app.state.clone();
    let started = std::time::Instant::now();
    let production = cfg.options.confirm_changes;
    form.message = None;
    let h = app.spawn_abortable(async move { st.test_connection(cfg, pw, ssh).await.map_err(|e| e.to_string()) }, move |a, r| {
        // Only the test still shown: not one that was cancelled or replaced
        if let Some(Popup::ConnForm(f)) = a.popup_under()
            && f.testing.as_ref().is_some_and(|t| t.0 == started)
        {
            f.testing = None;
            f.message = Some(match r {
                Ok(v) => (true, format!("✓ {v}")),
                Err(e) if e.contains(qurry_core::tunnel::UNKNOWN_HOST) => (false, format!("✕ {e} Press ⌃y to trust it.")),
                // Production: never a one-key path to plaintext (the GUI dialog shows no button either), only where the setting is
                Err(e) if production && qurry_shared::errors::tls_opt_in(&e) => (
                    false,
                    format!("✕ {e}\nProduction data source: Qurry won't turn off encryption from here. If the server has no TLS and the network is trusted, turn on \"Allow unencrypted\" in this form."),
                ),
                // The GUI dialog's Allow button, as a field to turn on: select it (the form only changes when Space is pressed)
                Err(e) if qurry_shared::errors::tls_opt_in(&e) => {
                    if let Some(i) = f.fields.iter().position(|x| x.label == "Allow unencrypted") {
                        f.sel = i;
                    }
                    (false, format!("✕ {e}\nOn a trusted network, Space turns on \"Allow unencrypted\" (selected), then ⌃t tests again."))
                }
                Err(e) => (false, format!("✕ {e}")),
            });
        }
    });
    form.testing = Some((started, h));
}

pub(super) fn save_form(app: &mut App, mut form: crate::app::ConnForm) {
    // A test still running is for the settings being saved now: stop it (and its tunnel)
    if let Some((_, h)) = form.testing.take() {
        h.abort();
    }
    // What the form doesn't show (scope, startup script …) comes from the source as saved now, not as it was when the form opened
    if let Some(fresh) = form.edit.as_ref().and_then(|e| app.state.store.get(&e.id).ok().flatten()) {
        form.edit = Some(fresh);
    }
    match App::form_config(&form) {
        Ok((cfg, pw, ssh)) => {
            let st = app.state.clone();
            // Form's Workspace: an edited Data Source moves out of where it was, a new one goes into the selected one
            let from = form.edit.as_ref().map(|e| app.ws_of_conn(&e.id));
            let to = form.fields.iter().find(|f| f.label == "Workspace").and_then(|f| app.ws_from_label(&f.value)).unwrap_or(app.cur_ws);
            app.spawn(async move { st.save_connection(cfg, pw, ssh).await.map_err(|e| e.to_string()) }, move |a, r| match r {
                Ok(c) => {
                    a.status = format!("Saved data source {}", c.name);
                    a.ws_place_conn(&c.id, from, to);
                    a.tree.forget(&c.id);
                    a.tree.expanded.remove(&c.id);
                    a.reload_conns();
                }
                Err(e) => a.status = format!("Save failed: {e}"),
            });
        }
        Err(e) => {
            form.message = Some((false, e));
            app.popup = Some(Popup::ConnForm(form));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::Harness;

    /// Saving the form keeps what it doesn't show (scope, startup script …) as saved now, not as it was when the form opened
    #[tokio::test]
    async fn conn_form_keeps_settings_it_does_not_show() {
        use qurry_core::store::{ConnectionConfig, DbKind};
        let mut h = Harness::new().await;
        let cfg = ConnectionConfig {
            id: "c1".into(), name: "pg".into(), kind: DbKind::Postgres, host: "localhost".into(), port: 5432, user: "postgres".into(), database: "postgres".into(),
            scope: Default::default(), password_saved: false, ssh_secret_saved: false, ssh: None, color: None, read_only: false, options: Default::default(), ssl: None, redis: None, ssm: None,
        };
        h.app.state.store.save(&cfg).unwrap();
        h.app.reload_conns();
        h.app.conn_form(Some(cfg.clone()));
        // Meanwhile the GUI changes what the TUI form doesn't show
        let mut other = cfg.clone();
        other.scope.databases = Some(vec!["shop".into()]);
        other.options.startup_script = "SET search_path = shop".into();
        h.app.state.store.save(&other).unwrap();
        let Some(crate::app::Popup::ConnForm(f)) = &mut h.app.popup else { panic!() };
        f.fields.iter_mut().find(|x| x.label == "Name").unwrap().value = "renamed".into();
        h.key("Enter");
        h.settle().await;
        let saved = h.app.state.store.get("c1").unwrap().unwrap();
        assert_eq!(saved.name, "renamed");
        assert_eq!(saved.scope.databases, Some(vec!["shop".to_string()]));
        assert_eq!(saved.options.startup_script, "SET search_path = shop");
    }

    /// A new Data Source starts hidden from MCP. Saves the MCP value shown in the form as is
    #[tokio::test]
    async fn conn_form_mcp_default() {
        let mut h = Harness::new().await;
        let cfg_of = |h: &Harness| match &h.app.popup {
            Some(crate::app::Popup::ConnForm(f)) => App::form_config(f).unwrap().0,
            _ => panic!("no form"),
        };
        h.app.conn_form(None);
        assert_eq!(cfg_of(&h).options.mcp.as_deref(), Some("off"));
        let mut old = cfg_of(&h);
        old.options.mcp = None;
        old.options.confirm_changes = true;
        h.app.conn_form(Some(old.clone()));
        assert_eq!(cfg_of(&h).options.mcp.as_deref(), Some("off"), "what the form shows is saved");
        // Turning off Production keeps the off that was shown (doesn't open up by itself)
        let Some(crate::app::Popup::ConnForm(f)) = &mut h.app.popup else { panic!() };
        f.fields.iter_mut().find(|x| x.label == "Production").unwrap().value = "off".into();
        assert_eq!(cfg_of(&h).options.mcp.as_deref(), Some("off"));
        let Some(crate::app::Popup::ConnForm(f)) = &mut h.app.popup else { panic!() };
        f.fields.iter_mut().find(|x| x.label == "MCP").unwrap().value = "read".into();
        assert_eq!(cfg_of(&h).options.mcp.as_deref(), Some("read"));
        // Write on a Production source is saved as read (MCP never writes there)
        let Some(crate::app::Popup::ConnForm(f)) = &mut h.app.popup else { panic!() };
        f.fields.iter_mut().find(|x| x.label == "Production").unwrap().value = "on".into();
        f.fields.iter_mut().find(|x| x.label == "MCP").unwrap().value = "write".into();
        assert_eq!(cfg_of(&h).options.mcp.as_deref(), Some("read"));
        // A value core reads as off ("On", a typo) is shown and saved as off, never as write
        old.options.confirm_changes = false;
        old.options.mcp = Some("On".into());
        h.app.conn_form(Some(old));
        assert_eq!(cfg_of(&h).options.mcp.as_deref(), Some("off"));
    }

    /// Tunnel: none / ssh / aws ssm. One tunnel at a time; a source that never used SSM is saved without it
    #[tokio::test]
    async fn conn_form_tunnel_choice() {
        let mut h = Harness::new().await;
        let cfg_of = |h: &Harness| match &h.app.popup {
            Some(crate::app::Popup::ConnForm(f)) => App::form_config(f).unwrap().0,
            _ => panic!("no form"),
        };
        let set = |h: &mut Harness, label: &str, v: &str| {
            let Some(crate::app::Popup::ConnForm(f)) = &mut h.app.popup else { panic!() };
            f.fields.iter_mut().find(|x| x.label == label).unwrap().value = v.into();
        };
        h.app.conn_form(None);
        assert_eq!(cfg_of(&h).ssm, None);
        set(&mut h, "Tunnel", "aws ssm");
        set(&mut h, "AWS profile", " prod ");
        set(&mut h, "AWS region", "ap-northeast-2");
        set(&mut h, "SSM target", "i-0123456789abcdef0");
        let c = cfg_of(&h);
        assert_eq!(c.active_ssm().map(|s| (s.profile.as_str(), s.region.as_str(), s.target.as_str())), Some(("prod", "ap-northeast-2", "i-0123456789abcdef0")));
        assert!(c.active_ssh().is_none());
        // Reopened: the choice and fields come back; switching to SSH turns SSM off but keeps its fields
        h.app.conn_form(Some(c));
        assert_eq!(cfg_of(&h).active_ssm().map(|s| s.target.clone()), Some("i-0123456789abcdef0".into()));
        set(&mut h, "Tunnel", "ssh");
        set(&mut h, "SSH host", "bastion");
        let c = cfg_of(&h);
        assert!(c.active_ssh().is_some() && c.active_ssm().is_none());
        assert_eq!(c.ssm.unwrap().target, "i-0123456789abcdef0");
    }

    /// On an 80×24 terminal the form scrolls to the selected field, and the test message and key hints stay on screen.
    /// Only the chosen tunnel's fields are shown and visited
    #[tokio::test]
    async fn conn_form_scrolls_and_keeps_the_message() {
        let mut h = Harness::new().await;
        let ssm = serde_json::from_value(serde_json::json!({ "id": "s", "name": "orders", "kind": "postgres", "host": "orders.internal", "port": 5432, "user": "app",
            "ssm": { "enabled": true, "profile": "prod", "region": "ap-northeast-2", "target": "i-0123456789abcdef0" } })).unwrap();
        h.app.conn_form(Some(ssm));
        let tunnel = {
            let Some(crate::app::Popup::ConnForm(f)) = &h.app.popup else { panic!() };
            f.fields.iter().position(|x| x.label == "Tunnel").unwrap()
        };
        let Some(crate::app::Popup::ConnForm(f)) = &mut h.app.popup else { panic!() };
        f.sel = tunnel;
        h.key("Down");
        h.key("Down");
        h.key("Down");
        let Some(crate::app::Popup::ConnForm(f)) = &mut h.app.popup else { panic!() };
        assert_eq!(f.fields[f.sel].label, "SSM target", "the SSH fields are skipped");
        f.message = Some((false, "✕ Your AWS SSO session has expired or you haven't signed in. Run: aws sso login --profile prod — then try again.".into()));
        let s = h.screen(80, 24);
        assert!(s.contains("SSM target") && s.contains("i-0123456789abcdef0"), "{s}");
        assert!(s.contains("sso login --profile prod — then try again."), "{s}");
        assert!(s.contains("Esc cancel"), "{s}");
        assert!(!s.contains("SSH host"), "{s}");
        // Back to the top: the first fields come into view again
        let Some(crate::app::Popup::ConnForm(f)) = &mut h.app.popup else { panic!() };
        f.sel = 0;
        let s = h.screen(80, 24);
        assert!(s.contains("Name") && s.contains("Esc cancel"), "{s}");
        // Switching to SSH shows its fields instead
        let Some(crate::app::Popup::ConnForm(f)) = &mut h.app.popup else { panic!() };
        f.sel = tunnel;
        h.key("Left");
        h.key("Down");
        let Some(crate::app::Popup::ConnForm(f)) = &h.app.popup else { panic!() };
        assert_eq!(f.fields[f.sel].label, "SSH host");
    }

    /// AWS SSM with a local Host: the form says Host and Port are the database as seen from the instance, without refusing it
    #[tokio::test]
    async fn conn_form_hints_a_local_host_with_ssm() {
        let mut h = Harness::new().await;
        h.app.conn_form(None);
        let set = |h: &mut Harness, label: &str, v: &str| form(h).fields.iter_mut().find(|x| x.label == label).unwrap().value = v.into();
        let hint = "With AWS SSM, 127.0.0.1 is the instance itself";
        set(&mut h, "Host", "127.0.0.1");
        assert!(!h.screen(120, 40).contains(hint), "not without SSM");
        set(&mut h, "Tunnel", "aws ssm");
        let s = h.screen(120, 40);
        assert!(s.contains(hint) && s.contains("as seen from the instance"), "{s}");
        set(&mut h, "Host", "mydb.abc.ap-northeast-2.rds.amazonaws.com");
        assert!(!h.screen(120, 40).contains("is the instance itself"));
        // A test's result takes the message's place
        set(&mut h, "Host", "localhost");
        form(&mut h).message = Some((false, "✕ failed".into()));
        let s = h.screen(120, 40);
        assert!(s.contains("✕ failed") && !s.contains("is the instance itself"), "{s}");
    }

    fn form(h: &mut Harness) -> &mut crate::app::ConnForm {
        let Some(crate::app::Popup::ConnForm(f)) = &mut h.app.popup else { panic!("no form") };
        f
    }

    /// The form doesn't move or resize when a test starts or fails, or when another Tunnel is chosen
    #[tokio::test]
    async fn conn_form_keeps_its_size() {
        let mut h = Harness::new().await;
        h.app.conn_form(None);
        let frame = |h: &mut Harness| {
            let s = h.screen(120, 40);
            let rows: Vec<&str> = s.lines().collect();
            let top = rows.iter().position(|l| l.contains("New Data Source")).expect("title");
            let hint = rows.iter().position(|l| l.contains("Esc cancel")).expect("hint");
            (top, hint)
        };
        let want = frame(&mut h);
        for tunnel in ["ssh", "aws ssm", "none"] {
            form(&mut h).fields.iter_mut().find(|x| x.label == "Tunnel").unwrap().value = tunnel.into();
            assert_eq!(frame(&mut h), want, "{tunnel}");
        }
        form(&mut h).message = Some((false, "✕ i-0123456789abcdef0 is not connected to Session Manager: it is stopped, in another region than the one used, or its SSM agent is offline. Check the instance id and region.".into()));
        assert_eq!(frame(&mut h), want, "a wrapped error");
        form(&mut h).message = None;
        assert_eq!(frame(&mut h), want);
    }

    /// ⌃t shows the elapsed time; ⌃t again or Esc cancels the test (it is dropped, and with it its tunnel) and keeps the form.
    /// Pressing ⌃t twice never runs two tests
    #[tokio::test]
    async fn conn_form_test_shows_time_and_cancels() {
        let mut h = Harness::new().await;
        // A server that never answers: the test would take the whole timeout
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        h.app.conn_form(None);
        form(&mut h).fields.iter_mut().find(|x| x.label == "Port").unwrap().value = port.to_string();
        form(&mut h).fields.iter_mut().find(|x| x.label == "Host").unwrap().value = "127.0.0.1".into();
        h.key("Ctrl+t");
        assert!(form(&mut h).testing.is_some());
        assert!(h.app.any_running(), "redrawn for the elapsed time");
        let s = h.screen(120, 40);
        assert!(s.contains("Testing… 0s · ⌃t / Esc cancel"), "{s}");
        h.key("Ctrl+t");
        assert!(form(&mut h).testing.is_none(), "the second ⌃t cancels instead of starting another test");
        assert!(h.screen(120, 40).contains("Test cancelled"));
        h.key("Ctrl+t");
        h.key("Esc");
        assert!(form(&mut h).testing.is_none(), "Esc cancels the test and keeps the form");
        h.settle().await;
        assert_eq!(form(&mut h).message.as_ref().map(|m| m.1.as_str()), Some("Test cancelled"), "a cancelled test reports nothing later");
        h.key("Esc");
        assert!(h.app.popup.is_none());
        drop(l);
    }

    /// Choosing another Tunnel drops the running test and the last message: nothing for the old tunnel shows up later
    #[tokio::test]
    async fn conn_form_tunnel_change_drops_the_test() {
        let mut h = Harness::new().await;
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        h.app.conn_form(None);
        form(&mut h).fields.iter_mut().find(|x| x.label == "Port").unwrap().value = port.to_string();
        form(&mut h).fields.iter_mut().find(|x| x.label == "Host").unwrap().value = "127.0.0.1".into();
        form(&mut h).message = Some((false, "✕ Unknown SSH host key … Press ⌃y to trust it.".into()));
        h.key("Ctrl+t");
        assert!(form(&mut h).testing.is_some());
        let f = form(&mut h);
        f.sel = f.fields.iter().position(|x| x.label == "Tunnel").unwrap();
        h.key("Right");
        assert!(form(&mut h).testing.is_none(), "the test stopped");
        h.settle().await;
        assert_eq!(form(&mut h).message, None);
        assert!(!h.screen(120, 40).contains("Testing…"));
        drop(l);
    }

    /// ⌃y trusts the SSH tunnel's host only while Tunnel is ssh (its fields are hidden otherwise)
    #[tokio::test]
    async fn conn_form_trust_key_only_with_ssh() {
        let mut h = Harness::new().await;
        h.app.conn_form(None);
        form(&mut h).fields.iter_mut().find(|x| x.label == "Tunnel").unwrap().value = "aws ssm".into();
        h.key("Ctrl+y");
        let m = form(&mut h).message.clone().unwrap();
        assert!(!m.0 && m.1.contains("set Tunnel to ssh first"), "{m:?}");
    }

    /// Empty AWS profile / region say what is used instead, like the GUI's placeholders
    #[tokio::test]
    async fn conn_form_says_what_empty_aws_fields_mean() {
        let mut h = Harness::new().await;
        h.app.conn_form(None);
        form(&mut h).fields.iter_mut().find(|x| x.label == "Tunnel").unwrap().value = "aws ssm".into();
        let s = h.screen(120, 40);
        assert!(s.contains("(default profile)") && s.contains("(the profile's region)"), "{s}");
        form(&mut h).fields.iter_mut().find(|x| x.label == "AWS region").unwrap().value = "us-east-1".into();
        assert!(!h.screen(120, 40).contains("(the profile's region)"));
    }
}
