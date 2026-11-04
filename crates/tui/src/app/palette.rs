use super::*;
use crate::theme::Theme;
use qurry_core::format::{align, Align, ValueClass};

/// The GUI's names (Settings › Appearance › Alignment, src/lib/gridAlign.ts VALUE_CLASSES)
fn class_name(c: ValueClass) -> &'static str {
    match c {
        ValueClass::String => "String",
        ValueClass::Number => "Number",
        ValueClass::DateTime => "Date / time",
        ValueClass::Boolean => "Boolean",
        ValueClass::Null => "NULL",
        ValueClass::JsonBinary => "JSON / binary",
    }
}

fn align_name(a: Align) -> &'static str {
    match a {
        Align::Left => "Left",
        Align::Center => "Center",
        Align::Right => "Right",
    }
}

fn channel_name(c: qurry_core::update::Channel) -> &'static str {
    match c {
        qurry_core::update::Channel::Stable => "Stable",
        qurry_core::update::Channel::Beta => "Beta",
    }
}

fn object_items(conn: &str, name: &str, db: &str, es: Vec<IndexEntry>, tag: &str) -> Vec<PItem> {
    es.into_iter()
        .map(|e| PItem {
            label: e.name.clone(),
            hint: format!("{} {} · {name}/{db}{tag}", e.kind, if e.kind == "schema" { String::new() } else { e.schema.clone() }),
            action: PAction::Object { conn: conn.into(), db: db.into(), e: IndexEntryLite { schema: e.schema, name: e.name, kind: e.kind } },
        })
        .collect()
}

/// Column → picking it opens its table (Ctrl+B opens that table's DDL)
fn column_items(conn: &str, name: &str, db: &str, ts: Vec<qurry_core::meta::CompletionTable>, tag: &str) -> Vec<PItem> {
    ts.into_iter()
        .flat_map(|t| {
            t.columns.into_iter().map(move |(col, ty)| PItem {
                label: col,
                hint: format!("column {ty} · {}.{} · {name}/{db}{tag}", t.schema, t.name),
                action: PAction::Object { conn: conn.into(), db: db.into(), e: IndexEntryLite { schema: t.schema.clone(), name: t.name.clone(), kind: "table".into() } },
            })
        })
        .collect()
}

impl App {
    // ───────── Popup: palette ─────────

    pub fn palette_actions(&mut self) {
        let items = ACTIONS
            .iter()
            .filter(|d| d.action != Action::Palette)
            .map(|d| PItem { label: title(d.action).to_string(), hint: self.keys.hint(d.action, d.hint), action: PAction::Act(d.action) })
            .collect();
        self.popup = Some(Popup::Palette(Palette { title: "Actions".into(), input: TextArea::default(), items, sel: 0, loading: 0, cache: Default::default() }));
    }

    pub fn palette_tabs(&mut self) {
        // Recently viewed tabs → then not-yet-viewed tabs (restored but never selected)
        let seen = self.mru.iter().filter_map(|id| self.tabs.iter().find(|t| t.id == *id));
        let rest = self.tabs.iter().filter(|t| !self.mru.contains(&t.id));
        let items = seen.chain(rest).map(|t| PItem { label: self.tab_label(t), hint: t.db.clone(), action: PAction::Tab(t.id) }).collect();
        self.popup = Some(Popup::Palette(Palette { title: "Recent Tabs".into(), input: TextArea::default(), items, sel: 1, loading: 0, cache: Default::default() }));
    }

    /// Search Everywhere (the GUI's ⇧⇧): recent tabs + actions + DB objects in one list
    pub fn palette_everywhere(&mut self) {
        let mut items: Vec<PItem> = self.mru.iter().filter_map(|id| self.tabs.iter().find(|t| t.id == *id)).map(|t| PItem { label: self.tab_label(t), hint: format!("tab · {}", t.db), action: PAction::Tab(t.id) }).collect();
        items.extend(ACTIONS.iter().filter(|d| d.action != Action::SearchEverywhere).map(|d| {
            let key = self.keys.hint(d.action, d.hint);
            PItem { label: title(d.action).to_string(), hint: format!("action · {key}"), action: PAction::Act(d.action) }
        }));
        self.palette_objects("Search Everywhere", items);
    }

