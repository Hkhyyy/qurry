//! Workspaces (like Orca): each has its own tabs and splits. Switch with Alt+1~9; the inactive ones keep their tabs as is (sessions, transactions, results).
//! Saves the open tabs and splits and restores them on the next launch (like DataGrip). File: config/workspace-tui.json (0600)
//! Restored tabs do not connect right away: only visible tabs are loaded, the rest when first selected (Tab.stale)
use crate::app::{App, Focus, PAction, PItem, Palette, Popup, SplitDir, Tab, TabKind, Workspace, WsSaved};
use crate::panes::Pane;
use serde_json::{json, Value};

fn pane_json(p: &Pane) -> Value {
    match p {
        Pane::Leaf(g) => json!({ "leaf": g }),
        Pane::Split { dir, pct, a, b } => json!({ "dir": if *dir == SplitDir::Right { "right" } else { "down" }, "pct": pct, "a": pane_json(a), "b": pane_json(b) }),
    }
}

fn pane_from(v: &Value) -> Option<Pane> {
    if let Some(g) = v.get("leaf").and_then(Value::as_u64) {
        return ((g as usize) < crate::panes::MAX).then_some(Pane::Leaf(g as usize));
    }
    let dir = match v.get("dir")?.as_str()? {
        "right" => SplitDir::Right,
        _ => SplitDir::Down,
    };
    let pct = v.get("pct").and_then(Value::as_u64).unwrap_or(50).clamp(15, 85) as u16;
    Some(Pane::Split { dir, pct, a: Box::new(pane_from(v.get("a")?)?), b: Box::new(pane_from(v.get("b")?)?) })
}

/// Tabs and splits of one workspace (the current one from app, the others from storage)
fn ws_json(id: &str, name: &str, conns: &[String], tabs: &[Tab], panes: &Pane, active: usize, group_active: &[Option<u64>]) -> Value {
    let tabs_v: Vec<Value> = tabs
        .iter()
        .map(|t| {
            let kind = match &t.kind {
                TabKind::Console(c) => json!({ "console": c.editor.lines().join("\n") }),
                TabKind::Table(tt) => json!({ "table": { "schema": tt.schema, "name": tt.table, "kind": tt.kind, "where": tt.where_, "order": tt.order } }),
                TabKind::Text(x) => json!({ "text": x.text }),
                TabKind::Dba(d) => json!({ "dba": d.report }),
                TabKind::Redis(r) => json!({ "redis": r.pattern }),
            };
            json!({ "title": t.title, "conn": t.conn, "db": t.db, "group": t.group, "kind": kind })
        })
        .collect();
    let index = |id: Option<u64>| id.and_then(|id| tabs.iter().position(|t| t.id == id));
    json!({
        "id": id,
        "name": name,
        "conns": conns,
        "tabs": tabs_v,
        "panes": pane_json(panes),
        "active": tabs.get(active).map(|_| active),
        "groupActive": group_active.iter().map(|x| index(*x)).collect::<Vec<_>>(),
    })
}

pub fn to_json(app: &App) -> Value {
    let empty = WsSaved::default();
    let list: Vec<Value> = app
        .workspaces
        .iter()
        .enumerate()
        .map(|(i, w)| {
            if i == app.cur_ws {
                ws_json(&w.id, &w.name, &w.conns, &app.tabs, &app.panes, app.active, &app.group_active)
            } else {
                let s = w.saved.as_ref().unwrap_or(&empty);
                ws_json(&w.id, &w.name, &w.conns, &s.tabs, &s.panes, s.active, &s.group_active)
            }
        })
        .collect();
    // shared: has been synced with the workspace list file shared with the GUI (workspaces.json)
    json!({ "version": 2, "shared": true, "current": app.cur_ws, "workspaces": list, "exec": app.exec.to_json() })
}

/// Restore the saved workspaces and tabs (version 1 = a single workspace). Tabs of removed Data Sources are dropped
pub fn restore(app: &mut App, v: &Value) {
    if let Some(e) = v.get("exec") {
        app.exec = crate::exec::ExecSettings::from_json(e);
    }
    let list: Vec<Value> = match v.get("workspaces").and_then(Value::as_array) {
        Some(l) if !l.is_empty() => l.clone(),
        _ => vec![v.clone()],
    };
    let cur = v.get("current").and_then(Value::as_u64).map(|x| x as usize).filter(|i| *i < list.len()).unwrap_or(0);
    // Save files without explorer lists (before 0.2.0, 0.2.0): the first workspace gets all Data Sources
    let has_lists = list.iter().any(|w| w.get("conns").is_some_and(Value::is_array));
    let all: Vec<String> = app.conns.iter().map(|c| c.config.id.clone()).collect();
    app.workspaces = list
        .iter()
        .enumerate()
        .map(|(i, w)| Workspace {
            id: w.get("id").and_then(Value::as_str).filter(|s| !s.is_empty()).map(String::from).unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
            name: w.get("name").and_then(Value::as_str).filter(|n| !n.trim().is_empty()).map(String::from).unwrap_or_else(|| if i == 0 { "Default".into() } else { format!("Workspace {}", i + 1) }),
            conns: if has_lists {
                w.get("conns").and_then(Value::as_array).into_iter().flatten().filter_map(|x| x.as_str().map(String::from)).collect()
            } else if i == 0 {
                all.clone()
            } else {
                vec![]
            },
            saved: None,
        })
        .collect();
    // Inactive workspaces: restore and keep in storage (loaded when shown)
    for (i, w) in list.iter().enumerate() {
        if i != cur {
            restore_one(app, w, false);
            app.workspaces[i].saved = Some(app.take_live());
        }
    }
    app.cur_ws = cur;
    app.ws_synced = v.get("shared").and_then(Value::as_bool).unwrap_or(false);
    restore_one(app, &list[cur], true);
}

