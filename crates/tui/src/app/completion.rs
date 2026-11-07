use super::*;

impl App {
    // ───────── Autocomplete ─────────

    /// Drop a Data Source's autocomplete metadata (disconnect·refresh)
    pub fn forget_completion(&mut self, conn: &str) {
        self.completion.retain(|t, _| qurry_core::app::split_target(t).0 != conn);
    }

    pub fn load_completion(&mut self, target: String) {
        if self.completion.contains_key(&target) || !self.completion_loading.insert(target.clone()) {
            return;
        }
        let st = self.state.clone();
        let t2 = target.clone();
        self.spawn(async move { st.completion_meta(t2).await.map_err(err) }, move |a, r| {
            a.completion_loading.remove(&target);
            match r {
                Ok(v) => {
                    a.completion.insert(target, Arc::new(complete::Meta::new(v)));
                    a.update_assist();
                }
                Err(e) => a.status = format!("Completion metadata: {e}"),
            }
        });
    }

    /// Recompute candidates at the cursor position. explicit = Ctrl+Space (otherwise '.' typed·typing with the list open)
    /// Manage Shown Schemas: read the DB list and open the window (db = expand that DB if opened from a DB row)
    pub(super) fn manage_schemas(&mut self, conn: &str, db: Option<String>) {
        self.status = "Loading databases…".into();
        let st = self.state.clone();
        let id = conn.to_string();
        self.spawn(async move { st.list_databases(id).await.map_err(err) }, {
            let conn = conn.to_string();
            move |a, r| match r {
                Ok(dbs) => {
                    a.status.clear();
                    a.open_scope(&conn, dbs, db.as_deref());
                }
                Err(e) => a.status = e,
            }
        });
    }

    pub fn open_scope(&mut self, conn: &str, dbs: Vec<String>, focus_db: Option<&str>) {
        let Some(c) = self.conn(conn) else { return };
        let mut e = crate::scope::ScopeEditor::new(conn.to_string(), dbs.clone(), self.default_db(conn), &c.config.scope, focus_db);
        // Without the scope, the databases the explorer shows are the user ones (system databases hidden)
        let unscoped = qurry_core::store::ConnectionConfig { scope: Default::default(), ..c.config.clone() };
        e.user_dbs = qurry_core::app::visible_dbs(&unscoped, dbs, &e.default_db);
        self.popup = Some(Popup::Scope(e));
        if let Some(db) = focus_db {
            self.load_scope_schemas(db.to_string());
        }
    }

    /// Read schemas of the DB expanded in the window
    pub fn load_scope_schemas(&mut self, db: String) {
        let Some(Popup::Scope(e)) = &self.popup else { return };
        let conn = e.conn.clone();
        let st = self.state.clone();
        let target = sql::target_id(&conn, Some(&db));
        self.spawn(async move { st.list_schemas(target).await.map_err(err) }, move |a, r| {
            if let Some(Popup::Scope(e)) = a.popup_under()
                && e.conn == conn
            {
                e.set_schemas(&db, r);
            }
        });
    }

    /// Save the chosen scope to the Data Source and redraw the tree.
    /// Connection settings are unchanged, so write to the store directly (save_connection closes the pool and drops open transactions)
    pub fn apply_scope(&mut self, e: crate::scope::ScopeEditor) {
        // Change only the scope, under the store lock (so read-only·host etc. changed meanwhile in the GUI·MCP aren't reverted)
        let scope = e.scope();
        let cfg = match self.state.store.modify(&e.conn, |c| {
            c.scope = scope;
            Ok(c.clone())
        }) {
            Ok(Some(cfg)) => cfg,
            Ok(None) => return,
            Err(er) => {
                self.status = er.to_string();
                return;
            }
        };
        if let Some(c) = self.conns.iter_mut().find(|c| c.config.id == cfg.id) {
            c.config = cfg.clone();
        }
        self.status = format!("Shown databases and schemas updated: {}", cfg.name);
        if let Some(root) = self.tree.roots.iter().find(|n| n.conn == cfg.id && n.kind == NodeKind::Conn).cloned() {
            self.refresh_node(&root);
        }
    }

