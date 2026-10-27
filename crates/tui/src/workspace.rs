//! 워크스페이스 (Orca 처럼): 각자 탭·화면 분할. Alt+1~9 로 바꾸고, 지금 것이 아닌 것은 탭 그대로 보관한다 (세션·트랜잭션·결과).
//! 열어 둔 탭·화면 분할을 저장하고 다시 켤 때 되살린다 (DataGrip 처럼). 파일: config/workspace-tui.json (0600)
//! 되살린 탭은 바로 접속하지 않는다: 보이는 탭만 불러오고, 나머지는 처음 고를 때 (Tab.stale)
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

/// 워크스페이스 하나의 탭·분할 (지금 것은 app 에서, 나머지는 보관한 것에서)
fn ws_json(name: &str, tabs: &[Tab], panes: &Pane, active: usize, group_active: &[Option<u64>]) -> Value {
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
        "name": name,
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
                ws_json(&w.name, &app.tabs, &app.panes, app.active, &app.group_active)
            } else {
                let s = w.saved.as_ref().unwrap_or(&empty);
                ws_json(&w.name, &s.tabs, &s.panes, s.active, &s.group_active)
            }
        })
        .collect();
    json!({ "version": 2, "current": app.cur_ws, "workspaces": list, "exec": app.exec.to_json() })
}

/// 저장해 둔 워크스페이스·탭을 되살린다 (version 1 = 워크스페이스 하나). 없어진 Data Source 의 탭은 버린다
pub fn restore(app: &mut App, v: &Value) {
    if let Some(e) = v.get("exec") {
        app.exec = crate::exec::ExecSettings::from_json(e);
    }
    let list: Vec<Value> = match v.get("workspaces").and_then(Value::as_array) {
        Some(l) if !l.is_empty() => l.clone(),
        _ => vec![v.clone()],
    };
    let cur = v.get("current").and_then(Value::as_u64).map(|x| x as usize).filter(|i| *i < list.len()).unwrap_or(0);
    app.workspaces = list
        .iter()
        .enumerate()
        .map(|(i, w)| Workspace { name: w.get("name").and_then(Value::as_str).filter(|n| !n.trim().is_empty()).map(String::from).unwrap_or_else(|| if i == 0 { "Default".into() } else { format!("Workspace {}", i + 1) }), saved: None })
        .collect();
    // 지금 것이 아닌 워크스페이스: 되살려서 보관 (보일 때 불러온다)
    for (i, w) in list.iter().enumerate() {
        if i != cur {
            restore_one(app, w, false);
            app.workspaces[i].saved = Some(app.take_live());
        }
    }
    app.cur_ws = cur;
    restore_one(app, &list[cur], true);
}

/// 워크스페이스 하나를 지금 화면으로 되살린다. show = 칸마다 보이는 탭을 불러온다
fn restore_one(app: &mut App, v: &Value, show: bool) {
    let Some(tabs) = v.get("tabs").and_then(Value::as_array) else { return };
    let mut panes = v.get("panes").and_then(pane_from).unwrap_or(Pane::Leaf(0));
    let leaves = panes.leaves();
    // 저장 순번 → 되살린 탭 id
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
    // 탭이 없는 칸은 없앤다
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
    /// 칸마다 보이는 탭을 불러오고 (되살린 탭이면 이때 접속) 활성 탭에 포커스 (마지막에)
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

    /// 지금 화면의 탭·분할을 떼어 낸다 (보관용). 화면은 빈다
    pub fn take_live(&mut self) -> WsSaved {
        let s = WsSaved {
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
        self.tabs = s.tabs;
        self.active = s.active;
        self.mru = s.mru;
        self.panes = s.panes;
        self.group_active = s.group_active;
    }

    /// Alt+1~9 · ⌃K › Switch Workspace: 지금 것은 그대로 보관하고 i 번째로
    pub fn ws_switch(&mut self, i: usize) {
        if i == self.cur_ws || i >= self.workspaces.len() {
            return;
        }
        let live = self.take_live();
        self.workspaces[self.cur_ws].saved = Some(live);
        let s = self.workspaces[i].saved.take().unwrap_or_default();
        self.cur_ws = i;
        self.put_live(s);
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
            a.workspaces.push(Workspace { name: name.into(), saved: Some(WsSaved::default()) });
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

    /// ⌃K › Delete Workspace…: 지금 워크스페이스와 그 탭을 닫는다 (커밋 안 한 변경은 롤백)
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
                a.show_visible();
                a.status = format!("Workspace {} · {}", to + 1, a.workspaces[to].name);
            }),
        });
    }

    fn pending_count_live(&self) -> usize {
        self.tabs.iter().filter(|t| matches!(&t.kind, TabKind::Console(c) if c.in_tx)).count()
    }

    /// ⌃K › Switch Workspace… / Move Tab to Workspace…: 고르기 창
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
                PItem { label: format!("{}{}", w.name, if i == self.cur_ws { "  (current)" } else { "" }), hint: format!("{n} tab(s){}", if i < 9 { format!(" · Alt+{}", i + 1) } else { String::new() }), action: PAction::Pick(i.to_string()) }
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

    /// 고르기 창에서 고른 워크스페이스
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
                    a.workspaces.push(Workspace { name: name.trim().into(), saved: Some(WsSaved::default()) });
                    a.ws_move(id, a.workspaces.len() - 1);
                });
            }
            (Flow::WsMove(id), v) => {
                if let Ok(i) = v.parse() {
                    self.ws_move(id, i);
                }
            }
            _ => {}
        }
    }

    /// 탭을 다른 워크스페이스로 (세션 그대로). 그 워크스페이스의 첫 칸 맨 뒤에, 보이는 탭으로
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
        self.status = format!("Moved the tab to {} (Alt+{})", self.workspaces[to].name, to + 1);
    }
}