/// Restore one workspace into the current screen. show = load the visible tab of each pane
fn restore_one(app: &mut App, v: &Value, show: bool) {
    let Some(tabs) = v.get("tabs").and_then(Value::as_array) else { return };
    let mut panes = v.get("panes").and_then(pane_from).unwrap_or(Pane::Leaf(0));
    let leaves = panes.leaves();
    // Saved index → restored tab id
    let mut ids: Vec<Option<u64>> = vec![];
    for t in tabs {
        let s = |k: &str| t.get(k).and_then(Value::as_str).unwrap_or_default().to_string();
        let conn = s("conn");
        if app.conn(&conn).is_none() {
            ids.push(None);
            continue;
        }
        let group = t.get("group").and_then(Value::as_u64).map(|g| g as usize).filter(|g| leaves.contains(g)).unwrap_or(leaves[0]);
        let k = t.get("kind").cloned().unwrap_or_default();
        let id = if let Some(sql) = k.get("console").and_then(Value::as_str) {
            app.restore_console(&conn, s("db"), s("title"), sql)
        } else if let Some(x) = k.get("table") {
            let g = |k: &str| x.get(k).and_then(Value::as_str).unwrap_or_default().to_string();
            app.restore_table(&conn, s("db"), g("schema"), g("name"), g("kind"), g("where"), g("order"))
        } else if let Some(text) = k.get("text").and_then(Value::as_str) {
            app.restore_text(&conn, s("db"), s("title"), text)
        } else if let Some(r) = k.get("dba").and_then(Value::as_str) {
            app.restore_dba(&conn, s("db"), s("title"), r)
        } else if let Some(p) = k.get("redis").and_then(Value::as_str) {
            app.restore_redis(&conn, s("db"), s("title"), p)
        } else {
            None
        };
        if let Some(id) = id {
            if let Some(i) = app.tab_index(id) {
                app.tabs[i].group = group;
            }
        }
        ids.push(id);
    }
    if app.tabs.is_empty() {
        return;
    }
    // Remove panes without tabs
    for g in panes.leaves() {
        if !app.tabs.iter().any(|t| t.group == g) && panes.leaves().len() > 1 {
            panes.remove(g);
        }
    }
    if !panes.leaves().iter().any(|g| app.tabs.iter().any(|t| t.group == *g)) {
        panes = Pane::Leaf(0);
        for t in &mut app.tabs {
            t.group = 0;
        }
    }
    app.panes = panes;
    let saved = |i: Option<u64>| i.and_then(|i| ids.get(i as usize).copied().flatten());
    if let Some(ga) = v.get("groupActive").and_then(Value::as_array) {
        for (g, x) in ga.iter().enumerate().take(crate::panes::MAX) {
            app.group_active[g] = saved(x.as_u64());
        }
    }
    let active = saved(v.get("active").and_then(Value::as_u64)).and_then(|id| app.tab_index(id)).unwrap_or(0);
    app.active = active;
    if show {
        app.show_visible();
    }
}

impl App {
    /// Load the visible tab of each pane (restored tabs connect now) and focus the active tab (last)
    pub fn show_visible(&mut self) {
        if self.tabs.is_empty() {
            self.focus = Focus::Tree;
            return;
        }
        let active = self.active.min(self.tabs.len() - 1);
        for g in self.panes.leaves() {
            if let Some(i) = self.group_tab(g) {
                if i != active {
                    self.activate(i);
                }
            }
        }
        self.activate(active);
    }

    /// Detach the current screen's tabs and splits (for storage). The screen becomes empty
    pub fn take_live(&mut self) -> WsSaved {
        let s = WsSaved {
            tree: std::mem::take(&mut self.tree),
            tabs: std::mem::take(&mut self.tabs),
            active: self.active,
            mru: std::mem::take(&mut self.mru),
            panes: std::mem::replace(&mut self.panes, Pane::Leaf(0)),
            group_active: std::mem::replace(&mut self.group_active, [None; crate::panes::MAX]),
        };
        self.active = 0;
        s
    }