    pub fn update_completion(&mut self, explicit: bool) {
        let Some(t) = self.tabs.get(self.active) else { return };
        let kind = self.kind_of(&t.conn);
        let (target, db) = (t.target.clone(), t.db.clone());
        let meta = self.completion.get(&target).cloned();
        if meta.is_none() {
            self.load_completion(target.clone());
        }
        let Some(TabKind::Console(c)) = self.tabs.get_mut(self.active).map(|t| &mut t.kind) else { return };
        let (row, col) = c.editor.cursor();
        let lines = c.editor.lines();
        let before: String = lines.get(row).map(|l| l.chars().take(col).collect()).unwrap_or_default();
        // Called on every key, so look only at lines around the cursor (don't split a long script every time)
        let (from, to) = (row.saturating_sub(200), (row + 50).min(lines.len()));
        let window = &lines[from..to];
        let text = window.join("\n");
        let at = byte_offset(window, row - from, col);
        let (stmt, caret) = complete::at_caret(&text, at, kind);
        let empty = complete::Meta::new(vec![]);
        // Value · json key spot: use already-read values if any, otherwise fetch and reopen
        if let Some(h) = meta.as_deref().and_then(|m| complete::hint_at(m, &stmt, sql::statement_head(&text, at, kind), &db, kind == DbKind::Mysql)) {
            use complete::HintMode;
            let after = lines.get(row).and_then(|l| l.chars().nth(col));
            let key = |mode: HintMode| format!("{target}\u{1}{}\u{1}{}\u{1}{}\u{1}{}\u{1}{mode:?}", h.schema, h.table, h.column, h.path.join("\u{2}"));
            let hints_cache = &self.hints;
            let fresh = |k: &str| hints_cache.get(k).filter(|(at, _)| at.elapsed() < std::time::Duration::from_secs(120)).map(|(_, v)| v.clone());
            let mut want = (h.mode, fresh(&key(h.mode)));
            // JSON without keys (array of objects etc.): instead of keys, frequent key: value pairs as @> conditions (replacing including the operator)
            if h.mode == HintMode::Keys && want.1.as_ref().is_some_and(|v| v.is_empty()) && kind != DbKind::Mysql {
                want = (HintMode::Pairs, fresh(&key(HintMode::Pairs)));
            }
            match want {
                (mode, Some(hints)) => {
                    let as_pairs = mode == HintMode::Pairs && h.mode == HintMode::Keys;
                    let items = complete::hint_items(&h, &hints, after, as_pairs, Options::TUI);
                    let prefix = if as_pairs { h.op.chars().count() } else { h.typed.chars().count() };
                    c.complete = if items.is_empty() { None } else { Some(Completion { items, sel: 0, prefix }) };
                }
                (mode, None) => {
                    c.complete = None;
                    let k = key(mode);
                    if self.hints_loading.insert(k.clone()) {
                        let st = self.state.clone();
                        let req = qurry_core::hints::HintReq { schema: h.schema, table: h.table, column: h.column, path: h.path, keys: mode == HintMode::Keys, pairs: mode == HintMode::Pairs };
                        self.spawn(async move { st.column_hints(target, req).await.map_err(err) }, move |a, r| {
                            a.hints_loading.remove(&k);
                            match r {
                                Ok(v) => {
                                    a.hints.insert(k, (std::time::Instant::now(), v));
                                    a.update_completion(explicit);
                                }
                                Err(e) => a.status = format!("Value suggestions: {e}"),
                            }
                        });
                    }
                }
            }
            return;
        }
        let (prefix, items) = complete::suggest(meta.as_deref().unwrap_or(&empty), &stmt, caret, &db, kind == DbKind::Mysql, Options::TUI);
        // Auto-open only after '.' or while typing a word (`*` expands into its columns only when asked for)
        let keep = explicit || prefix > 0 && !before.ends_with('*') || before.ends_with('.');
        c.complete = if items.is_empty() || !keep { None } else { Some(Completion { items, sel: 0, prefix }) };
        if explicit && c.complete.is_none() {
            self.status = if meta.is_none() { "Loading completion metadata… try again in a moment".into() } else { "No suggestions".into() };
        }
    }

