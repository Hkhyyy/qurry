use super::*;

/// Redis value → grid (None for string: shown as text)
pub fn redis_grid(d: &KeyDetail) -> Option<Grid> {
    let arr = d.value.as_array()?;
    let (names, rows): (Vec<&str>, Vec<Vec<Value>>) = match d.ty.as_str() {
        "hash" => (vec!["field", "value"], arr.iter().map(|x| vec![x[0].clone(), x[1].clone()]).collect()),
        "zset" => (vec!["member", "score"], arr.iter().map(|x| vec![x[0].clone(), x[1].clone()]).collect()),
        "list" => (vec!["index", "value"], arr.iter().enumerate().map(|(i, x)| vec![Value::from(i), x.clone()]).collect()),
        "set" => (vec!["member"], arr.iter().map(|x| vec![x.clone()]).collect()),
        "stream" => (vec!["id", "fields"], arr.iter().map(|x| vec![x[0].clone(), Value::from(x[1].to_string())]).collect()),
        _ => return None,
    };
    let result = QueryResult {
        columns: names.iter().map(|n| ColumnInfo { name: n.to_string(), type_name: "TEXT".into() }).collect(),
        rows,
        rows_affected: 0,
        truncated: false,
        elapsed_ms: 0,
        has_result_set: true,
        in_transaction: false,
        warning: None,
        editable: None,
    };
    Some(Grid::new(result))
}

impl App {
    /// The data source's Redis tab (one per source): opened, or switched to `db` (and `key`)
    pub fn open_redis(&mut self, conn: &str, db: String, key: Option<String>) {
        let id = match self.tabs.iter().position(|t| t.conn == conn && matches!(t.kind, TabKind::Redis(_))) {
            Some(i) => {
                self.activate(i);
                let id = self.tabs[i].id;
                self.redis_switch_db(id, db);
                id
            }
            None => {
                let name = self.conn(conn).map(|c| c.config.name.clone()).unwrap_or_default();
                let id = self.push_tab(conn, db.clone(), format!("{name} [{db}]"), TabKind::Redis(RedisTab::new("", key.clone())));
                self.redis_scan(id, true);
                id
            }
        };
        if let Some(k) = key {
            if let Some(t) = self.tab_mut(id)
                && let TabKind::Redis(r) = &mut t.kind
                && !r.reveal(&k)
            {
                r.want = Some(k.clone());
            }
            self.redis_load_key(id, k);
        }
    }

    /// A click on a Redis database in the explorer: switches the source's open Redis tab to it (opens none)
    pub fn redis_db_clicked(&mut self, conn: &str, db: String) {
        if self.tabs.iter().any(|t| t.conn == conn && matches!(t.kind, TabKind::Redis(_))) {
            self.open_redis(conn, db, None);
        }
    }

    /// The tab shows another database: its own keys, from the top
    pub fn redis_switch_db(&mut self, id: u64, db: String) {
        let name = self.tab_mut(id).and_then(|t| (t.db != db).then(|| t.conn.clone())).and_then(|c| self.conn(&c).map(|c| c.config.name.clone()));
        let Some(name) = name else { return };
        let Some(t) = self.tab_mut(id) else { return };
        t.target = sql::target_id(&t.conn, Some(&db));
        t.title = format!("{name} [{db}]");
        t.db = db;
        if let TabKind::Redis(r) = &mut t.kind {
            *r = RedisTab { dbs: std::mem::take(&mut r.dbs), ..RedisTab::new(&r.pattern, None) };
        }
        self.redis_scan(id, true);
    }

    /// d: pick another database of the server (with key counts)
    pub fn redis_pick_db(&mut self, id: u64) {
        let Some(t) = self.tab_mut(id) else { return };
        let target = t.target.clone();
        let st = self.state.clone();
        self.status = "Reading databases…".into();
        self.spawn(async move { st.redis_overview(target).await.map_err(err) }, move |a, r| match r {
            Ok(o) => a.redis_db_choices(id, o.databases),
            Err(e) => a.status = e,
        });
    }

    /// d's list: the databases with their key counts ("db0 · 500 keys"), the current one selected (Enter keeps it)
    pub(crate) fn redis_db_choices(&mut self, id: u64, dbs: Vec<qurry_core::redisdb::RedisDb>) {
        let Some(t) = self.tab_mut(id) else { return };
        let cur = t.db.clone();
        if let TabKind::Redis(r) = &mut t.kind {
            r.dbs = dbs.clone();
        }
        let sel = dbs.iter().position(|d| d.name == cur).unwrap_or(0);
        let items = dbs.iter().map(|d| PItem { label: qurry_shared::redis_cmd::db_label(&d.name, d.keys), hint: if d.name == cur { "current".into() } else { String::new() }, action: PAction::Pick(d.name.clone()) }).collect();
        self.status.clear();
        self.flow = Some(crate::migrate::Flow::RedisDb(id));
        self.open_after_notices(Popup::Palette(Palette { title: "Show which database?".into(), input: TextArea::default(), items, sel, loading: 0, cache: Default::default() }));
    }