    fn put_live(&mut self, s: WsSaved) {
        self.tree = s.tree;
        self.tabs = s.tabs;
        self.active = s.active;
        self.mru = s.mru;
        self.panes = s.panes;
        self.group_active = s.group_active;
    }

    /// Alt+1~9 · ⌃K › Switch Workspace: keep the current one as is and go to the i-th
    pub fn ws_switch(&mut self, i: usize) {
        if i == self.cur_ws || i >= self.workspaces.len() {
            return;
        }
        let live = self.take_live();
        self.workspaces[self.cur_ws].saved = Some(live);
        let s = self.workspaces[i].saved.take().unwrap_or_default();
        self.cur_ws = i;
        self.put_live(s);
        self.rebuild_roots();
        self.show_visible();
        self.status = format!("Workspace {} · {} ({} tab(s))", i + 1, self.workspaces[i].name, self.tabs.len());
    }

    /// ⌃K › New Workspace…
    pub fn ws_new(&mut self) {
        let def = format!("Workspace {}", self.workspaces.len() + 1);
        self.input("New workspace name", &def, |a, name| {
            let name = name.trim();
            if name.is_empty() {
                return;
            }
            a.workspaces.push(Workspace { id: uuid::Uuid::new_v4().to_string(), name: name.into(), conns: vec![], saved: Some(WsSaved::default()) });
            a.ws_switch(a.workspaces.len() - 1);
        });
    }

    /// ⌃K › Rename Workspace…
    pub fn ws_rename(&mut self) {
        let cur = self.workspaces[self.cur_ws].name.clone();
        self.input("Rename this workspace", &cur, |a, name| {
            if !name.trim().is_empty() {
                let i = a.cur_ws;
                a.workspaces[i].name = name.trim().into();
            }
        });
    }

    /// ⌃K › Delete Workspace…: close the current workspace and its tabs (uncommitted changes are rolled back)
    pub fn ws_delete(&mut self) {
        if self.workspaces.len() < 2 {
            self.status = "This is the only workspace".into();
            return;
        }
        let i = self.cur_ws;
        let dirty = self.pending_count_live();
        let text = format!(
            "Delete the workspace \"{}\"{}?{}",
            self.workspaces[i].name,
            if self.tabs.is_empty() { String::new() } else { format!(" and close its {} tab(s)", self.tabs.len()) },
            if dirty > 0 { format!("\n\n{dirty} console(s) have uncommitted changes — they are rolled back.") } else { String::new() }
        );
        self.popup = Some(Popup::Confirm {
            text,
            on_yes: Box::new(move |a| {
                let closing = a.take_live();
                a.close_sessions(&closing.tabs);
                a.workspaces.remove(i);
                let to = i.min(a.workspaces.len() - 1);
                let s = a.workspaces[to].saved.take().unwrap_or_default();
                a.cur_ws = to;
                a.put_live(s);
                a.rebuild_roots();
                a.show_visible();
                a.status = format!("Workspace {} · {}", to + 1, a.workspaces[to].name);
            }),
        });
    }

    fn pending_count_live(&self) -> usize {
        self.tabs.iter().filter(|t| matches!(&t.kind, TabKind::Console(c) if c.in_tx)).count()
    }

    /// ⌃K › Switch Workspace… / Move Tab to Workspace…: picker
    pub fn ws_pick(&mut self, move_tab: bool) {
        let tab = if move_tab {
            match self.tabs.get(self.active) {
                Some(t) if self.focus != Focus::Tree => Some(t.id),
                _ => {
                    self.status = "Open the tab to move first".into();
                    return;
                }
            }
        } else {
            None
        };
        let mut items: Vec<PItem> = self
            .workspaces
            .iter()
            .enumerate()
            .filter(|(i, _)| !move_tab || *i != self.cur_ws)
            .map(|(i, w)| {
                let n = if i == self.cur_ws { self.tabs.len() } else { w.saved.as_ref().map(|s| s.tabs.len()).unwrap_or(0) };
                PItem { label: format!("{}{}", w.name, if i == self.cur_ws { "  (current)" } else { "" }), hint: format!("{n} tab(s){}", if i < 9 { format!(" · ⌥{}", i + 1) } else { String::new() }), action: PAction::Pick(i.to_string()) }
            })
            .collect();
        items.push(PItem { label: "New Workspace…".into(), hint: String::new(), action: PAction::Pick("new".into()) });
        self.flow = Some(match tab {
            Some(id) => crate::migrate::Flow::WsMove(id),
            None => crate::migrate::Flow::WsSwitch,
        });
        let title = if move_tab { "Move this tab to workspace" } else { "Switch workspace" };
        self.popup = Some(Popup::Palette(Palette { title: title.into(), input: tui_textarea::TextArea::default(), items, sel: 0, loading: 0, cache: Default::default() }));
    }