#[cfg(test)]
mod tests {
    use crate::testkit::Harness;

    /// "test" Data Source 가 있는 빈 앱 (되살릴 때 없는 Data Source 의 탭은 버리므로)
    async fn harness() -> Harness {
        let mut h = Harness::new().await;
        let cfg = serde_json::from_value(serde_json::json!({ "id": "test", "name": "test", "kind": "postgres", "host": "127.0.0.1", "port": 1, "user": "u", "database": "postgres" })).unwrap();
        h.app.state.save_connection(cfg, None, None).await.unwrap();
        h.app.reload_conns();
        h.settle().await;
        h
    }

    fn titles(h: &Harness) -> Vec<String> {
        h.app.tabs.iter().map(|t| t.title.clone()).collect()
    }

    #[tokio::test]
    async fn workspaces_keep_their_tabs_and_late_results() {
        let mut h = harness().await;
        h.table_tab("orders", &["id"], &[&["1"]], &["id"]);
        // 새 워크스페이스 (이름 그대로 Enter) → 빈 화면
        h.app.ws_new();
        h.key("Enter");
        assert_eq!((h.app.cur_ws, h.app.tabs.len()), (1, 0));
        let users = h.table_tab("users", &["id"], &[&["1"]], &["id"]);
        assert!(h.screen(100, 20).contains("1 Default"), "the workspace strip shows when there are two");
        // Alt+1: 처음 워크스페이스, 탭 그대로
        h.key("Alt+1");
        assert_eq!((h.app.cur_ws, titles(&h)), (0, vec!["orders".to_string()]));
        // 보관된 탭에 늦게 온 결과도 남는다
        h.app.tab_mut(users).unwrap().title = "users (done)".into();
        h.key("Alt+2");
        assert_eq!(titles(&h), ["users (done)"]);
        // 없는 번호는 무시
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
        // items 를 2 번 워크스페이스로 (고르기 창의 첫 줄)
        let i = h.app.tab_index(items).unwrap();
        h.app.activate(i);
        h.app.ws_pick(true);
        h.key("Enter");
        assert_eq!(titles(&h), ["orders"]);
        h.key("Alt+2");
        assert_eq!(titles(&h), ["items"]);
        // 저장 → 새 앱에서 되살리기: 워크스페이스·지금 것·탭
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
        // 예전 형식 (version 1) 은 워크스페이스 하나
        let mut h3 = harness().await;
        super::restore(&mut h3.app, &serde_json::json!({ "version": 1, "tabs": v["workspaces"][0]["tabs"], "panes": { "leaf": 0 } }));
        assert_eq!((h3.app.workspaces.len(), titles(&h3)), (1, vec!["orders".to_string()]));
        // 지우기: 확인하면 옆 워크스페이스로
        h2.app.ws_delete();
        h2.key("Enter");
        assert_eq!(h2.app.workspaces.len(), 1);
        assert_eq!(titles(&h2), ["items"]);
        assert!(!h2.screen(100, 20).contains("1 Reports"), "one workspace: no strip");
    }
}
