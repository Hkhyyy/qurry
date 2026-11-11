use super::*;

pub(super) fn redis_key(app: &mut App, id: u64, k: KeyEvent) {
    let Some(TabKind::Redis(r)) = app.tab_mut(id).map(|t| &mut t.kind) else { return };
    let cur_key = r.detail.as_ref().map(|d| d.0.clone());
    let cur_ty = r.detail.as_ref().map(|d| d.1.ty.clone()).unwrap_or_default();
    if r.focus_value {
        match k.code {
            KeyCode::Esc | KeyCode::Tab | KeyCode::Left if r.value.is_none() || k.code != KeyCode::Left => {
                r.focus_value = false;
            }
            KeyCode::Enter | KeyCode::Char('e') => {
                let Some(key) = cur_key else { return };
                // Edit the selected element
                let (field, val) = match (&r.value, &r.detail) {
                    (Some(g), _) => (g.value(g.row, 0).unwrap_or_default(), g.value(g.row, g.cols().saturating_sub(1)).unwrap_or_default()),
                    (None, Some((_, d))) => (String::new(), d.value.as_str().unwrap_or("").to_string()),
                    _ => return,
                };
                let ty = cur_ty.clone();
                let title = match ty.as_str() {
                    "hash" => format!("HSET {key} {field}"),
                    "list" => format!("LSET {key} {field}"),
                    "zset" => format!("score of {field}"),
                    "set" => "replace member".into(),
                    "string" => format!("SET {key}"),
                    _ => return,
                };
                let init = if ty == "set" { field.clone() } else { val };
                app.input(title, &init, move |a, v| {
                    let cmds: Vec<Vec<String>> = match ty.as_str() {
                        "hash" => vec![vec!["HSET".into(), key.clone(), field.clone(), v]],
                        "list" => vec![vec!["LSET".into(), key.clone(), field.clone(), v]],
                        "zset" => vec![vec!["ZADD".into(), key.clone(), v, field.clone()]],
                        "set" => vec![vec!["SREM".into(), key.clone(), field.clone()], vec!["SADD".into(), key.clone(), v]],
                        "string" => vec![vec!["SET".into(), key.clone(), v, "KEEPTTL".into()]],
                        _ => vec![],
                    };
                    for c in cmds {
                        a.redis_cmd(id, c, false);
                    }
                });
            }
            KeyCode::Char('x') | KeyCode::Delete => {
                let (Some(key), Some(g)) = (cur_key, &r.value) else { return };
                let item = g.value(g.row, 0).unwrap_or_default();
                let cmd: Vec<String> = match cur_ty.as_str() {
                    "hash" => vec!["HDEL".into(), key, item],
                    "set" => vec!["SREM".into(), key, item],
                    "zset" => vec!["ZREM".into(), key, item],
                    "stream" => vec!["XDEL".into(), key, item],
                    "list" => {
                        // Replace the element at the index with a marker, then LREM
                        let mark = format!("__dbtool_del_{}", uuid::Uuid::new_v4());
                        app.redis_cmd(id, vec!["LSET".into(), key.clone(), item, mark.clone()], false);
                        vec!["LREM".into(), key, "1".into(), mark]
                    }
                    _ => return,
                };
                app.popup = Some(Popup::Confirm { opened: None, text: format!("{}?", cmd.join(" ")), on_yes: Box::new(move |a| a.redis_cmd(id, cmd, false)) });
            }
            KeyCode::Char('a') => {
                let Some(key) = cur_key else { return };
                let ty = cur_ty.clone();
                let hint = match ty.as_str() {
                    "hash" => "field value",
                    "zset" => "score member",
                    "stream" => "field value",
                    _ => "value",
                };
                app.input(format!("Add to {key} ({hint})"), "", move |a, v| {
                    let parts: Vec<String> = v.splitn(2, ' ').map(String::from).collect();
                    let two = |p: &Vec<String>| p.len() == 2;
                    let cmd: Vec<String> = match ty.as_str() {
                        "hash" if two(&parts) => vec!["HSET".into(), key.clone(), parts[0].clone(), parts[1].clone()],
                        "zset" if two(&parts) => vec!["ZADD".into(), key.clone(), parts[0].clone(), parts[1].clone()],
                        "stream" if two(&parts) => vec!["XADD".into(), key.clone(), "*".into(), parts[0].clone(), parts[1].clone()],
                        "list" => vec!["RPUSH".into(), key.clone(), v],
                        "set" => vec!["SADD".into(), key.clone(), v],
                        _ => {
                            a.status = format!("Expected: {hint}");
                            return;
                        }
                    };
                    a.redis_cmd(id, cmd, false);
                });
            }
            _ => {
                if r.value.is_some() {
                    nav(app, k)
                }
            }
        }
        return;
    }
    let rows = r.rows();
    let n = rows.len();
    match k.code {
        KeyCode::Down | KeyCode::Char('j') => {
            r.sel = (r.sel + 1).min(n.saturating_sub(1));
            if let Some(k) = r.sel_key() {
                app.redis_load_key(id, k)
            }
        }
        KeyCode::Up | KeyCode::Char('k') => {
            r.sel = r.sel.saturating_sub(1);
            if let Some(k) = r.sel_key() {
                app.redis_load_key(id, k)
            }
        }
        // Folders: → / Enter expand·collapse. Keys: go to the value
        KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') if matches!(rows.get(r.sel), Some(crate::app::KRow::Folder { .. })) => {
            if let Some(crate::app::KRow::Folder { path, .. }) = rows.get(r.sel) {
                let open = r.open.contains(path);
                if open && k.code == KeyCode::Right {
                    r.sel += 1;
                } else if open {
                    r.open.remove(path);
                } else {
                    r.open.insert(path.clone());
                }
            }
        }
        // ←: collapse an open folder, otherwise go to the parent folder
        KeyCode::Left | KeyCode::Char('h') => match rows.get(r.sel) {
            Some(crate::app::KRow::Folder { path, .. }) if r.open.contains(path) => {
                r.open.remove(path);
            }
            Some(row) => {
                let depth = match row {
                    crate::app::KRow::Folder { depth, .. } | crate::app::KRow::Key { depth, .. } => *depth,
                };
                if depth > 0
                    && let Some(p) = rows[..r.sel].iter().rposition(|x| matches!(x, crate::app::KRow::Folder { depth: d, .. } if *d == depth - 1))
                {
                    r.sel = p;
                }
            }
            None => {}
        },
        KeyCode::Enter | KeyCode::Right | KeyCode::Tab => r.focus_value = true,
        KeyCode::Char('/') => {
            let p = r.pattern.clone();
            app.input("Filter keys (pattern, e.g. prod:user:*; empty = all)", &p, move |a, s| {
                if let Some(TabKind::Redis(r)) = a.tab_mut(id).map(|t| &mut t.kind) {
                    r.pattern = s.trim().to_string();
                }
                a.redis_scan(id, true);
            });
        }
        KeyCode::Char('d') => app.redis_pick_db(id),
        KeyCode::Char('m') => app.redis_scan(id, false),
        KeyCode::Char('r') => app.redis_scan(id, true),
        KeyCode::Char('D') | KeyCode::Delete => {
            if let Some(key) = cur_key {
                app.popup = Some(Popup::Confirm { opened: None, text: format!("Delete key \"{key}\"?"), on_yes: Box::new(move |a| {
                    if let Some(TabKind::Redis(r)) = a.tab_mut(id).map(|t| &mut t.kind) {
                        r.detail = None;
                        r.value = None;
                    }
                    a.redis_cmd(id, vec!["DEL".into(), key], true)
                }) });
            }
        }
        KeyCode::Char('R') => {
            if let Some(key) = cur_key {
                let k2 = key.clone();
                app.input("Rename key", &key, move |a, to| {
                    if to != k2 {
                        if let Some(TabKind::Redis(r)) = a.tab_mut(id).map(|t| &mut t.kind) {
                            r.detail = None;
                        }
                        a.redis_cmd(id, vec!["RENAME".into(), k2, to], true);
                    }
                });
            }
        }
        KeyCode::Char('t') => {
            if let Some(key) = cur_key {
                let ttl = r.detail.as_ref().map(|d| if d.1.ttl >= 0 { (d.1.ttl / 1000).to_string() } else { String::new() }).unwrap_or_default();
                app.input("TTL seconds (empty = no expiry)", &ttl, move |a, s| {
                    let cmd = if s.trim().is_empty() { vec!["PERSIST".into(), key] } else { vec!["EXPIRE".into(), key, s.trim().into()] };
                    a.redis_cmd(id, cmd, true);
                });
            }
        }
        KeyCode::Char('a') => {
            app.input("Add key: SET key value | HSET key field value | RPUSH key v | SADD key m | ZADD key score m", "SET ", move |a, line| {
                let st = a.state.clone();
                let target = a.tab_mut(id).map(|t| t.target.clone()).unwrap_or_default();
                a.spawn(async move { st.redis_exec_line(target, line).await.map_err(|e| e.to_string()) }, move |a, r| match r {
                    Ok(v) => {
                        a.status = crate::ui::format_reply(&v);
                        a.redis_scan(id, true);
                    }
                    Err(e) => a.status = e,
                });
            });
        }
        KeyCode::Char(':') => app.redis_prompt(id),
        KeyCode::Esc => app.focus = Focus::Tree,
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::Harness;

    /// The Redis key browser also has ":" folders (Medis): → expand, ← collapse / to parent folder, ↓↑ over visible lines
    #[tokio::test]
    async fn redis_browser_groups_keys_into_folders() {
        let mut h = Harness::new().await;
        h.redis_tab(&["dev:sessions:{1}", "dev:sessions:{2}", "prod:x", "token"]);
        let rows = |h: &Harness| match &h.app.tabs[h.app.active].kind {
            TabKind::Redis(r) => r
                .rows()
                .iter()
                .map(|x| match x {
                    crate::app::KRow::Folder { name, depth, count, path } => format!("{}{} {name} {count}", "  ".repeat(*depth), if r.open.contains(path) { "▾" } else { "▸" }),
                    crate::app::KRow::Key { idx, depth } => format!("{}{}", "  ".repeat(*depth), r.keys[*idx].key),
                })
                .collect::<Vec<_>>(),
            _ => panic!("not redis"),
        };
        let sel = |h: &Harness| match &h.app.tabs[h.app.active].kind {
            TabKind::Redis(r) => r.sel,
            _ => panic!(),
        };
        assert_eq!(rows(&h), ["▸ dev 2", "▸ prod 1", "token"]);
        h.key("Right");
        assert_eq!(rows(&h), ["▾ dev 2", "  ▸ sessions 2", "▸ prod 1", "token"]);
        h.key("Down");
        h.key("Right");
        h.key("Down");
        assert_eq!(rows(&h)[sel(&h)], "    dev:sessions:{1}");
        // ← : on a key goes to the parent folder, on an open folder collapses it
        h.key("Left");
        assert_eq!(sel(&h), 1);
        h.key("Left");
        assert_eq!(rows(&h), ["▾ dev 2", "  ▸ sessions 2", "▸ prod 1", "token"]);
        // On screen: a folder's count beside its name (like "tables 68"), the right column only has key TTLs (— without expiry)
        let s = h.screen(120, 20);
        let line = s.lines().find(|l| l.contains("▸ ▤ sessions")).unwrap_or_else(|| panic!("{s}"));
        assert!(line.contains("▸ ▤ sessions 2 "), "{s}");
        let token = s.lines().find(|l| l.contains("token")).unwrap_or_else(|| panic!("{s}"));
        assert!(token.trim_end_matches(['┃', '│', ' ']).ends_with('—'), "{s}");
    }

    /// Redis: the key list and value side have different status-line hints (D·t·R·: don't work on the value side)
    #[tokio::test]
    async fn redis_status_bar_follows_side() {
        use crate::app::{RedisTab, TabKind};
        let mut h = Harness::new().await;
        h.app.push_tab("test", "db0".into(), "redis".into(), TabKind::Redis(RedisTab::new("", None)));
        h.app.focus = crate::app::Focus::Grid;
        let last = |h: &mut Harness| h.screen(220, 20).lines().last().unwrap().to_string();
        let s = last(&mut h);
        assert!(s.contains("D del key") && s.contains("/ filter") && s.contains("d database"), "{s}");
        if let Some(TabKind::Redis(r)) = h.app.tabs.last_mut().map(|t| &mut t.kind) {
            r.focus_value = true;
        }
        let s = last(&mut h);
        assert!(s.contains("e edit") && s.contains("x delete") && s.contains("Esc keys"), "{s}");
        assert!(!s.contains("D del key"), "{s}");
    }

    /// A Redis data source saved in the harness (nothing connects until something runs)
    async fn redis_source(h: &mut Harness, id: &str, read_only: bool, production: bool) {
        let cfg: qurry_core::store::ConnectionConfig = serde_json::from_value(serde_json::json!({ "id": id, "name": id, "kind": "redis", "host": "127.0.0.1", "port": 1, "user": "", "database": "0", "readOnly": read_only, "options": { "confirmChanges": production } })).unwrap();
        h.app.state.store.save(&cfg).unwrap();
        h.app.reload_conns();
        h.settle().await;
    }
    fn input_line(h: &Harness) -> String {
        match &h.app.popup {
            Some(Popup::Input { input, .. }) => input.lines().join(""),
            _ => panic!("no input"),
        }
    }

    /// `:` completes commands with their arguments from the shared command table (⇥ picks), shows the arguments still to type,
    /// and ↑↓ walk this data source's history, newest first, with passwords masked
    #[tokio::test]
    async fn command_line_completes_and_walks_history() {
        let mut h = Harness::new().await;
        for l in ["GET a", "AUTH s3cret"] {
            h.app.state.history.add("test", Some("db0"), l).unwrap();
        }
        h.app.state.history.add("other", None, "GET elsewhere").unwrap();
        h.redis_tab(&["user:1"]);
        h.app.focus = crate::app::Focus::Grid;
        h.key(":");
        h.type_str("HG");
        let Some(Popup::Input { complete: Some(c), .. }) = &h.app.popup else { panic!("a list") };
        assert_eq!(c.items.iter().map(|i| i.label.as_str()).collect::<Vec<_>>(), ["HGET", "HGETALL"]);
        assert!(c.items[0].detail.starts_with("key field — Returns the value of a field in a hash"));
        let s = h.screen(120, 30);
        assert!(s.contains("db0>") && s.contains("HGETALL") && s.contains("↑↓ history"), "{s}");
        h.key("Tab");
        assert_eq!(input_line(&h), "HGET ");
        let s = h.screen(120, 30);
        assert!(s.lines().any(|l| l.contains("HGET key field")), "the arguments still to type, after the text\n{s}");
        h.key("Up");
        assert_eq!(input_line(&h), "AUTH ***");
        h.key("Up");
        assert_eq!(input_line(&h), "GET a");
        h.key("Up");
        assert_eq!(input_line(&h), "GET a", "only this source's history");
        h.key("Down");
        h.key("Down");
        assert_eq!(input_line(&h), "");
    }

    /// Key names: ↩ runs a line whose key is typed out (`GET user:1` with `user:10` there too) instead of picking the longer key;
    /// after ↑↓ ↩ picks; ⇥ picks; a key with a space goes in quoted, as one argument
    #[tokio::test]
    async fn command_line_key_names_dont_replace_a_typed_key() {
        let mut h = Harness::new().await;
        redis_source(&mut h, "plain", false, false).await;
        h.app.push_tab("plain", "db0".into(), "plain [db0]".into(), TabKind::Redis(crate::app::RedisTab::new("", None)));
        // The names a bounded SCAN would read for the prefix
        let scanned = |h: &mut Harness, prefix: &str, names: &[&str]| {
            if let Some(Popup::Input { input, redis: Some(p), complete, .. }) = &mut h.app.popup {
                p.keys = Some((prefix.into(), names.iter().map(|s| s.to_string()).collect()));
                *complete = App::redis_complete(input, p).0;
            }
        };
        let labels = |h: &Harness| match &h.app.popup {
            Some(Popup::Input { complete: Some(c), .. }) => c.items.iter().map(|i| i.label.clone()).collect::<Vec<_>>(),
            _ => vec![],
        };
        let open = |h: &mut Harness, line: &str| {
            h.app.popup = None;
            h.app.focus = crate::app::Focus::Grid;
            h.key(":");
            h.type_str(line);
        };
        open(&mut h, "GET user:1");
        scanned(&mut h, "user:1", &["user:10", "user:1"]);
        assert_eq!(labels(&h), ["user:1", "user:10"], "the typed key first");
        h.key("Enter");
        assert!(!matches!(h.app.popup, Some(Popup::Input { .. })), "↩ ran the line");
        assert_eq!(h.app.state.history.list(Some("plain"), "", 10)[0].sql, "GET user:1");
        // ↓ to the longer one, then ↩ picks it
        open(&mut h, "GET user:1");
        scanned(&mut h, "user:1", &["user:10", "user:1"]);
        h.key("Down");
        h.key("Enter");
        assert_eq!(input_line(&h), "GET user:10 ");
        // ⇥ picks; a key with a space stays one argument
        open(&mut h, "DEL us");
        scanned(&mut h, "us", &["user 1"]);
        h.key("Tab");
        assert_eq!(input_line(&h), r#"DEL "user 1" "#);
    }

    /// The command line follows the console's rules: HELP answers from the table, a read-only source refuses a write without
    /// sending it, Production asks before a write, FLUSHDB / DEL always ask. Each line lands in this source's history
    #[tokio::test]
    async fn command_line_follows_the_console_rules() {
        let mut h = Harness::new().await;
        let run = |h: &mut Harness, line: &str| {
            h.app.focus = crate::app::Focus::Grid;
            h.key(":");
            h.type_str(line);
            h.key("Enter");
        };
        let text = |h: &Harness| match &h.app.popup {
            Some(Popup::Message { title, text }) => format!("{title}: {text}"),
            Some(Popup::Confirm { text, .. }) => format!("confirm: {text}"),
            _ => String::new(),
        };
        redis_source(&mut h, "ro", true, false).await;
        h.app.push_tab("ro", "db0".into(), "ro [db0]".into(), TabKind::Redis(crate::app::RedisTab::new("", None)));
        run(&mut h, "help hget");
        assert_eq!(text(&h), "help hget: HGET key field — Returns the value of a field in a hash. (read)");
        h.app.popup = None;
        run(&mut h, "SET greeting hi");
        assert!(text(&h).contains("is read-only: SET changes data"), "{}", text(&h));
        h.app.popup = None;
        run(&mut h, "FLUSHDB");
        assert!(text(&h).contains("is read-only: FLUSHDB changes data"), "read-only refuses before asking: {}", text(&h));
        h.app.popup = None;
        assert_eq!(h.app.state.history.list(Some("ro"), "", 10).iter().map(|i| i.sql.as_str()).collect::<Vec<_>>(), ["FLUSHDB", "SET greeting hi", "help hget"]);

        redis_source(&mut h, "prod", false, true).await;
        h.app.push_tab("prod", "db0".into(), "prod [db0]".into(), TabKind::Redis(crate::app::RedisTab::new("", None)));
        run(&mut h, "SET greeting hi");
        assert_eq!(text(&h), "confirm: Production: prod — this command changes data:\n  SET greeting hi\nRun it?");
        h.key("n");
        assert!(h.app.state.history.list(Some("prod"), "", 10).is_empty(), "a cancelled line isn't kept");
        run(&mut h, "DEL tags");
        assert_eq!(text(&h), "confirm: Production: prod — DEL deletes or replaces data:\n  DEL tags\nRun it anyway?");
        h.key("n");

        redis_source(&mut h, "plain", false, false).await;
        h.app.push_tab("plain", "db0".into(), "plain [db0]".into(), TabKind::Redis(crate::app::RedisTab::new("", None)));
        run(&mut h, "FLUSHDB");
        assert_eq!(text(&h), "confirm: FLUSHDB deletes or replaces data:\n  FLUSHDB\nRun it anyway?");
        h.key("n");
        run(&mut h, "SET greeting hi");
        assert!(!text(&h).starts_with("confirm"), "off Production a write runs without asking");
    }

    /// One Redis tab per source: a workspace saved before 0.3.2 with a tab per database restores one, on the database saved last
    #[tokio::test]
    async fn old_per_database_redis_tabs_restore_as_one() {
        let mut h = Harness::new().await;
        redis_source(&mut h, "rd", false, false).await;
        let a = h.app.restore_redis("rd", "db0".into(), "rd [db0]".into(), "*");
        let b = h.app.restore_redis("rd", "db3".into(), "rd [db3]".into(), "user:*");
        assert_eq!(a, b);
        let t: Vec<_> = h.app.tabs.iter().map(|t| (t.title.as_str(), t.db.as_str(), t.target.as_str())).collect();
        assert_eq!(t, [("rd [db3]", "db3", "rd::db3")]);
        let pattern = |h: &Harness| match &h.app.tabs[0].kind {
            TabKind::Redis(r) => r.pattern.clone(),
            _ => panic!(),
        };
        assert_eq!(pattern(&h), "user:*");
        // The "*" saved for every key before 0.3.2 is no filter now (the title and / don't show one)
        h.app.restore_redis("rd", "db0".into(), "rd [db0]".into(), "*");
        assert_eq!(pattern(&h), "");
    }

    /// On a real server (QURRY_TEST_REDIS, own db12 and redis-browser:tui:* keys): the explorer lists the source's databases with
    /// key counts and no keys; Enter on one opens the source's tab, a click on another switches it, d picks one from the list;
    /// / filters the keys; `:` completes key names from a bounded SCAN and shows a hash as a field / value table
    #[tokio::test]
    #[ignore]
    async fn redis_explorer_and_tab_on_a_real_server() {
        let addr = std::env::var("QURRY_TEST_REDIS").unwrap_or_else(|_| "127.0.0.1:16379".into());
        let (host, port) = addr.rsplit_once(':').unwrap();
        let mut h = Harness::new().await;
        let cfg: qurry_core::store::ConnectionConfig = serde_json::from_value(serde_json::json!({ "id": "rb", "name": "rb", "kind": "redis", "host": host, "port": port.parse::<u16>().unwrap(), "user": "", "database": "12" })).unwrap();
        h.app.state.store.save(&cfg).unwrap();
        h.app.reload_conns();
        h.settle().await;
        let st = h.app.state.clone();
        for l in ["SET redis-browser:tui:a:1 x", "SET redis-browser:tui:b y", "HSET redis-browser:tui:h name kim tier gold"] {
            st.redis_exec_line("rb::db12".into(), l.into()).await.unwrap();
        }
        // The explorer: databases with key counts, leaves
        let conn = h.app.tree.visible().into_iter().map(|(_, n)| n.clone()).find(|n| n.conn == "rb").unwrap();
        h.app.toggle_node(&conn);
        h.settle().await;
        let dbs: Vec<_> = h.app.tree.visible().into_iter().map(|(_, n)| n.clone()).filter(|n| n.conn == "rb" && n.kind == crate::tree::NodeKind::Db).collect();
        let db12 = dbs.iter().find(|n| n.label == "db12").expect("db12 listed");
        assert!(db12.hint.parse::<i64>().unwrap() >= 3 && dbs.iter().all(|n| !n.expandable), "{dbs:?}");
        assert!(h.app.tree.visible().iter().all(|(_, n)| !matches!(&n.kind, crate::tree::NodeKind::Object(k) if k == "key")), "no keys in the tree");
        h.app.activate_node(db12, false);
        h.settle().await;
        let tab = |h: &Harness| h.app.tabs.iter().filter(|t| matches!(t.kind, TabKind::Redis(_))).map(|t| (t.db.clone(), t.target.clone())).collect::<Vec<_>>();
        assert_eq!(tab(&h), [("db12".to_string(), "rb::db12".to_string())]);
        let s = h.screen(140, 30);
        assert!(s.contains("db12 · 3 keys") && s.contains("▸ ▤ redis-browser 3"), "{s}");
        // A click on db0 switches the open tab (still one), d picks db12 back from the list. A db12 key still loading when it
        // switches doesn't land in the db0 tab (its value edits would write into db0)
        let id = h.app.tabs[h.app.active].id;
        h.app.redis_load_key(id, "redis-browser:tui:h".into());
        h.app.redis_db_clicked("rb", "db0".into());
        h.settle().await;
        assert_eq!(tab(&h), [("db0".to_string(), "rb::db0".to_string())]);
        let TabKind::Redis(r) = &h.app.tabs[h.app.active].kind else { panic!() };
        assert!(r.detail.as_ref().is_none_or(|d| d.0 != "redis-browser:tui:h"), "db12's key in the db0 tab");
        h.app.focus = crate::app::Focus::Grid;
        h.key("d");
        h.settle().await;
        let Some(Popup::Palette(p)) = &h.app.popup else { panic!("the database list") };
        let i = p.items.iter().position(|it| it.label.starts_with("db12 · ")).expect("db12 with its key count");
        if let Some(Popup::Palette(p)) = &mut h.app.popup {
            p.sel = i;
        }
        h.key("Enter");
        h.settle().await;
        assert_eq!(tab(&h), [("db12".to_string(), "rb::db12".to_string())]);
        // / filters the keys (SCAN MATCH)
        h.key("/");
        h.type_str("redis-browser:tui:a*");
        h.key("Enter");
        h.settle().await;
        let keys = |h: &Harness| match &h.app.tabs[h.app.active].kind {
            TabKind::Redis(r) => r.keys.iter().map(|k| k.key.clone()).collect::<Vec<_>>(),
            _ => panic!(),
        };
        assert_eq!(keys(&h), ["redis-browser:tui:a:1"]);
        // : completes key names from a bounded SCAN
        h.key(":");
        h.type_str("HGETALL redis-browser:tui:");
        h.settle().await;
        let Some(Popup::Input { complete: Some(c), .. }) = &h.app.popup else { panic!("key names") };
        assert_eq!(c.items.iter().map(|i| i.label.as_str()).collect::<Vec<_>>(), ["redis-browser:tui:a:1", "redis-browser:tui:b", "redis-browser:tui:h"]);
        h.key("Down");
        h.key("Down");
        h.key("Enter");
        h.key("Enter");
        h.settle().await;
        let Some(Popup::Message { text, .. }) = &h.app.popup else { panic!("the reply") };
        assert_eq!(text, "field  value\nname   kim\ntier   gold");
        st.redis_exec_line("rb::db12".into(), "DEL redis-browser:tui:a:1 redis-browser:tui:b redis-browser:tui:h".into()).await.unwrap();
    }
}