    /// Workspace chosen in the picker
    pub fn ws_flow(&mut self, flow: crate::migrate::Flow, value: String) {
        use crate::migrate::Flow;
        match (flow, value.as_str()) {
            (Flow::WsSwitch, "new") => self.ws_new(),
            (Flow::WsSwitch, v) => {
                if let Ok(i) = v.parse() {
                    self.ws_switch(i);
                }
            }
            (Flow::WsMove(id), "new") => {
                let def = format!("Workspace {}", self.workspaces.len() + 1);
                self.input("New workspace name", &def, move |a, name| {
                    if name.trim().is_empty() {
                        return;
                    }
                    a.workspaces.push(Workspace { id: uuid::Uuid::new_v4().to_string(), name: name.trim().into(), conns: vec![], saved: Some(WsSaved::default()) });
                    a.ws_move(id, a.workspaces.len() - 1);
                });
            }
            (Flow::WsMoveConn(conn), "new") => {
                let def = format!("Workspace {}", self.workspaces.len() + 1);
                self.input("New workspace name", &def, move |a, name| {
                    if name.trim().is_empty() {
                        return;
                    }
                    a.workspaces.push(Workspace { id: uuid::Uuid::new_v4().to_string(), name: name.trim().into(), conns: vec![], saved: Some(WsSaved::default()) });
                    let (from, to) = (a.cur_ws, a.workspaces.len() - 1);
                    a.ws_place_conn(&conn, Some(from), to);
                });
            }
            (Flow::WsMoveConn(conn), v) => {
                if let Ok(i) = v.parse() {
                    let from = self.cur_ws;
                    self.ws_place_conn(&conn, Some(from), i);
                }
            }
            (Flow::WsAddConn, id) => {
                let i = self.cur_ws;
                if !self.workspaces[i].conns.iter().any(|c| c == id) {
                    self.workspaces[i].conns.push(id.to_string());
                }
                self.rebuild_roots();
            }
            (Flow::WsMove(id), v) => {
                if let Ok(i) = v.parse() {
                    self.ws_move(id, i);
                }
            }
            _ => {}
        }
    }

    /// Workspace name shown in the form: "2 Reports" (numbered so identical names stay distinct)
    pub fn ws_label(&self, i: usize) -> String {
        format!("{} {}", i + 1, self.workspaces.get(i).map(|w| w.name.as_str()).unwrap_or(""))
    }

    /// "2 Reports" → 1
    pub fn ws_from_label(&self, label: &str) -> Option<usize> {
        label.split_whitespace().next()?.parse::<usize>().ok().and_then(|n| n.checked_sub(1)).filter(|i| *i < self.workspaces.len())
    }

    /// Workspace containing this Data Source (the current one if several, else the first. If none, the current one)
    pub fn ws_of_conn(&self, id: &str) -> usize {
        let has = |i: usize| self.workspaces[i].conns.iter().any(|c| c == id);
        if has(self.cur_ws) {
            return self.cur_ws;
        }
        (0..self.workspaces.len()).find(|i| has(*i)).unwrap_or(self.cur_ws)
    }

    /// Workspace field in the Data Source form: remove from from, add to to (from = None: new Data Source)
    pub fn ws_place_conn(&mut self, id: &str, from: Option<usize>, to: usize) {
        if Some(to) == from || to >= self.workspaces.len() {
            return;
        }
        if let Some(f) = from.filter(|f| *f < self.workspaces.len()) {
            self.workspaces[f].conns.retain(|c| c != id);
        }
        if !self.workspaces[to].conns.iter().any(|c| c == id) {
            self.workspaces[to].conns.push(id.to_string());
        }
        self.rebuild_roots();
        if to != self.cur_ws {
            self.status = format!("Moved to {} — ⌥{} to see it", self.ws_label(to), to + 1);
        }
    }

    /// ⌃K › Add Data Source to Workspace…: pick a Data Source not in this workspace's explorer
    pub fn ws_add_conn_pick(&mut self) {
        let mine = &self.workspaces[self.cur_ws].conns;
        let items: Vec<PItem> = self
            .conns
            .iter()
            .filter(|c| !mine.contains(&c.config.id))
            .map(|c| PItem { label: c.config.name.clone(), hint: format!("{:?}", c.config.kind).to_lowercase(), action: PAction::Pick(c.config.id.clone()) })
            .collect();
        if items.is_empty() {
            self.status = "Every data source is already in this workspace — n in the explorer makes a new one".into();
            return;
        }
        self.flow = Some(crate::migrate::Flow::WsAddConn);
        self.popup = Some(Popup::Palette(Palette { title: "Add to this workspace's explorer".into(), input: tui_textarea::TextArea::default(), items, sel: 0, loading: 0, cache: Default::default() }));
    }