    pub fn redis_scan(&mut self, id: u64, reset: bool) {
        let Some(t) = self.tab_mut(id) else { return };
        let target = t.target.clone();
        let TabKind::Redis(r) = &mut t.kind else { return };
        r.scanning = true;
        let (cursor, pattern) = (if reset { "0".to_string() } else { r.cursor.clone() }, r.pattern.clone());
        let st = self.state.clone();
        if reset {
            // The title's key count and d's list
            let (st, target) = (st.clone(), target.clone());
            self.spawn(async move { st.redis_overview(target).await.map_err(err) }, move |a, r| {
                if let (Ok(o), Some(TabKind::Redis(t))) = (r, a.tab_mut(id).map(|t| &mut t.kind)) {
                    t.dbs = o.databases;
                }
            });
        }
        let asked = target.clone();
        self.spawn(async move { st.redis_scan(target, cursor, pattern, 500, None).await.map_err(err) }, move |a, res| {
            // The tab switched databases meanwhile: these keys are the old one's
            let Some(t) = a.tab_mut(id).filter(|t| t.target == asked) else { return };
            let TabKind::Redis(r) = &mut t.kind else { return };
            r.scanning = false;
            match res {
                Ok(page) => {
                    if reset {
                        r.keys.clear();
                        r.sel = 0;
                        r.off = 0;
                    }
                    let mut seen: HashSet<String> = r.keys.iter().map(|k| k.key.clone()).collect();
                    for k in page.keys {
                        if seen.insert(k.key.clone()) {
                            r.keys.push(k);
                        }
                    }
                    r.keys.sort_by(|a, b| a.key.cmp(&b.key));
                    r.sel = r.sel.min(r.rows().len().saturating_sub(1));
                    r.cursor = page.cursor;
                    let n = r.keys.len();
                    let more = r.cursor != "0";
                    let first = match r.want.take() {
                        Some(w) => {
                            r.reveal(&w);
                            None
                        }
                        // If the first row is a key, show it right away (if a folder, expand it to pick)
                        None if reset && r.detail.is_none() => r.sel_key(),
                        None => None,
                    };
                    a.status = format!("{n}{} keys", if more { "+ (m: scan more)" } else { "" });
                    if let Some(k) = first {
                        a.redis_load_key(id, k);
                    }
                }
                Err(e) => a.status = e,
            }
        });
    }

    pub fn redis_load_key(&mut self, id: u64, key: String) {
        let Some(t) = self.tab_mut(id) else { return };
        let target = t.target.clone();
        let st = self.state.clone();
        let (k2, asked) = (key.clone(), target.clone());
        self.spawn(async move { st.redis_key(target, k2, Some(1000)).await.map_err(err) }, move |a, res| {
            // A key of the database the tab showed before: its edits would go to the new one
            let Some(t) = a.tab_mut(id).filter(|t| t.target == asked) else { return };
            let TabKind::Redis(r) = &mut t.kind else { return };
            match res {
                Ok(d) => {
                    r.value = redis_grid(&d);
                    r.text = d.value.as_str().map(|s| match serde_json::from_str::<Value>(s) {
                        Ok(v) if v.is_object() || v.is_array() => serde_json::to_string_pretty(&v).unwrap_or_default(),
                        _ => s.to_string(),
                    });
                    r.detail = Some((key, d));
                }
                Err(e) => {
                    r.detail = None;
                    r.value = None;
                    a.status = e;
                }
            }
        });
    }

    /// `:` — the command line (the GUI's Terminal): completion and argument hints from the shared command table, key names from a
    /// bounded SCAN, ↑↓ through this data source's history
    pub fn redis_prompt(&mut self, id: u64) {
        let Some((conn, db)) = self.tab_mut(id).map(|t| (t.conn.clone(), t.db.clone())) else { return };
        let hist = self.state.history.list(Some(&conn), "", 200).into_iter().map(|h| h.sql).collect();
        self.input(format!("{db}>"), "", move |a, line| a.redis_run_line(id, line));
        if let Some(Popup::Input { redis, .. }) = &mut self.popup {
            *redis = Some(RedisPrompt { tab: id, hist, hi: None, keys: None, moved: false });
        }
    }

    /// The command line's completion for the text before the caret: commands (with their arguments), or key names once read
    /// (None asks for them: redis_key_names)
    pub fn redis_complete(input: &TextArea, p: &RedisPrompt) -> (Option<Completion>, Option<String>) {
        let line = input.lines().first().cloned().unwrap_or_default();
        let caret = line.char_indices().nth(input.cursor().1).map_or(line.len(), |(i, _)| i);
        let a = qurry_shared::redis_cmd::assist(&line[..caret]);
        let prefix = a.prefix.chars().count();
        let items: Vec<Item> = if !a.commands.is_empty() {
            a.commands
                .iter()
                .map(|c| Item { label: c.doc.name.into(), detail: [c.doc.args, c.doc.summary].iter().filter(|s| !s.is_empty()).copied().collect::<Vec<_>>().join(" — "), insert: format!("{} ", c.insert), kind: complete::ItemKind::Keyword })
                .collect()
        } else if a.key {
            match &p.keys {
                Some((pre, names)) if *pre == a.prefix => qurry_shared::redis_cmd::key_choices(&a.prefix, names)
                    .into_iter()
                    .map(|k| Item { label: k.label, detail: String::new(), insert: format!("{} ", k.insert), kind: complete::ItemKind::Value })
                    .collect(),
                _ => return (None, Some(a.prefix)),
            }
        } else {
            vec![]
        };
        ((!items.is_empty()).then_some(Completion { items, sel: 0, prefix }), None)
    }