    /// Replace the word before the cursor with the picked candidate. A snippet (a template, a function, an INSERT's column list)
    /// is written without its markers, its first placeholder selected (typing replaces it) or the caret at its stop
    pub fn accept_completion(&mut self) {
        let Some(TabKind::Console(c)) = self.tabs.get_mut(self.active).map(|t| &mut t.kind) else { return };
        let Some(cmp) = c.complete.take() else { return };
        let Some(item) = cmp.items.get(cmp.sel) else { return };
        for _ in 0..cmp.prefix {
            c.editor.delete_char();
        }
        if !matches!(item.kind, complete::ItemKind::Snippet | complete::ItemKind::Function) {
            c.editor.insert_str(&item.insert);
            return self.update_assist();
        }
        let (text, stops) = complete::snippet_text(&item.insert);
        let (row, col) = c.editor.cursor();
        c.editor.insert_str(&text);
        if let Some(&(a, b)) = stops.first() {
            // A stop's (row, col) from its char offset in the inserted text
            let at = |n: usize| {
                let head: Vec<&str> = text.chars().take(n).collect::<String>().split('\n').map(|_| "").collect();
                let last = text.chars().take(n).collect::<String>().rsplit('\n').next().map_or(0, |l| l.chars().count());
                let r = row + head.len() - 1;
                (r as u16, (if r == row { col + last } else { last }) as u16)
            };
            c.editor.cancel_selection();
            c.editor.move_cursor(tui_textarea::CursorMove::Jump(at(a).0, at(a).1));
            if b > a {
                c.editor.start_selection();
                c.editor.move_cursor(tui_textarea::CursorMove::Jump(at(b).0, at(b).1));
            }
        }
        self.update_assist();
    }

    /// Refresh the console's signature hint and unknown-name marks for the text around the caret (as update_completion, the lines
    /// around it: a long script isn't read on every key). Without loaded metadata, no marks
    pub fn update_assist(&mut self) {
        let Some(t) = self.tabs.get(self.active) else { return };
        let kind = self.kind_of(&t.conn);
        let (target, db) = (t.target.clone(), t.db.clone());
        let meta = self.completion.get(&target).cloned();
        let Some(TabKind::Console(c)) = self.tabs.get_mut(self.active).map(|t| &mut t.kind) else { return };
        let Some(meta) = meta.filter(|_| matches!(kind, DbKind::Postgres | DbKind::Mysql)) else {
            c.assist = Default::default();
            return;
        };
        let (row, col) = c.editor.cursor();
        let lines = c.editor.lines();
        let (from, to) = (row.saturating_sub(200), (row + 50).min(lines.len()));
        let window = &lines[from..to];
        let text = window.join("\n");
        let at = byte_offset(window, row - from, col);
        let mysql = kind == DbKind::Mysql;
        let (stmt, caret) = complete::at_caret(&text, at, kind);
        let sig = complete::signature(&meta, &stmt, caret, mysql);
        // Byte offsets in the window → (row, char columns)
        let starts: Vec<usize> = std::iter::once(0).chain(text.match_indices('\n').map(|(i, _)| i + 1)).collect();
        let names = complete::script_marks(&meta, &text, kind, &db)
            .into_iter()
            .map(|m| {
                let r = starts.partition_point(|&s| s <= m.start) - 1;
                let line = &text[starts[r]..];
                let chars = |b: usize| line[..b - starts[r]].chars().count();
                NameMark { row: from + r, from: chars(m.start), to: chars(m.end), message: m.message, fixes: m.fixes }
            })
            .collect();
        c.assist = Assist { sig, names };
    }

    /// ⌥↩: on an underlined unknown name, its closest names as a list (picking one replaces it); anywhere else Execute, so the
    /// key keeps running statements
    pub fn quick_fix(&mut self) {
        let Some(TabKind::Console(c)) = self.tabs.get_mut(self.active).map(|t| &mut t.kind) else { return self.execute(false) };
        let (row, col) = c.editor.cursor();
        let Some(m) = c.assist.at(row, col).cloned() else { return self.execute(false) };
        if m.fixes.is_empty() {
            self.status = format!("{} — no close names. If it was just created, refresh the data source (its metadata is from when it loaded)", m.message);
            return;
        }
        c.editor.cancel_selection();
        c.editor.move_cursor(tui_textarea::CursorMove::Jump(row as u16, m.to as u16));
        let items = m.fixes.iter().map(|f| Item { label: f.clone(), detail: "fix".into(), insert: f.clone(), kind: complete::ItemKind::Column }).collect();
        c.complete = Some(Completion { items, sel: 0, prefix: m.to - m.from });
        self.focus = Focus::Editor;
    }
}