    /// Explorer w: move the selected Data Source to another workspace (picker)
    pub fn ws_move_conn_pick(&mut self) {
        let Some(conn) = self.tree.selected().map(|n| n.conn.clone()) else {
            self.status = "Select a data source in the explorer first".into();
            return;
        };
        let name = self.conn(&conn).map(|c| c.config.name.clone()).unwrap_or_default();
        let mut items: Vec<PItem> = (0..self.workspaces.len())
            .filter(|i| *i != self.cur_ws)
            .map(|i| PItem { label: self.ws_label(i), hint: if self.workspaces[i].conns.contains(&conn) { "already there".into() } else { String::new() }, action: PAction::Pick(i.to_string()) })
            .collect();
        items.push(PItem { label: "New Workspace…".into(), hint: String::new(), action: PAction::Pick("new".into()) });
        self.flow = Some(crate::migrate::Flow::WsMoveConn(conn));
        self.popup = Some(Popup::Palette(Palette { title: format!("Move {name} to workspace"), input: tui_textarea::TextArea::default(), items, sel: 0, loading: 0, cache: Default::default() }));
    }

    /// ⌃K › Remove Data Source from Workspace: remove the Data Source selected in the explorer from this workspace (the Data Source itself stays)
    pub fn ws_remove_conn(&mut self) {
        let Some(conn) = self.tree.selected().map(|n| n.conn.clone()) else {
            self.status = "Select a data source in the explorer first".into();
            return;
        };
        let i = self.cur_ws;
        self.workspaces[i].conns.retain(|c| *c != conn);
        self.rebuild_roots();
        self.status = format!("Removed from {} (the data source itself is kept)", self.workspaces[i].name);
    }

    /// Move a tab to another workspace (session intact). Placed at the end of that workspace's first pane, as the visible tab
    fn ws_move(&mut self, id: u64, to: usize) {
        if to == self.cur_ws || to >= self.workspaces.len() {
            return;
        }
        let Some(i) = self.tab_index(id) else { return };
        let Some(mut t) = self.detach_tab(i) else { return };
        let s = self.workspaces[to].saved.get_or_insert_with(WsSaved::default);
        let g = s.panes.leaves()[0];
        t.group = g;
        s.group_active[g] = Some(t.id);
        s.mru.insert(0, t.id);
        s.tabs.push(t);
        s.active = s.tabs.len() - 1;
        self.status = format!("Moved the tab to {} (⌥{})", self.workspaces[to].name, to + 1);
    }
}

// ───────── Workspace list shared with the GUI (qurry_core::workspaces) ─────────

impl App {
    fn ws_infos(&self) -> Vec<qurry_core::workspaces::WsInfo> {
        self.workspaces.iter().map(|w| qurry_core::workspaces::WsInfo { id: w.id.clone(), name: w.name.clone(), conn_ids: w.conns.clone() }).collect()
    }

    /// On first launch: sync this app's list with the shared list (the first time, pair entries with the same name)
    pub fn ws_adopt_shared(&mut self) {
        let local = self.ws_infos();
        match self.state.store.adopt_workspaces(&local, self.ws_synced) {
            Ok(a) => {
                for (from, to) in &a.renamed {
                    if let Some(w) = self.workspaces.iter_mut().find(|w| &w.id == from) {
                        w.id = to.clone();
                    }
                }
                self.ws_apply_shared(&a.list);
                self.ws_shared = Some(a.list);
            }
            Err(e) => self.status = format!("Workspaces: {e}"),
        }
    }

    /// Write only this app's changes to the shared file (called on every key/event — does nothing if unchanged)
    pub fn ws_push(&mut self) {
        let Some(prev) = &self.ws_shared else { return };
        let next = self.ws_infos();
        if *prev == next {
            return;
        }
        match self.state.store.sync_workspaces(prev, &next) {
            Ok(list) => {
                if list != next {
                    self.ws_apply_shared(&list);
                }
                self.ws_shared = Some(list);
            }
            Err(e) => {
                self.status = format!("Workspaces: {e}");
                self.ws_shared = Some(next);
            }
        }
    }

    /// Pull changes made on the other side (GUI) (every few seconds)
    pub fn ws_pull(&mut self) {
        self.ws_push();
        let Some(prev) = &self.ws_shared else { return };
        let Some(list) = self.state.store.shared_workspaces() else { return };
        if *prev != list {
            self.ws_apply_shared(&list);
            self.ws_shared = Some(list);
        }
    }