    /// Go to Database Object: default DB of connected Data Sources + expanded DBs
    /// Go to Database Object / Search Everywhere: like the GUI, objects of the visible DBs (at most 8) of each connected Data Source.
    /// Redis and single DB: default DB only
    pub fn palette_objects(&mut self, title: &str, items: Vec<PItem>) {
        let conns: Vec<ConnectionConfig> = self.conns.iter().filter(|c| c.connected).map(|c| c.config.clone()).collect();
        // Unconnected Data Sources use the list read last time (connects when picked)
        let mut items = items;
        let columns = title == "Search Everywhere";
        for c in self.state.cached_index().unwrap_or_default() {
            let (conn, db) = qurry_core::app::split_target(&c.target);
            let Some(cv) = self.conns.iter().find(|x| x.config.id == conn && !x.connected) else { continue };
            let (name, db) = (cv.config.name.clone(), db.unwrap_or_default().to_string());
            items.extend(object_items(conn, &name, &db, c.objects, " (cached)"));
            if columns {
                items.extend(column_items(conn, &name, &db, c.columns, " (cached)"));
            }
        }
        let empty = conns.is_empty() && items.iter().all(|i| !matches!(i.action, PAction::Object { .. }));
        self.popup = Some(Popup::Palette(Palette { title: title.into(), input: TextArea::default(), items, sel: 0, loading: conns.len(), cache: Default::default() }));
        if empty {
            self.status = "No connected data sources — connect one (Enter in the explorer) to search its objects".into();
        }
        for cfg in conns {
            let def = sql::default_db(&cfg);
            if cfg.kind == DbKind::Redis || cfg.options.single_db {
                self.index_into_palette(&cfg, vec![def]);
                continue;
            }
            let st = self.state.clone();
            let id = cfg.id.clone();
            self.spawn(async move { st.list_databases(id).await.map_err(err) }, move |a, r: Result<Vec<String>, String>| {
                let mut dbs = r.map(|dbs| qurry_core::app::visible_dbs(&cfg, dbs, &def)).unwrap_or_default();
                // Default DB first (it's searched even if the DB list can't be read)
                dbs.retain(|d| d != &def);
                dbs.insert(0, def);
                dbs.truncate(8);
                a.index_into_palette(&cfg, dbs);
            });
        }
    }

    /// Add DB objects to the open palette (turns one Data Source's loading count into the number of DBs)
    pub(crate) fn index_into_palette(&mut self, cfg: &ConnectionConfig, dbs: Vec<String>) {
        // Search Everywhere also finds columns (the GUI's All / Symbols)
        let columns = match self.popup_under() {
            Some(Popup::Palette(p)) => {
                p.loading = (p.loading + dbs.len()).saturating_sub(1);
                p.title == "Search Everywhere" && cfg.kind != DbKind::Redis
            }
            _ => return,
        };
        for db in dbs {
            if columns {
                self.columns_into_palette(cfg, db.clone());
            }
            let st = self.state.clone();
            let target = sql::target_id(&cfg.id, Some(&db));
            let (conn, name) = (cfg.id.clone(), cfg.name.clone());
            self.spawn(async move { st.object_index(target).await.map_err(err) }, move |a, r: Result<Vec<IndexEntry>, String>| {
                if let Err(e) = &r {
                    a.status = format!("{name}/{db}: {e}");
                }
                if let Some(Popup::Palette(p)) = a.popup_under() {
                    p.loading = p.loading.saturating_sub(1);
                    if let Ok(es) = r {
                        p.items.extend(object_items(&conn, &name, &db, es, ""));
                    }
                }
            });
        }
    }

    /// Column → picking it opens its table (Ctrl+B opens that table's DDL)
    fn columns_into_palette(&mut self, cfg: &ConnectionConfig, db: String) {
        let st = self.state.clone();
        let target = sql::target_id(&cfg.id, Some(&db));
        let (conn, name) = (cfg.id.clone(), cfg.name.clone());
        self.spawn(async move { st.completion_meta(target).await.map_err(err) }, move |a, r| {
            let (Some(Popup::Palette(p)), Ok(ts)) = (a.popup_under(), r) else { return };
            p.items.extend(column_items(&conn, &name, &db, ts, ""));
        });
    }

