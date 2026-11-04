//! TUI keymap (source for F1 help, `qurry-tui --help`, docs/shortcuts-tui.md).
//! Rebindable keys are in the shared command table (crates/shared/src/keymap-table.json, keys::COMMANDS: commands, and the explorer's
//! and the grid's single-letter commands); only fixed per-screen keys live here: moving, opening, editing, copying and filtering
use qurry_core::keymap::Scope;

pub const HELP: &[(&str, &[(&str, &str)])] = &[
    ("Database Explorer", &[
        ("↑ ↓  (j k)", "Move"),
        ("→ / ←  (l h)", "Expand / collapse (← on a child jumps to its parent)"),
        ("Enter / double-click", "Expand / collapse (connects) · open table data · open a Redis key · show DDL of routines"),
        ("F4 / ⌃↓", "Open a query console for the data source / database (Redis: the key browser)"),
        ("/", "Filter loaded nodes (/ again edits it · empty or Esc clears)"),
        ("F5", "Refresh (the run key refreshes in the explorer)"),
        ("y  ⌘C", "Copy reference: schema.name, as the GUI's Copy Reference (OSC 52 clipboard; marked nodes: one per line)"),
        ("PgUp / PgDn · Home / End", "Page · first / last node"),
        ("Tab / Esc", "Go to the open tab (Esc clears the marks first)"),
    ]),
    ("Query console", &[
        ("completion list", "Opens as you type (or ⌃Space) · ↑↓ choose · Enter/Tab insert · Esc close"),
        ("⇧Tab", "Go to the result grid (Esc / Tab returns)"),
        ("Esc", "Go to the explorer"),
        ("⌃u / ⌃r", "Undo / redo (editor)"),
        ("⌃f", "Find / replace bar: ⌥x regex · ⌥c match case · ⌥w words · Enter next (↑ previous) · Tab to Replace: Enter one, ⌥a all ($1 groups) · Esc close"),
        ("⌘A  ⌫", "Select all, then delete (or ⌃k › Clear Console)"),
        ("click / drag  ⌘C", "Move the cursor / select text (runs only the selection) · copy the selection"),
        ("mouse drag", "Drag the editor/result border to resize"),
    ]),
    ("Tabs & split (mouse)", &[
        ("drag a tab", "Onto a pane's right / bottom edge = split it (up to 4 panes) · onto another pane or tab bar = move · within a bar = reorder"),
        ("drag the border", "Resize the Database Explorer (its right border) or the panes (the border between them; in a top/bottom split also the empty part of the lower tab bar)"),
        ("click a pane", "Focus it"),
        ("⇧wheel / trackpad", "Scroll the grid and the explorer sideways (⇧← / ⇧→ in the explorer)"),
    ]),
    ("Data grid (results & table data)", &[
        ("arrows  (h j k l)", "Move"),
        ("PgUp / PgDn · g / G", "Page · first / last row (more rows load automatically)"),
        ("Home / End", "First / last column"),
        ("⇧↑↓←→ / drag", "Select a range of cells (Esc clears) · scroll sideways: wheel over the header row, ⌥+wheel, or drag the bar on the bottom border"),
        ("Enter / F2", "Edit cell (table data)"),
        ("s", "Sort by the selected column: ▲ → ▼ → off (table data: ORDER BY · results: loaded rows)"),
        ("w or / · o", "Edit WHERE · ORDER BY (table data)"),
        ("r", "Reload (table data)"),
        ("y  ⌘C", "Copy cell value (a selected range or marked rows: as TSV, pastes into spreadsheets as cells)"),
        ("⌘V", "Paste into cells from the selected one (tabs / lines fill several; extra lines add rows; table data)"),
        ("Esc", "Back (query results: to the editor)"),
    ]),
    ("DDL / routine view", &[
        ("↑ ↓  (j k) · PgUp / PgDn", "Scroll"),
        ("v", "Table DDL: switch between SQL and Structure (columns, keys, indexes, foreign keys, triggers, options); kept for new DDL tabs"),
        ("r", "Reload what the tab shows (the DDL, or the Structure view)"),
        ("y  ⌘C", "Copy the text (a table's DDL: the SQL, in either view)"),
        ("Esc", "Back to the explorer"),
    ]),
    ("Database Admin tab (⌃k › Database Admin…)", &[
        ("r · a", "Refresh · auto-refresh on/off (sessions and locks refresh every 3 s)"),
        ("c · K", "Cancel the query · terminate the session of the selected row (asks first)"),
        ("Enter · g", "Open the row's query in a console · generate SQL from the row (DROP INDEX, VACUUM, REVOKE…)"),
        ("p", "Pick another report"),
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
        ("⌃t", "Test connection"),
        ("⌃y", "Trust the SSH server's host key (after ⌃t showed its fingerprint)"),
        ("Enter / ⌃s", "Save"),
        ("Esc", "Cancel"),
    ]),
    ("Popups", &[
        ("Go to DDL key", "DDL of the selected object (in Search Everywhere / Go to Database Object)"),
        ("type", "Filter (palettes)"),
        ("↑ ↓ / PgUp PgDn", "Move / scroll"),
        ("Enter", "Run / confirm"),
        ("y / n", "Answer confirmations"),
        ("Esc", "Close"),
    ]),
];

/// `--help` output (command shortcuts at their defaults)
/// "Start here" at the top of F1: keys to know when starting out (commands with the current keymap)
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
        (keys.hint(A::ManageSchemas, ""), "Choose the databases and schemas shown"),
        ("Esc".into(), "Back (to the explorer, or from results to the editor)"),
        (k(A::Quit), "Quit"),
    ]
}

pub fn text() -> String {
    let mut s = String::from("\nStart here\n");
    for (k, d) in start_here(&crate::keys::Keymap::new(&Default::default())) {
        s.push_str(&format!("  {:<30} {}\n", k, d));
    }
    // Lines with default keys (F1 also lists the commands without one)
    let keys = crate::keys::Keymap::new(&Default::default());
    let mut pane = Scope::Global;
    s.push_str("\nCommands (change them with F1)\n");
    for (slot, _) in crate::keys::COMMANDS.iter().filter(|(_, k)| !k.is_empty()) {
        if slot.pane != pane {
            pane = slot.pane;
            s.push_str(if pane == Scope::Tree { "\nDatabase Explorer keys (change them with F1)\n" } else { "\nData grid keys (change them with F1)\n" });
        }
        s.push_str(&format!("  {:<30} {}\n", keys.slot_label(*slot), crate::app::title(slot.action)));
    }
    for (title, rows) in HELP {
        s.push_str(&format!("\n{title}\n"));
        for (k, d) in *rows {
            s.push_str(&format!("  {:<30} {}\n", crate::keys::disp(k), crate::keys::disp(d)));
        }
    }
    s
}
