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
                if depth > 0 {
                    if let Some(p) = rows[..r.sel].iter().rposition(|x| matches!(x, crate::app::KRow::Folder { depth: d, .. } if *d == depth - 1)) {
                        r.sel = p;
                    }
                }
            }
            None => {}
        },
        KeyCode::Enter | KeyCode::Right | KeyCode::Tab => r.focus_value = true,
        KeyCode::Char('/') => {
            let p = r.pattern.clone();
            app.input("Key pattern (SCAN MATCH)", &p, move |a, s| {
                if let Some(TabKind::Redis(r)) = a.tab_mut(id).map(|t| &mut t.kind) {
                    r.pattern = if s.trim().is_empty() { "*".into() } else { s };
                }
                a.redis_scan(id, true);
            });
        }
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
        KeyCode::Char(':') => {
            app.input("redis>", "", move |a, line| {
                let run = move |a: &mut App, line: String| {
                    let st = a.state.clone();
                    let target = a.tab_mut(id).map(|t| t.target.clone()).unwrap_or_default();
                    let shown = line.clone();
                    a.spawn(async move { st.redis_exec_line(target, line).await.map_err(|e| e.to_string()) }, move |a, r| match r {
                        Ok(v) => a.notice(shown, crate::ui::format_reply(&v)),
                        Err(e) => a.notice(shown, format!("(error) {e}")),
                    });
                };
                // Destructive commands like FLUSHALL·FLUSHDB·DEL ask once more
                match qurry_core::guard::unsafe_statement(&line, qurry_core::store::DbKind::Redis) {
                    Some(why) => a.popup = Some(Popup::Confirm { opened: None, text: format!("{why}:\n  {line}\nRun it anyway?"), on_yes: Box::new(move |a| run(a, line)) }),
                    None => run(a, line),
                }
            });
        }
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
        // Folders and counts on screen too (counts at the right edge, like the GUI)
        let s = h.screen(120, 20);
        let line = s.lines().find(|l| l.contains("▸ ▤ sessions")).unwrap_or_else(|| panic!("{s}"));
        assert!(line.trim_end_matches(['┃', '│', ' ']).ends_with('2') && s.contains("token"), "{s}");
    }

    /// Redis: the key list and value side have different status-line hints (D·t·R·: don't work on the value side)
    #[tokio::test]
    async fn redis_status_bar_follows_side() {
        use crate::app::{RedisTab, TabKind};
        let mut h = Harness::new().await;
        let r = RedisTab { pattern: "*".into(), keys: vec![], cursor: "0".into(), sel: 0, off: 0, open: Default::default(), detail: None, text: None, value: None, focus_value: false, scanning: false, want: None };
        h.app.push_tab("test", "db0".into(), "redis".into(), TabKind::Redis(r));
        h.app.focus = crate::app::Focus::Grid;
        let last = |h: &mut Harness| h.screen(220, 20).lines().last().unwrap().to_string();
        let s = last(&mut h);
        assert!(s.contains("D del key") && s.contains("/ pattern"), "{s}");
        if let Some(TabKind::Redis(r)) = h.app.tabs.last_mut().map(|t| &mut t.kind) {
            r.focus_value = true;
        }
        let s = last(&mut h);
        assert!(s.contains("e edit") && s.contains("x delete") && s.contains("Esc keys"), "{s}");
        assert!(!s.contains("D del key"), "{s}");
    }
}