    pub fn palette_matches(p: &Palette) -> Vec<&PItem> {
        let q = p.input.lines().join("");
        let mut cache = p.cache.borrow_mut();
        if !cache.as_ref().is_some_and(|(cq, n, _)| *cq == q && *n == p.items.len()) {
            // Columns go after tables·actions with the same name (same as the GUI's All)
            let mut v: Vec<(i64, usize)> = p.items.iter().enumerate().filter_map(|(i, it)| fuzzy(&q, &it.label).map(|s| (s - if it.hint.starts_with("column ") { 50 } else { 0 }, i))).collect();
            if !q.is_empty() {
                v.sort_by(|a, b| b.0.cmp(&a.0));
            }
            *cache = Some((q, p.items.len(), v.into_iter().take(300).map(|x| x.1).collect()));
        }
        cache.as_ref().map(|c| c.2.iter().map(|&i| &p.items[i]).collect()).unwrap_or_default()
    }

    /// A notice to acknowledge. Over a popup that is open (the palette, a confirmation, the Production Log, another notice) it
    /// doesn't replace it: closing the notice goes back there, as it was
    pub fn notice(&mut self, title: String, text: String) {
        self.popup = Some(match self.popup.take() {
            Some(back) => Popup::Note { title, text, back: Box::new(back) },
            None => Popup::Message { title, text },
        });
    }

    /// A popup that an async result opens (a confirmation after the row counts, a picker after a load). Under a notice it goes
    /// below the notices (taking the place of what was there), so a notice such as a failed Commit is read first, not replaced.
    /// A confirm comes up quiet (`opened`), and while the user works in another popup it waits (App::waiting) instead of replacing it
    pub(crate) fn open_after_notices(&mut self, mut p: Popup) {
        fn below(top: Popup, p: Popup) -> Popup {
            match top {
                Popup::Message { title, text } => Popup::Note { title, text, back: Box::new(p) },
                Popup::Note { title, text, back } => Popup::Note { title, text, back: Box::new(below(*back, p)) },
                _ => p,
            }
        }
        if let Popup::Confirm { opened, .. } = &mut p {
            *opened = Some(std::time::Instant::now());
            let mut under = self.popup.as_ref();
            while let Some(Popup::Note { back, .. }) = under {
                under = Some(back);
            }
            if !matches!(under, None | Some(Popup::Message { .. })) {
                self.status = "A confirmation is waiting: it opens when you close this popup".into();
                return self.waiting.push(p);
            }
        }
        self.popup = Some(match self.popup.take() {
            Some(top) => below(top, p),
            None => p,
        });
    }

    /// The popup the user works in: the open one, or the one under the notices covering it. An async result for that popup (palette
    /// items, a connection test, schemas) goes there, so it isn't lost while a notice is shown
    pub(crate) fn popup_under(&mut self) -> Option<&mut Popup> {
        let mut p = self.popup.as_mut()?;
        while let Popup::Note { back, .. } = p {
            p = back;
        }
        Some(p)
    }

