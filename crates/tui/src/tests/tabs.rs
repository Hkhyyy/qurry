use crate::app::{Action, PAction, Popup};
use crate::testkit::Harness;

fn active(h: &Harness) -> &str {
    &h.app.tabs[h.app.active].title
}

/// ⌥→ / ⌥← (and ⌃PgDn / ⌃PgUp) cycle the tabs and wrap around at both ends
#[tokio::test]
async fn tab_cycling_wraps() {
    let mut h = Harness::new().await;
    for t in ["a", "b", "c"] {
        h.table_tab(t, &["id"], &[&["1"]], &["id"]);
    }
    assert_eq!(active(&h), "c");
    h.key("Alt+Right");
    assert_eq!(active(&h), "a");
    h.key("Alt+Left");
    assert_eq!(active(&h), "c");
    h.key("Ctrl+PgUp");
    assert_eq!(active(&h), "b");
    h.key("Ctrl+PgDn");
    assert_eq!(active(&h), "c");
}

/// ⌥\ splits off the current tab to the right; ⌃⌥←/→ and ⌥Tab move between panes; cycling tabs stays inside the focused pane; Unsplit merges back
#[tokio::test]
async fn splits_and_pane_focus() {
    let mut h = Harness::new().await;
    for t in ["a", "b", "c"] {
        h.table_tab(t, &["id"], &[&["1"]], &["id"]);
    }
    h.key("Alt+\\");
    assert_eq!(h.app.panes.leaves().len(), 2);
    assert_eq!(active(&h), "c");
    // Pane moves use the drawn layout
    h.screen(160, 20);
    h.key("Ctrl+Alt+Left");
    assert_eq!(active(&h), "b", "the left pane shows the tab next to the one split off");
    h.key("Alt+Right");
    assert_eq!(active(&h), "a", "cycling stays in the left pane (a, b)");
    h.key("Alt+Right");
    assert_eq!(active(&h), "b");
    h.key("Ctrl+Alt+Right");
    assert_eq!(active(&h), "c");
    // Nothing further right: stays, and says so
    h.key("Ctrl+Alt+Right");
    assert_eq!(active(&h), "c");
    assert!(h.app.status.contains("on the right"), "{}", h.app.status);
    h.key("Alt+Tab");
    assert_eq!(active(&h), "b");
    h.key("Alt+Tab");
    assert_eq!(active(&h), "c");
    // New tabs open in the focused pane
    let g = h.app.focused_group();
    h.table_tab("d", &["id"], &[&["1"]], &["id"]);
    assert_eq!(h.app.tabs[h.app.active].group, g);
    h.app.run_action(Action::Unsplit);
    assert_eq!(h.app.panes.leaves().len(), 1);
    assert!(h.app.tabs.iter().all(|t| t.group == 0));
    assert_eq!(active(&h), "d");
}

/// ⌃k lists every command with its key (not the palette itself); typing filters; Enter runs the command
#[tokio::test]
async fn palette_lists_actions_and_runs_one() {
    let mut h = Harness::new().await;
    h.table_tab("a", &["id"], &[&["1"]], &["id"]);
    h.key("Ctrl+K");
    let Some(Popup::Palette(p)) = &h.app.popup else { panic!("no palette") };
    assert_eq!(p.title, "Actions");
    assert_eq!(p.items.len(), crate::app::ACTIONS.len() - 1);
    assert!(!p.items.iter().any(|i| matches!(i.action, PAction::Act(Action::Palette))));
    let item = |label: &str| p.items.iter().find(|i| i.label == label).unwrap_or_else(|| panic!("no {label}"));
    assert_eq!(item("New Query Console").hint, h.app.keys.label(Action::NewConsole));
    assert_eq!(item("Clear Console (delete all text)").hint, "⌘A ⌫ (console)");
    h.type_str("split right");
    let Some(Popup::Palette(p)) = &h.app.popup else { panic!("no palette") };
    let matches: Vec<_> = crate::app::App::palette_matches(p).iter().map(|i| i.label.clone()).collect();
    assert_eq!(matches[0], "Split Right (focused pane, up to 4)", "{matches:?}");
    let s = h.screen(120, 30);
    assert!(s.contains("Split Right"), "{s}");
    h.key("Enter");
    assert!(h.app.popup.is_none());
    assert_eq!(h.app.panes.leaves().len(), 2);
}

