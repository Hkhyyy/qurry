//! Search Everywhere (⌃p) and Go to Database Object (⌃o): the scope (This Workspace, All Places, Context Data Sources) as the GUI's,
//! saved for both apps, and objects of every kind (MySQL as well as PostgreSQL). The sources aren't connected, so their objects come
//! from the search cache, as they would after an earlier run
use crate::app::{PAction, Popup, Workspace};
use crate::testkit::Harness;
use qurry_core::meta::IndexEntry;
use qurry_core::search_cache::SearchScope;

/// "Default" has [PG] app and [MySQL] shop, "Reports" has [PG] reports; each has one table in the search cache
async fn two_workspaces() -> Harness {
    let mut h = Harness::new().await;
    for (id, name, kind, db, table) in [("pg", "[PG] app", "postgres", "app", "accounts"), ("my", "[MySQL] shop", "mysql", "shop", "product_reviews"), ("rep", "[PG] reports", "postgres", "reports", "monthly_totals")] {
        let cfg = serde_json::from_value(serde_json::json!({ "id": id, "name": name, "kind": kind, "host": "127.0.0.1", "port": 1, "user": "u", "database": db })).unwrap();
        h.app.state.store.save(&cfg).unwrap();
        let entry = |name: &str, kind: &str| IndexEntry { schema: if kind == "mysql" { db.into() } else { "public".into() }, name: name.into(), kind: "table".into() };
        qurry_core::search_cache::save_objects(&h.app.state.store.dir(), &format!("{id}::{db}"), &[entry(table, kind)]);
    }
    h.app.reload_conns();
    h.settle().await;
    h.app.workspaces[0].conns = vec!["pg".into(), "my".into()];
    h.app.workspaces.push(Workspace { id: "w2".into(), name: "Reports".into(), conns: vec!["rep".into()], saved: None });
    h.app.rebuild_roots();
    h
}

fn found(h: &Harness) -> Vec<String> {
    let Some(Popup::Palette(p)) = &h.app.popup else { panic!("no palette") };
    let mut v: Vec<String> = p.items.iter().filter(|i| matches!(i.action, PAction::Object { .. })).map(|i| i.label.clone()).collect();
    v.sort();
    v
}

#[tokio::test]
async fn this_workspace_is_the_default_and_finds_mysql_tables() {
    let mut h = two_workspaces().await;
    assert_eq!(h.app.search_scope, SearchScope::Workspace);
    h.key("Ctrl+O");
    assert_eq!(found(&h), ["accounts", "product_reviews"]);
    let s = h.screen(120, 30);
    assert!(s.contains("Go to Database Object · This Workspace"), "{s}");
    assert!(s.contains("⌃o scope"), "{s}");
    let Some(Popup::Palette(p)) = &h.app.popup else { panic!() };
    assert!(p.items.iter().any(|i| i.label == "product_reviews" && i.hint.contains("[MySQL] shop/shop (cached)")));
}

#[tokio::test]
async fn the_key_again_goes_to_all_places_and_it_is_kept() {
    let mut h = two_workspaces().await;
    h.key("Ctrl+P");
    h.type_str("tot");
    // ⌃p again: All Places, with what was typed
    h.key("Ctrl+P");
    assert_eq!(h.app.search_scope, SearchScope::All);
    assert_eq!(found(&h), ["accounts", "monthly_totals", "product_reviews"]);
    let s = h.screen(120, 30);
    assert!(s.contains("Search Everywhere · All Places"), "{s}");
    let Some(Popup::Palette(p)) = &h.app.popup else { panic!() };
    assert_eq!(p.input.lines().join(""), "tot");
    // Saved for the GUI and the next run
    assert_eq!(h.app.state.store.search_scope(), SearchScope::All);
    h.key("Esc");
    h.key("Ctrl+O");
    assert!(h.screen(120, 30).contains("Go to Database Object · All Places"));
}

#[tokio::test]
async fn context_data_sources_is_the_explorer_cursors_source() {
    let mut h = two_workspaces().await;
    h.app.search_scope = SearchScope::All;
    h.key("Ctrl+O");
    h.key("Ctrl+O");
    // The explorer's cursor is on [PG] app
    assert_eq!(h.app.search_scope, SearchScope::Context);
    assert_eq!(found(&h), ["accounts"]);
    assert!(h.screen(120, 30).contains("Go to Database Object · Context Data Sources"));
    // Round again: This Workspace
    h.key("Ctrl+O");
    assert_eq!(h.app.search_scope, SearchScope::Workspace);
    assert_eq!(found(&h), ["accounts", "product_reviews"]);
}

/// Switching workspaces changes what This Workspace covers
#[tokio::test]
async fn this_workspace_follows_the_current_workspace() {
    let mut h = two_workspaces().await;
    h.app.ws_switch(1);
    h.key("Ctrl+O");
    assert_eq!(found(&h), ["monthly_totals"]);
}

/// An object list read for the palette before the scope changed doesn't land in the new one
#[tokio::test]
async fn a_late_list_for_an_earlier_scope_is_dropped() {
    let mut h = two_workspaces().await;
    let cfg = serde_json::from_value(serde_json::json!({ "id": "x", "name": "X", "kind": "postgres", "host": "127.0.0.1", "port": 1, "user": "u" })).unwrap();
    h.key("Ctrl+O");
    h.app.index_into_palette(&cfg, vec!["d".into()]);
    h.key("Ctrl+O");
    let before = found(&h);
    h.settle().await;
    // Nothing listens on port 1: had the answer been taken, the status would say X/d failed
    assert!(!h.app.status.starts_with("X/d"), "{}", h.app.status);
    assert_eq!(found(&h), before);
    let Some(Popup::Palette(p)) = &h.app.popup else { panic!() };
    assert_eq!(p.loading, 0);
}
