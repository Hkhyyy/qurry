//! TUI 키맵 (F1 도움말, `qurry-tui --help`, README 의 기준).
//! 바꿀 수 있는 명령 단축키는 keys.rs 의 DEFAULT_KEYS 에 있고, 여기는 화면별로 고정된 키만 둔다
pub const HELP: &[(&str, &[(&str, &str)])] = &[
    ("Database Explorer", &[
        ("↑ ↓  (j k)", "Move"),
        ("→ / ←  (l h)", "Expand / collapse (← on a child jumps to its parent)"),
        ("Enter / double-click", "Expand / collapse (connects) · open table data · open a Redis key · show DDL of routines"),
        ("F4 / Ctrl+↓", "Open a query console for the data source / database (Redis: the key browser)"),
        ("/", "Filter loaded nodes (/ again edits it · empty or Esc clears)"),
        ("n / e / D", "New / edit / remove data source"),
        ("c / X", "Connect / disconnect"),
        ("r / F5", "Refresh"),
        ("s", "Manage shown databases and schemas (Space check · Enter apply · shared with the GUI)"),
        ("y  ⌘C", "Copy name (OSC 52 clipboard)"),
        ("PgUp / PgDn · Home / End", "Page · first / last node"),
        ("?", "Keymap (same as F1)"),
        ("Tab / Esc", "Go to the open tab"),
    ]),
    ("Query console", &[
        ("completion list", "Opens as you type (or Ctrl+Space) · ↑↓ choose · Enter/Tab insert · Esc close"),
        ("Shift+Tab", "Go to the result grid (Esc / Tab returns)"),
        ("Esc", "Go to the explorer"),
        ("Ctrl+U / Ctrl+R", "Undo / redo (editor)"),
        ("Ctrl+F", "Find / replace bar: Alt+X regex · Alt+C match case · Alt+W words · Enter next (↑ previous) · Tab to Replace: Enter one, Alt+A all ($1 groups) · Esc close"),
        ("⌘A  ⌫", "Select all, then delete (or Ctrl+K › Clear Console)"),
        ("click / drag  ⌘C", "Move the cursor / select text (runs only the selection) · copy the selection"),
        ("mouse drag", "Drag the editor/result border to resize"),
    ]),
    ("Tabs & split (mouse)", &[
        ("drag a tab", "Onto a pane's right / bottom edge = split it (up to 4 panes) · onto another pane or tab bar = move · within a bar = reorder"),
        ("drag the border", "Resize the Database Explorer (its right border) or the panes (the border between them; in a top/bottom split also the empty part of the lower tab bar)"),
        ("click a pane", "Focus it"),
        ("Shift+wheel / trackpad", "Scroll the grid and the explorer sideways (Shift+← / Shift+→ in the explorer)"),
    ]),
    ("Data grid (results & table data)", &[
        ("arrows  (h j k l)", "Move"),
        ("PgUp / PgDn · g / G", "Page · first / last row (more rows load automatically)"),
        ("Home / End", "First / last column"),
        ("Shift+↑↓←→ / drag", "Select a range of cells (Esc clears) · scroll sideways: wheel over the header row, ⌥+wheel, or drag the bar on the bottom border"),
        ("Enter / F2", "Edit cell (table data)"),
        ("n", "Set NULL (table data)"),
        ("a / Insert", "Add row (table data)"),
        ("d / Delete", "Mark row deleted, again = undo (table data)"),
        ("Ctrl+Z", "Revert pending changes (table data)"),
        ("s", "Sort by the selected column: ▲ → ▼ → off (table data: ORDER BY · results: loaded rows)"),
        ("w or / · o", "Edit WHERE · ORDER BY (table data)"),
        ("r", "Reload (table data)"),
        ("x", "Export data (CSV, TSV, JSON, SQL INSERT, Markdown → file or clipboard)"),
        ("y  ⌘C", "Copy cell value (a selected range: as TSV, pastes into spreadsheets as cells)"),
        ("Y", "Copy as an IN list: ('a', 'b', …) — the selected range, or the whole column"),
        ("⌘V", "Paste into cells from the selected one (tabs / lines fill several; extra lines add rows; table data)"),
        ("Esc", "Back (query results: to the editor)"),
    ]),
    ("DDL / routine view", &[
        ("↑ ↓  (j k) · PgUp / PgDn", "Scroll"),
        ("y  ⌘C", "Copy the text"),
        ("Esc", "Back to the explorer"),
    ]),
    ("Redis browser (Medis style)", &[
        ("↑ ↓", "Select key or folder (keys are grouped by \":\")"),
        ("→ / Enter · ←", "Folder: expand · collapse (← on a key: go to its folder)"),
        ("Enter / → / Tab", "Key: go to the value"),
        ("/", "Key pattern (SCAN MATCH)"),
        ("m", "Scan more"),
        ("r", "Rescan"),
        ("a", "Add key (SET / HSET / RPUSH / SADD / ZADD …)"),
        ("D / Delete", "Delete key"),
        ("R", "Rename key"),
        ("t", "Set TTL (empty = persist)"),
        (":", "Run any command"),
        ("e / Enter (value)", "Edit selected element / string"),
        ("a (value)", "Add element"),
        ("x / Delete (value)", "Delete element"),
        ("Esc / Tab / ← (value)", "Back to keys"),
    ]),
    ("Data source form", &[
        ("↑ ↓ / Tab", "Move between fields"),
        ("type / Backspace", "Edit text fields"),
        ("Space / ← →", "Toggle on/off · change choice (Type, Color, SSL mode, Redis mode…)"),
        ("Ctrl+T", "Test connection"),
        ("Ctrl+Y", "Trust the SSH server's host key (after Ctrl+T showed its fingerprint)"),
        ("Enter / Ctrl+S", "Save"),
        ("Esc", "Cancel"),
    ]),
    ("Popups", &[
        ("Ctrl+B", "Go to DDL (in Search Everywhere / Go to Database Object)"),
        ("type", "Filter (palettes)"),
        ("↑ ↓ / PgUp PgDn", "Move / scroll"),
        ("Enter", "Run / confirm"),
        ("y / n", "Answer confirmations"),
        ("Esc", "Close"),
    ]),
];