/// 16 colors: the focused pane's active tab of a [PROD] Data Source is its own red in reverse video, not the plain
/// selection, so the tab you work in still looks like production; a tab without a color uses the selection
#[tokio::test]
async fn active_tab_keeps_the_data_source_color_in_16_colors() {
    use ratatui::style::{Color, Modifier};
    let mut h = Harness::new().await;
    let cfg = serde_json::from_value(serde_json::json!({ "id": "prod", "name": "[PROD] kr", "kind": "postgres", "host": "127.0.0.1", "port": 1, "user": "u", "database": "shop", "color": "#6e2a2a" })).unwrap();
    h.app.state.save_connection(cfg, None, None).await.unwrap();
    h.app.reload_conns();
    h.settle().await;
    for t in ["payment", "contract"] {
        let id = h.table_tab(t, &["id"], &[&["1"]], &["id"]);
        h.app.tabs.iter_mut().find(|t| t.id == id).unwrap().conn = "prod".into();
    }
    h.table_tab("plain", &["id"], &[&["1"]], &["id"]);
    h.app.color = crate::theme::ColorMode::Ansi16;
    // The first cell of a tab's label in the tab bar
    let label = |h: &mut Harness, name: &str| {
        let buf = h.buffer(120, 20);
        let y = (0..20).find(|&y| (0..120).map(|x| buf[(x, y)].symbol()).collect::<String>().contains("▦ payment")).unwrap();
        let row: String = (0..120).map(|x| buf[(x, y)].symbol()).collect();
        let x = row[..row.find(name).unwrap()].chars().count() as u16;
        buf[(x, y)].clone()
    };
    h.app.activate(1);
    let (on, off) = (label(&mut h, "contract"), label(&mut h, "payment"));
    assert_eq!((on.bg, on.modifier.contains(Modifier::REVERSED | Modifier::BOLD)), (Color::Red, true));
    assert_eq!((off.bg, off.modifier.contains(Modifier::REVERSED)), (Color::Red, false));
    h.app.activate(2);
    let plain = label(&mut h, "plain");
    assert_eq!((plain.bg, plain.modifier.contains(Modifier::REVERSED)), (Color::Reset, true));
}

/// A custom theme's Production tint and NULL color reach the TUI like the GUI: the [PROD] tree row and tab take its prodTint
/// (the active tab a stronger one), NULL its nullText; 16 colors keep the Production red
#[tokio::test]
async fn custom_theme_tints_the_production_row_and_colors_null() {
    use crate::theme::ColorMode;
    use qurry_core::theme::CustomTheme;
    use ratatui::style::Color;
    let mut h = Harness::new().await;
    for (id, name, color) in [("prod", "[PROD] kr", Some("#6e2a2a")), ("dev", "[DEV] kr", None)] {
        let cfg = serde_json::from_value(serde_json::json!({ "id": id, "name": name, "kind": "postgres", "host": "127.0.0.1", "port": 1, "user": "u", "database": "shop", "color": color })).unwrap();
        h.app.state.save_connection(cfg, None, None).await.unwrap();
    }
    h.app.reload_conns();
    h.settle().await;
    // The tree's cursor on the other Data Source (the selection's color would win on the row)
    h.app.tree.sel = h.app.tree.visible().iter().position(|(_, n)| n.conn == "dev").unwrap();
    for t in ["payment", "contract"] {
        let id = h.table_tab(t, &["id", "note"], &[&["1", "x"]], &["id"]);
        let tab = h.app.tabs.iter_mut().find(|t| t.id == id).unwrap();
        tab.conn = "prod".into();
        let crate::app::TabKind::Table(tt) = &mut tab.kind else { unreachable!() };
        tt.grid.as_mut().unwrap().result.rows[0][1] = serde_json::Value::Null;
    }
    let colors = [("prodTint".to_string(), "#5a1e2a".to_string()), ("nullText".to_string(), "#a0a4c0".to_string())].into();
    h.app.themes.custom = vec![CustomTheme { id: "c-prod".into(), name: "Mine".into(), base: "dracula".into(), colors }];
    h.app.themes.theme = Some("c-prod".into());
    h.app.color = ColorMode::TrueColor;
    // The first cell of `text` on the screen
    let at = |h: &mut Harness, text: &str, skip: usize| {
        let buf = h.buffer(120, 20);
        let (x, y) = (0..20)
            .filter_map(|y| {
                let row: String = (0..120).map(|x| buf[(x, y)].symbol()).collect();
                row.find(text).map(|i| (row[..i].chars().count() as u16, y))
            })
            .nth(skip)
            .unwrap_or_else(|| panic!("{text} not on screen"));
        buf[(x, y)].clone()
    };
    let tint = Color::Rgb(0x5a, 0x1e, 0x2a);
    assert_eq!(at(&mut h, "[PROD] kr", 0).bg, tint, "tree row");
    let (off, on) = (at(&mut h, "payment", 0), at(&mut h, "contract", 0));
    assert_eq!(off.bg, tint, "a tab of the Data Source");
    assert!(matches!(on.bg, Color::Rgb(r, ..) if r > 0x5a), "the active tab's stronger tint: {:?}", on.bg);
    assert_eq!(at(&mut h, "<null>", 0).fg, Color::Rgb(0xa0, 0xa4, 0xc0), "NULL in the theme's NULL color");
    h.app.color = ColorMode::Ansi16;
    assert_eq!(at(&mut h, "[PROD] kr", 0).bg, Color::Red, "16 colors keep the Production red");
}

