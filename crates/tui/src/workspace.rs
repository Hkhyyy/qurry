//! 열어 둔 탭·화면 분할을 저장하고 다시 켤 때 되살린다 (DataGrip 처럼). 파일: config/workspace-tui.json (0600)
//! 되살린 탭은 바로 접속하지 않는다: 보이는 탭만 불러오고, 나머지는 처음 고를 때 (Tab.stale)
use crate::app::{App, SplitDir, TabKind};
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

pub fn to_json(app: &App) -> Value {
    let tabs: Vec<Value> = app
        .tabs
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
    let index = |id: Option<u64>| id.and_then(|id| app.tab_index(id));
    json!({
        "version": 1,
        "tabs": tabs,
        "panes": pane_json(&app.panes),
        "active": app.tabs.get(app.active).map(|_| app.active),
        "groupActive": app.group_active.iter().map(|x| index(*x)).collect::<Vec<_>>(),
        "exec": app.exec.to_json(),
    })
}

/// 저장해 둔 탭을 되살린다. 없어진 Data Source 의 탭은 버린다
pub fn restore(app: &mut App, v: &Value) {
    if let Some(e) = v.get("exec") {
        app.exec = crate::exec::ExecSettings::from_json(e);
    }
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
    // 칸마다 보이는 탭을 먼저 불러오고 (활성 탭은 마지막에: 포커스가 거기로)
    let active = saved(v.get("active").and_then(Value::as_u64)).and_then(|id| app.tab_index(id)).unwrap_or(0);
    for g in app.panes.leaves() {
        if let Some(i) = app.group_tab(g) {
            if i != active {
                app.activate(i);
            }
        }
    }
    app.activate(active);
}