/// `--help` 출력 (명령 단축키는 기본값)
/// F1 맨 위 "Start here": 처음 쓸 때 알아야 할 키 (명령은 지금 키맵으로)
pub fn start_here(keys: &crate::keys::Keymap) -> Vec<(String, &'static str)> {
    use crate::app::Action as A;
    let k = |a: A| keys.label(a);
    vec![
        (k(A::SearchEverywhere), "Search everything: tables, columns, tabs, actions (not sure? start here)"),
        (k(A::Palette), "All actions, each with its shortcut"),
        (k(A::NewConsole), "New query console for the selected data source"),
        (k(A::Execute), "Run (asks what to run when there are several statements)"),
        (k(A::RecentTabs), "Recent tabs"),
        (k(A::CloseTab), "Close tab"),
        ("Enter (explorer)".into(), "Connect · open a table"),
        ("s (explorer)".into(), "Choose the databases and schemas shown"),
        ("Esc".into(), "Back (to the explorer, or from results to the editor)"),
        (k(A::Quit), "Quit"),
    ]
}

pub fn text() -> String {
    let mut s = String::from("\nStart here\n");
    for (k, d) in start_here(&crate::keys::Keymap::new(&Default::default())) {
        s.push_str(&format!("  {:<30} {}\n", k, d));
    }
    s.push_str("\nCommands (change them with F1)\n");
    for (a, keys) in crate::keys::DEFAULT_KEYS {
        s.push_str(&format!("  {:<30} {}\n", crate::keys::disp(&keys.join(" / ")), crate::input::name_of(*a)));
    }
    for (title, rows) in HELP {
        s.push_str(&format!("\n{title}\n"));
        for (k, d) in *rows {
            s.push_str(&format!("  {:<30} {}\n", crate::keys::disp(k), crate::keys::disp(d)));
        }
    }
    s
}