/// The tab bar shows what each tab holds, with the explorer tree's glyph for objects and ≡ (text) after it on a DDL tab: a view's DDL ◫≡, a function's ƒ≡ (#83)
#[tokio::test]
async fn tab_bar_glyphs_follow_the_tree() {
    use crate::app::{TabKind, TextTab};
    let mut h = Harness::new().await;
    h.console_tab();
    h.table_tab("orders", &["id"], &[], &[]);
    let ddl = |text: &str, kind: &str| TabKind::Text(TextTab { text: text.into(), kind: kind.into(), ..Default::default() });
    h.app.push_tab("test", "postgres".into(), "v_users [DDL]".into(), ddl("CREATE VIEW", "view"));
    h.app.push_tab("test", "postgres".into(), "calc [DDL]".into(), ddl("CREATE FUNCTION", "routine"));
    h.app.push_tab("test", "postgres".into(), "Diagram: public".into(), ddl("erd", ""));
    let bar = h.screen(160, 20).lines().find(|l| l.contains("orders")).unwrap().to_string();
    for want in ["❯ console", "▦ orders", "◫≡ v_users [DDL]", "ƒ≡ calc [DDL]", "≡ Diagram: public"] {
        assert!(bar.contains(want), "{want:?} not in {bar:?}");
    }
}

/// Recent Tabs and Search Everywhere put the same glyph before each tab row as the tab bar (#83)
#[tokio::test]
async fn tab_lists_show_the_tab_glyph() {
    use crate::app::{TabKind, TextTab};
    let mut h = Harness::new().await;
    h.table_tab("orders", &["id"], &[], &[]);
    h.app.push_tab("test", "postgres".into(), "v_users [DDL]".into(), TabKind::Text(TextTab { text: "CREATE VIEW".into(), kind: "view".into(), ..Default::default() }));
    h.app.palette_tabs();
    let s = h.screen(140, 30);
    assert!(s.contains("▦ orders") && s.contains("◫≡ v_users [DDL]"), "{s}");
    h.app.popup = None;
    h.app.palette_everywhere();
    let s = h.screen(140, 30);
    assert!(s.contains("▦ orders") && s.contains("◫≡ v_users [DDL]"), "{s}");
}

/// DDL tab, r: reloads what it shows — the DDL text in the SQL view (any object, a view here), the structure in a table's Structure view
#[tokio::test]
async fn r_reloads_the_ddl_or_the_structure() {
    use crate::app::{Focus, TabKind, TextTab};
    let mut h = Harness::new().await;
    let view = TextTab { text: "CREATE VIEW public.v AS SELECT 1;".into(), kind: "view".into(), ddl_of: Some(("public".into(), "v".into())), ..Default::default() };
    h.app.push_tab("test", "postgres".into(), "v [DDL]".into(), TabKind::Text(view));
    h.app.focus = Focus::Grid;
    assert!(h.screen(120, 30).contains("r reload"));
    h.key("r");
    assert_eq!(h.app.status, "Loading DDL of public.v…");
    // No such connection here: the fetch fails, which shows it asked the database again
    h.settle().await;
    assert!(h.app.status.starts_with("DDL failed"), "{}", h.app.status);

    let table = TextTab { text: "CREATE TABLE public.users ();".into(), kind: "table".into(), ddl_of: Some(("public".into(), "users".into())), structure_view: true, structure: Some(Ok(crate::structure::sample())), ..Default::default() };
    let id = h.app.push_tab("test", "postgres".into(), "users [DDL]".into(), TabKind::Text(table));
    h.app.status.clear();
    h.key("r");
    let TabKind::Text(t) = &h.app.tab_mut(id).unwrap().kind else { panic!() };
    assert!(t.structure.is_none(), "the structure is loaded again");
    assert!(h.app.status.is_empty(), "not the DDL: {}", h.app.status);
}

/// r on a DDL tab that doesn't know its object (saved by an older build) says how to get one that does
#[tokio::test]
async fn r_on_a_ddl_without_its_object_says_to_reopen() {
    use crate::app::{Focus, TabKind, TextTab};
    let mut h = Harness::new().await;
    h.app.push_tab("test", "postgres".into(), "v [DDL]".into(), TabKind::Text(TextTab { text: "CREATE VIEW v AS SELECT 1;".into(), kind: "view".into(), ..Default::default() }));
    h.app.focus = Focus::Grid;
    h.key("r");
    assert_eq!(h.app.status, "Reopen the DDL to reload");
}