    /// Apply the shared list: order · names · Data Sources. Workspaces deleted on the other side close their tabs
    /// (kept if a console has uncommitted changes — it comes back on the next sync)
    fn ws_apply_shared(&mut self, list: &[qurry_core::workspaces::WsInfo]) {
        if list.is_empty() {
            return;
        }
        let cur_id = self.workspaces[self.cur_ws].id.clone();
        let dirty = |a: &App, i: usize| {
            let tabs: &[Tab] = if i == a.cur_ws { &a.tabs } else { a.workspaces[i].saved.as_ref().map(|s| s.tabs.as_slice()).unwrap_or(&[]) };
            tabs.iter().any(|t| matches!(&t.kind, TabKind::Console(c) if c.in_tx))
        };
        // If the current workspace was deleted, detach the screen
        let cur_gone = !list.iter().any(|w| w.id == cur_id) && !dirty(self, self.cur_ws);
        if cur_gone {
            let live = self.take_live();
            self.close_sessions(&live.tabs);
        } else {
            let live = self.take_live();
            self.workspaces[self.cur_ws].saved = Some(live);
        }
        let mut old: Vec<Workspace> = std::mem::take(&mut self.workspaces);
        let mut next: Vec<Workspace> = vec![];
        for w in list {
            let saved = old.iter().position(|o| o.id == w.id).and_then(|i| old[i].saved.take());
            next.push(Workspace { id: w.id.clone(), name: w.name.clone(), conns: w.conn_ids.clone(), saved });
        }
        let mut kept = vec![];
        for o in old.into_iter().filter(|o| !list.iter().any(|w| w.id == o.id)) {
            let has_tx = o.saved.as_ref().is_some_and(|s| s.tabs.iter().any(|t| matches!(&t.kind, TabKind::Console(c) if c.in_tx)));
            if has_tx {
                kept.push(o.name.clone());
                next.push(o);
            } else if let Some(s) = &o.saved {
                self.close_sessions(&s.tabs);
            }
        }
        self.workspaces = next;
        self.cur_ws = self.workspaces.iter().position(|w| w.id == cur_id).unwrap_or(0);
        let s = self.workspaces[self.cur_ws].saved.take().unwrap_or_default();
        self.put_live(s);
        self.rebuild_roots();
        if cur_gone {
            self.show_visible();
            self.status = format!("This workspace was deleted in the GUI — now in {}", self.workspaces[self.cur_ws].name);
        }
        if !kept.is_empty() {
            self.status = format!("Kept workspace {} — it was deleted elsewhere but has uncommitted changes", kept.join(", "));
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::testkit::Harness;

    /// Empty app with a "test" Data Source (restoring drops tabs of missing Data Sources)
    async fn harness() -> Harness {
        let mut h = Harness::new().await;
        let cfg = serde_json::from_value(serde_json::json!({ "id": "test", "name": "test", "kind": "postgres", "host": "127.0.0.1", "port": 1, "user": "u", "database": "postgres" })).unwrap();
        h.app.state.save_connection(cfg, None, None).await.unwrap();
        h.app.reload_conns();
        h.settle().await;
        h
    }

    /// List shared with the GUI: workspaces the GUI creates · renames · deletes come in, and TUI changes do not erase GUI changes
    #[tokio::test]
    async fn workspace_list_is_shared_with_the_gui() {
        use qurry_core::workspaces::WsInfo;
        let mut h = harness().await;
        let st = h.app.state.clone();
        let store = &st.store;
        let shared = store.shared_workspaces().expect("adopted on start");
        assert_eq!(shared.iter().map(|w| (w.name.as_str(), w.conn_ids.clone())).collect::<Vec<_>>(), [("Default", vec!["test".to_string()])]);
        let names = |h: &Harness| h.app.workspaces.iter().map(|w| w.name.clone()).collect::<Vec<_>>();
        // The GUI created Reports
        let mut gui = shared.clone();
        gui.push(WsInfo { id: "g2".into(), name: "Reports".into(), conn_ids: vec!["test".into()] });
        store.sync_workspaces(&shared, &gui).unwrap();
        h.app.ws_pull();
        assert_eq!(names(&h), ["Default", "Reports"]);
        // The TUI renamed and opened a tab, while the GUI removed the Data Source from Reports
        h.table_tab("orders", &["id"], &[&["1"]], &["id"]);
        h.app.workspaces[0].name = "Main".into();
        let gui_prev = store.shared_workspaces().unwrap();
        let mut gui = gui_prev.clone();
        gui[1].conn_ids.clear();
        store.sync_workspaces(&gui_prev, &gui).unwrap();
        h.app.ws_push();
        let now = store.shared_workspaces().unwrap();
        assert_eq!(now.iter().map(|w| (w.name.as_str(), w.conn_ids.len())).collect::<Vec<_>>(), [("Main", 1), ("Reports", 0)]);
        assert_eq!(h.app.workspaces[1].conns.len(), 0, "the GUI change came back");
        assert_eq!(titles(&h), ["orders"], "tabs stay");
        // The GUI deleted the current workspace (Main) → Reports
        let gui: Vec<WsInfo> = now.iter().filter(|w| w.name != "Main").cloned().collect();
        store.sync_workspaces(&now, &gui).unwrap();
        h.app.ws_pull();
        assert_eq!((names(&h), h.app.cur_ws, h.app.tabs.len()), (vec!["Reports".to_string()], 0, 0));
        // The save file keeps id and shared (for pairing on the next launch)
        let v = super::to_json(&h.app);
        assert_eq!((v["shared"].as_bool(), v["workspaces"][0]["id"].as_str()), (Some(true), Some("g2")));
    }

    fn titles(h: &Harness) -> Vec<String> {
        h.app.tabs.iter().map(|t| t.title.clone()).collect()
    }

    #[tokio::test]
    async fn workspaces_keep_their_tabs_and_late_results() {
        let mut h = harness().await;
        h.table_tab("orders", &["id"], &[&["1"]], &["id"]);
        // New workspace (Enter with the default name) → empty screen
        h.app.ws_new();
        h.key("Enter");
        assert_eq!((h.app.cur_ws, h.app.tabs.len()), (1, 0));
        let users = h.table_tab("users", &["id"], &[&["1"]], &["id"]);
        assert!(h.screen(100, 20).contains("1 Default"), "the workspace strip shows when there are two");
        // Alt+1: the first workspace, tabs intact
        h.key("Alt+1");
        assert_eq!((h.app.cur_ws, titles(&h)), (0, vec!["orders".to_string()]));
        // Results that arrive late in stored tabs are kept too
        h.app.tab_mut(users).unwrap().title = "users (done)".into();
        h.key("Alt+2");
        assert_eq!(titles(&h), ["users (done)"]);
        // Missing numbers are ignored
        h.key("Alt+9");
        assert_eq!(h.app.cur_ws, 1);
    }

    #[tokio::test]
    async fn move_tab_save_restore_and_delete() {
        let mut h = harness().await;
        h.table_tab("orders", &["id"], &[&["1"]], &["id"]);
        let items = h.table_tab("items", &["id"], &[&["1"]], &["id"]);
        h.app.ws_new();
        h.key("Enter");
        h.key("Alt+1");
        // Move items to workspace 2 (first line of the picker)
        let i = h.app.tab_index(items).unwrap();
        h.app.activate(i);
        h.app.ws_pick(true);
        h.key("Enter");
        assert_eq!(titles(&h), ["orders"]);
        h.key("Alt+2");
        assert_eq!(titles(&h), ["items"]);
        // Save → restore in a new app: workspaces · current one · tabs
        h.app.ws_rename();
        for _ in 0..20 {
            h.key("Backspace");
        }
        h.type_str("Reports");
        h.key("Enter");
        let v = super::to_json(&h.app);
        assert_eq!(v["version"], 2);
        let mut h2 = harness().await;
        super::restore(&mut h2.app, &v);
        assert_eq!(h2.app.workspaces.iter().map(|w| w.name.as_str()).collect::<Vec<_>>(), ["Default", "Reports"]);
        assert_eq!((h2.app.cur_ws, titles(&h2)), (1, vec!["items".to_string()]));
        h2.key("Alt+1");
        assert_eq!(titles(&h2), ["orders"]);
        // The old format (version 1) is a single workspace
        let mut h3 = harness().await;
        super::restore(&mut h3.app, &serde_json::json!({ "version": 1, "tabs": v["workspaces"][0]["tabs"], "panes": { "leaf": 0 } }));
        assert_eq!((h3.app.workspaces.len(), titles(&h3)), (1, vec!["orders".to_string()]));
        // Delete: on confirm, go to the neighboring workspace
        h2.app.ws_delete();
        h2.key("Enter");
        assert_eq!(h2.app.workspaces.len(), 1);
        assert_eq!(titles(&h2), ["items"]);
        assert!(!h2.screen(100, 20).contains("1 Reports"), "one workspace: no strip");
    }

    fn roots(h: &Harness) -> Vec<String> {
        h.app.tree.roots.iter().map(|n| n.conn.clone()).collect()
    }

    #[tokio::test]
    async fn each_workspace_has_its_own_explorer() {
        let mut h = harness().await;
        assert_eq!(roots(&h), ["test"], "existing data sources start in Default");
        h.app.ws_new();
        h.key("Enter");
        assert!(roots(&h).is_empty(), "a new workspace starts with an empty explorer");
        // Add an existing Data Source
        h.app.ws_add_conn_pick();
        h.key("Enter");
        assert_eq!(roots(&h), ["test"]);
        // Remove it from this workspace only
        h.app.ws_remove_conn();
        assert!(roots(&h).is_empty());
        h.key("Alt+1");
        assert_eq!(roots(&h), ["test"], "Default keeps it");
        // New Data Sources go to the current workspace (created in Alt+2 → there)
        h.key("Alt+2");
        let cfg = serde_json::from_value(serde_json::json!({ "id": "new", "name": "new", "kind": "postgres", "host": "127.0.0.1", "port": 1, "user": "u", "database": "postgres" })).unwrap();
        h.app.state.save_connection(cfg, None, None).await.unwrap();
        h.app.reload_conns();
        h.settle().await;
        assert_eq!(roots(&h), ["new"]);
        h.key("Alt+1");
        assert_eq!(roots(&h), ["test"]);
        // Save → restore: a list per workspace
        let v = super::to_json(&h.app);
        let mut h2 = harness().await;
        h2.app.state.save_connection(serde_json::from_value(serde_json::json!({ "id": "new", "name": "new", "kind": "postgres", "host": "127.0.0.1", "port": 1, "user": "u", "database": "postgres" })).unwrap(), None, None).await.unwrap();
        h2.app.reload_conns();
        h2.settle().await;
        super::restore(&mut h2.app, &v);
        h2.app.rebuild_roots();
        assert_eq!(roots(&h2), ["test"]);
        h2.key("Alt+2");
        assert_eq!(roots(&h2), ["new"]);
        // Old save file (no lists): the first workspace gets everything
        let mut old = v.clone();
        for w in old["workspaces"].as_array_mut().unwrap() {
            w.as_object_mut().unwrap().remove("conns");
        }
        let mut h3 = harness().await;
        super::restore(&mut h3.app, &old);
        h3.app.rebuild_roots();
        assert_eq!(h3.app.workspaces[1].conns, Vec::<String>::new());
        assert_eq!(h3.app.workspaces[0].conns, ["test"]);
    }

    /// Choose the form's Workspace field (◂ ▸ moves one step)
    fn pick_workspace(h: &mut Harness) {
        let Some(crate::app::Popup::ConnForm(f)) = h.app.popup.as_mut() else { panic!("no form") };
        f.sel = f.fields.iter().position(|x| x.label == "Workspace").unwrap();
        h.key("Right");
    }

    #[tokio::test]
    async fn data_source_form_moves_it_between_workspaces() {
        let mut h = harness().await;
        h.app.ws_new();
        h.key("Enter");
        h.key("Alt+1");
        // Edit: Default(1) → 2
        let cfg = h.app.conn("test").unwrap().config.clone();
        h.app.conn_form(Some(cfg));
        let Some(crate::app::Popup::ConnForm(f)) = h.app.popup.as_ref() else { panic!() };
        assert_eq!(f.fields.iter().find(|x| x.label == "Workspace").unwrap().value, "1 Default");
        pick_workspace(&mut h);
        h.key("Enter");
        h.settle().await;
        assert!(roots(&h).is_empty(), "moved out of Default");
        assert_eq!(h.app.workspaces[1].conns, ["test"]);
        h.key("Alt+2");
        assert_eq!(roots(&h), ["test"]);
        // Create: the current one (2) is the default; switch to 1 and create
        h.app.conn_form(None);
        let Some(crate::app::Popup::ConnForm(f)) = h.app.popup.as_mut() else { panic!() };
        assert_eq!(f.fields.iter().find(|x| x.label == "Workspace").unwrap().value, "2 Workspace 2");
        f.fields.iter_mut().find(|x| x.label == "Name").unwrap().value = "fresh".into();
        f.fields.iter_mut().find(|x| x.label == "Host").unwrap().value = "127.0.0.1".into();
        pick_workspace(&mut h);
        h.key("Enter");
        h.settle().await;
        let fresh = h.app.conns.iter().find(|c| c.config.name == "fresh").unwrap().config.id.clone();
        assert_eq!(h.app.workspaces[0].conns, [fresh.clone()]);
        assert!(!h.app.workspaces[1].conns.contains(&fresh), "not adopted into the current workspace");
    }

    #[tokio::test]
    async fn workspace_keys_and_explorer_toggle() {
        let mut h = harness().await;
        // Alt+0 = explorer (Alt+1 is a workspace)
        h.table_tab("t", &["id"], &[&["1"]], &["id"]);
        h.app.focus = crate::app::Focus::Tree;
        h.key("Alt+0");
        assert!(!h.app.explorer, "Alt+0 hides the explorer");
        h.key("Alt+0");
        assert!(h.app.explorer);
        h.app.close_tab_now(0);
        // Alt+N: new workspace (Enter with the default name) · Alt+Shift+N: rename
        h.key("Alt+n");
        h.key("Enter");
        assert_eq!((h.app.workspaces.len(), h.app.cur_ws), (2, 1));
        h.key("Alt+Shift+N");
        for _ in 0..20 {
            h.key("Backspace");
        }
        h.type_str("Ops");
        h.key("Enter");
        assert_eq!(h.app.workspaces[1].name, "Ops");
        // macOS shows ⌥ / ⌃ instead of Alt+ / Ctrl+ (the workspace bar hints used to print them as is)
        if cfg!(target_os = "macos") {
            let scr = h.screen(140, 30);
            assert!(scr.contains("⌥1…9"), "{scr}");
            assert!(!scr.contains("Alt+") && !scr.contains("Ctrl+"), "{scr}");
        }
        // Explorer w: move test from Default to Ops
        h.key("Alt+1");
        h.app.focus = crate::app::Focus::Tree;
        h.app.tree.sel = 0;
        h.key("w");
        h.key("Enter");
        assert!(roots(&h).is_empty());
        assert_eq!(h.app.workspaces[1].conns, ["test"]);
    }
}