    pub fn run_palette(&mut self, item: PItem, ddl: bool) {
        let shown = self.popup.take();
        match item.action {
            PAction::Act(a) => self.run_action(a),
            PAction::Tab(id) => {
                if let Some(i) = self.tab_index(id) {
                    self.activate(i)
                }
            }
            PAction::Run { stmts, explain } => self.run_stmts(stmts, explain, false),
            PAction::ExportFormat(f) => self.export_dest(f),
            PAction::Dba(r) => self.dba_open(r),
            PAction::InsertSql(q) => self.insert_sql(q),
            PAction::OpenUndo { conn, db, undo, note } => self.open_undo(conn, db, undo, note, shown),
            PAction::Pick(v) => self.flow_pick(v),
            PAction::DbaSql(t) => self.dba_sql(t),
            PAction::DbaExplain => self.dba_explain(),
            PAction::ExportTo(f, clipboard) => self.export_to(f, clipboard),
            PAction::Exec(e) => {
                self.exec = e;
                self.palette_exec();
                // Stay on the selected row
                if let Some(Popup::Palette(p)) = &mut self.popup {
                    p.sel = p.items.iter().position(|i| i.hint == item.hint && i.label.chars().skip(2).eq(item.label.chars().skip(2))).unwrap_or(0);
                }
            }
            PAction::Channel(c) => self.set_update_channel(c),
            PAction::AlignClass(c) => self.palette_align_of(c),
            PAction::Align(c, a) => {
                self.set_grid_align(c, a);
                self.palette_grid_align(c);
            }
            PAction::Theme(t) => self.set_theme(t),
            PAction::Object { conn, db, e } => {
                let kind = match e.kind.as_str() {
                    "function" | "procedure" => "routine".to_string(),
                    k => k.to_string(),
                };
                if ddl && kind != "schema" {
                    return self.open_ddl(&conn, db, e.schema, e.name, kind);
                }
                match kind.as_str() {
                    "table" | "view" | "matview" | "foreign" => self.open_table(&conn, db, e.schema, e.name, kind),
                    "key" => self.open_redis(&conn, db, Some(e.name)),
                    "schema" => {
                        self.explorer = true;
                        self.focus = Focus::Tree;
                        let key = format!("{conn}|{db}|{}", e.schema);
                        self.tree.select_key(&key);
                    }
                    _ => self.open_ddl(&conn, db, e.schema, e.name, kind),
                }
            }
        }
    }

    pub fn input(&mut self, title: impl Into<String>, init: &str, on_ok: impl FnOnce(&mut App, String) + 'static) {
        self.input_words(title, init, vec![], on_ok);
    }

    /// Input with an explanation box under it (answers, reasons); the title stays one line on the border
    pub fn input_with_body(&mut self, title: &str, body: &str, on_ok: impl FnOnce(&mut App, String) + 'static) {
        self.input(title, "", on_ok);
        if let Some(Popup::Input { body: b, .. }) = &mut self.popup {
            *b = body.into();
        }
    }

    /// Single-line input with autocomplete (typed word filters words, ⌃Space / Tab opens it right away)
    pub fn input_words(&mut self, title: impl Into<String>, init: &str, words: Vec<String>, on_ok: impl FnOnce(&mut App, String) + 'static) {
        let mut t = TextArea::new(vec![init.to_string()]);
        t.move_cursor(tui_textarea::CursorMove::End);
        t.set_cursor_line_style(ratatui::style::Style::default());
        self.popup = Some(Popup::Input { title: title.into(), body: String::new(), input: t, on_ok: Box::new(on_ok), words, complete: None });
    }

    /// Autocomplete candidates for the table tab's WHERE / ORDER BY: columns + common keywords
    pub fn table_words(&self, id: u64, order: bool) -> Vec<String> {
        let Some(t) = self.tabs.iter().find(|t| t.id == id) else { return vec![] };
        let kind = self.kind_of(&t.conn);
        let mut w: Vec<String> = match &t.kind {
            TabKind::Table(tt) => tt.grid.as_ref().map(|g| g.result.columns.iter().map(|c| sql::quote_ident(kind, &c.name)).collect()).unwrap_or_default(),
            _ => vec![],
        };
        let kw: &[&str] = if order { &["ASC", "DESC", "NULLS FIRST", "NULLS LAST"] } else { &["AND", "OR", "NOT", "IS NULL", "IS NOT NULL", "IN", "LIKE", "ILIKE", "BETWEEN", "TRUE", "FALSE", "NOW()"] };
        w.extend(kw.iter().map(|s| s.to_string()));
        w
    }