    /// Key names for the command line's completion (a bounded SCAN); shown if the line still asks for the same prefix
    pub fn redis_key_names(&mut self, id: u64, prefix: String) {
        let Some(target) = self.tab_mut(id).map(|t| t.target.clone()) else { return };
        let st = self.state.clone();
        let p2 = prefix.clone();
        self.spawn(async move { st.redis_key_names(target, p2, 50).await.map_err(err) }, move |a, r| {
            let Ok(names) = r else { return };
            if let Some(Popup::Input { input, redis: Some(p), complete, .. }) = &mut a.popup {
                p.keys = Some((prefix, names));
                if let (c, None) = App::redis_complete(input, p) {
                    *complete = c;
                }
            }
        });
    }

    /// A line from the command line. Same rules as the console: HELP answers from the command table, a read-only source runs reads
    /// only, FLUSHALL / FLUSHDB / DEL … always ask, Production asks before any write (one question for both)
    pub fn redis_run_line(&mut self, id: u64, line: String) {
        use qurry_core::store::DbKind::Redis;
        let line = line.trim().to_string();
        let Some((conn, db)) = self.tab_mut(id).map(|t| (t.conn.clone(), t.db.clone())).filter(|_| !line.is_empty()) else { return };
        let Some(cfg) = self.conn(&conn).map(|c| c.config.clone()) else { return };
        // This source's history (the core masks AUTH / HELLO … AUTH / requirepass); not for a cancelled line
        let remember = move |a: &mut App, line: &str| {
            let _ = a.state.history.add(&conn, Some(&db), line);
        };
        if let Some(h) = qurry_shared::redis_cmd::help(&line) {
            remember(self, &line);
            return self.notice(line, h);
        }
        let write = !qurry_core::guard::is_read(&line, Redis);
        if cfg.read_only && write {
            remember(self, &line);
            let cmd = line.split_whitespace().next().unwrap_or_default().to_uppercase();
            return self.notice(line, format!("(error) \"{}\" is read-only: {cmd} changes data. A read-only data source runs read commands only (data source form ▸ Read-only).", cfg.name));
        }
        let why = qurry_core::guard::unsafe_statement(&line, Redis);
        let prod = cfg.options.confirm_changes && write;
        let run = move |a: &mut App, line: String| {
            remember(a, &line);
            a.redis_send(id, line, write);
        };
        if why.is_none() && !prod {
            return run(self, line);
        }
        let head = if prod { format!("Production: {} — ", cfg.name) } else { String::new() };
        let text = match why {
            Some(why) => format!("{head}{why}:\n  {line}\nRun it anyway?"),
            None => format!("{head}this command changes data:\n  {line}\nRun it?"),
        };
        self.popup = Some(Popup::Confirm { opened: None, text, on_yes: Box::new(move |a| run(a, line)) });
    }

    /// Runs the line; the reply (by type: a hash as a field / value table …) in a notice. A write reads the keys again
    fn redis_send(&mut self, id: u64, line: String, write: bool) {
        let Some(target) = self.tab_mut(id).map(|t| t.target.clone()) else { return };
        let st = self.state.clone();
        let l2 = line.clone();
        self.spawn(async move { st.redis_exec_line(target, l2).await.map_err(|e| e.to_string()) }, move |a, r| {
            match r {
                Ok(v) => a.notice(line.clone(), crate::ui::format_typed(&line, &v)),
                Err(e) => a.notice(line, format!("(error) {}", e.trim_start_matches("Redis: "))),
            }
            if write {
                a.redis_scan(id, true);
            }
        });
    }

    /// Re-read keys after running a Redis command
    pub fn redis_cmd(&mut self, id: u64, args: Vec<String>, rescan: bool) {
        if self.tab_mut(id).is_none() {
            return;
        }
        if self.conn(&self.tabs[self.tab_index(id).unwrap()].conn).map(|c| c.config.read_only).unwrap_or(false) {
            self.status = "Data source is read-only".into();
            return;
        }
        let t = self.tab_mut(id).unwrap();
        let target = t.target.clone();
        let key = match &t.kind {
            TabKind::Redis(r) => r.detail.as_ref().map(|d| d.0.clone()),
            _ => None,
        };
        let st = self.state.clone();
        let shown = args.join(" ");
        self.spawn(async move { st.redis_exec(target, args).await.map_err(err) }, move |a, res| match res {
            Ok(v) => {
                a.status = format!("{shown} → {}", crate::ui::format_reply(&v).lines().next().unwrap_or(""));
                if rescan {
                    a.redis_scan(id, true);
                }
                if let Some(k) = key {
                    a.redis_load_key(id, k);
                }
            }
            Err(e) => a.status = e,
        });
    }
}