    /// Check for a new version right now (⌃K › Check for Updates)
    pub(super) fn check_update(&mut self) {
        self.status = "Checking for updates…".into();
        let dir = self.state.store.dir();
        let channel = self.state.store.update_channel();
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let r = qurry_core::update::check(&dir, env!("CARGO_PKG_VERSION"), channel, true).await;
            let _ = tx.send(Box::new(move |a: &mut App| match r {
                Ok(r) => a.update_checked(channel, r),
                Err(e) => a.status = e.to_string(),
            }));
        });
    }

    /// A forced check's answer. None also clears the ⬆ badge: the GUI may have switched the shared channel since it was set
    pub fn update_checked(&mut self, channel: qurry_core::update::Channel, r: Option<qurry_core::update::Release>) {
        match r {
            Some(r) => self.show_update(r),
            None => {
                self.update = None;
                self.status = format!("You have the latest {} version ({}).", channel_name(channel).to_lowercase(), env!("CARGO_PKG_VERSION"));
            }
        }
    }

    /// ⌃K › Update Channel…: stable (version tags only) or beta (also every merge to main). Shared with the GUI
    pub fn palette_update_channel(&mut self) {
        use qurry_core::update::Channel;
        let on = self.state.store.update_channel();
        let channels = [(Channel::Stable, "releases only (default)"), (Channel::Beta, "also the pre-release built from every merge to main")];
        // Opens on the saved channel, so Enter keeps it
        let sel = channels.iter().position(|(c, _)| *c == on).unwrap_or(0);
        let items = channels
            .into_iter()
            .map(|(c, hint)| PItem { label: format!("{} {}", if c == on { "●" } else { " " }, channel_name(c)), hint: hint.into(), action: PAction::Channel(c) })
            .collect();
        self.popup = Some(Popup::Palette(Palette { title: "Update Channel".into(), input: TextArea::default(), items, sel, loading: 0, cache: Default::default() }));
    }

    fn set_update_channel(&mut self, channel: qurry_core::update::Channel) {
        self.status = match self.state.store.set_update_channel(channel) {
            Ok(()) => {
                // The ⬆ badge follows the new channel, from the saved check (both channels, no request) as in the GUI
                let cur = env!("CARGO_PKG_VERSION");
                let saved = if qurry_core::update::disabled() { None } else { qurry_core::update::cached(&self.state.store.dir(), cur, channel) };
                self.update = saved.clone().flatten().map(|r| r.version);
                let name = channel_name(channel);
                match saved {
                    Some(Some(r)) => format!("Update channel: {name} — Qurry {} is available — {} › Update Qurry & Restart", r.version, self.keys.label(Action::Palette)),
                    Some(None) => format!("Update channel: {name} — you have the latest {} version ({cur})", name.to_lowercase()),
                    None => format!("Update channel: {name} — {} › Check for Updates looks now", self.keys.label(Action::Palette)),
                }
            }
            Err(e) => format!("Couldn't save the update channel: {e}"),
        };
    }

    /// ⌃K › Grid Alignment…: one row per value class with where it sits (the GUI's Settings › Appearance › Alignment, shared through
    /// settings.json). Reads the file again, so a change made in the GUI since the TUI started shows here and in the grids
    pub fn palette_grid_align(&mut self, sel: ValueClass) {
        self.align = self.state.store.grid_align();
        let items = ValueClass::ALL
            .iter()
            .map(|&c| {
                let (a, def) = (align(c, &self.align), c.default_align());
                let hint = if a == def { "default".to_string() } else { format!("changed — default {}", align_name(def)) };
                PItem { label: format!("{:<14}{:<6}", class_name(c), align_name(a)), hint, action: PAction::AlignClass(c) }
            })
            .collect();
        let sel = ValueClass::ALL.iter().position(|&c| c == sel).unwrap_or(0);
        self.popup = Some(Popup::Palette(Palette { title: "Grid Alignment".into(), input: TextArea::default(), items, sel, loading: 0, cache: Default::default() }));
    }

    /// Left / Center / Right for one value class, opened on where it sits now (Enter keeps it)
    fn palette_align_of(&mut self, class: ValueClass) {
        let on = align(class, &self.align);
        let items = [Align::Left, Align::Center, Align::Right]
            .into_iter()
            .map(|a| PItem { label: format!("{} {}", if a == on { "●" } else { " " }, align_name(a)), hint: if a == class.default_align() { "default".into() } else { String::new() }, action: PAction::Align(class, a) })
            .collect();
        let sel = [Align::Left, Align::Center, Align::Right].iter().position(|&a| a == on).unwrap_or(0);
        self.popup = Some(Popup::Palette(Palette { title: format!("{} Alignment", class_name(class)), input: TextArea::default(), items, sel, loading: 0, cache: Default::default() }));
    }

    /// Save one class's alignment for the GUI and the TUI (the core drops the override when it is the default)
    fn set_grid_align(&mut self, class: ValueClass, a: Align) {
        self.status = match self.state.store.set_grid_align(class, a) {
            Ok(next) => {
                self.align = next;
                format!("Grid alignment: {} on the {} — saved for the GUI too", class_name(class), align_name(a).to_lowercase())
            }
            Err(e) => format!("Couldn't save the grid alignment: {e}"),
        };
    }

    /// ⌃K › Theme…: Auto (light on a light terminal background, from COLORFGBG), Dark or Light. Opened on the current one (Enter keeps it)
    pub fn palette_theme(&mut self) {
        let auto = if self.auto_light { "light now" } else { "dark now" };
        let items = Theme::ALL
            .into_iter()
            .map(|t| {
                let hint = match t {
                    Theme::Auto => format!("follows the terminal background (COLORFGBG): {auto}"),
                    Theme::Dark => "for a dark terminal background".into(),
                    Theme::Light => "for a light terminal background".into(),
                };
                PItem { label: format!("{} {}", if t == self.theme { "●" } else { " " }, t.name()), hint, action: PAction::Theme(t) }
            })
            .collect();
        let sel = Theme::ALL.iter().position(|&t| t == self.theme).unwrap_or(0);
        self.popup = Some(Popup::Palette(Palette { title: "Theme".into(), input: TextArea::default(), items, sel, loading: 0, cache: Default::default() }));
    }

    /// Switch to a theme now and save it in settings.json (a failed save still switches, for this session)
    fn set_theme(&mut self, t: Theme) {
        self.theme = t;
        let colors = if t.light(self.auto_light) { "light" } else { "dark" };
        self.status = match self.state.store.set_tui_theme(&t.name().to_lowercase()) {
            Ok(()) => format!("Theme: {} — {colors} colors", t.name()),
            Err(e) => format!("Couldn't save the theme: {e} — {} for this session only", t.name()),
        };
    }

    /// Status line text at startup: one notice, so it fits the status text's half of an 80-cell line. Auto picking the light
    /// palette comes first, since the colors changed by themselves (NO_COLOR draws no palette, so nothing changed there);
    /// otherwise how to get more colors
    pub fn startup_notice(&self) -> Option<String> {
        if self.theme == Theme::Auto && self.auto_light && self.color != crate::theme::ColorMode::NoColor {
            return Some(format!("Light palette — {} › Theme… › Dark to undo", self.keys.label(Action::Palette)));
        }
        self.color.notice().map(String::from)
    }

    /// New version window: changes since the current version (per version) + how to update
    pub fn show_update(&mut self, r: qurry_core::update::Release) {
        let cur = env!("CARGO_PKG_VERSION");
        let how = format!("{} › Update Qurry & Restart", self.keys.label(Action::Palette));
        let changes = qurry_core::update::changes_text(&r.changes_since(cur));
        // How to update goes at the top (with many versions the bottom of the window gets cut off)
        let text = format!("You have {cur} — {how}\n\n{}", if changes.is_empty() { "See the release page for what changed.\n".into() } else { changes });
        self.status = format!("Qurry {} is available — {how}", r.version);
        self.notice(format!("Qurry {} is available", r.version), text);
        self.update = Some(r.version);
    }

    /// Download the latest release, swap it in, and relaunch when done (open tabs are saved on quit)
    pub(super) fn install_update(&mut self) {
        self.status = "Updating Qurry… (downloading)".into();
        let channel = self.state.store.update_channel();
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let r = crate::selfupdate::install(channel, true).await;
            let _ = tx.send(Box::new(move |a: &mut App| match r {
                Ok(crate::selfupdate::Outcome::Installed { version, .. }) => {
                    a.status = format!("Qurry {version} installed — restarting…");
                    a.restart = true;
                    a.quit = true;
                }
                Ok(crate::selfupdate::Outcome::UpToDate) => a.update_checked(channel, None),
                Ok(crate::selfupdate::Outcome::Skipped(why)) => a.status = why,
                Err(e) => a.status = format!("Update failed: {e}"),
            }));
        });
    }
}
