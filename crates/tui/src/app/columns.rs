//! Hide Column, Pin Column and Columns… (#178) on a table tab or a result: the same layout as the GUI's header menu. A table tab's
//! layout is kept for its table in settings.json "columnLayout", where the GUI reads and writes it too; a console result keeps it
//! until it runs again
use super::*;
use qurry_core::store::TableRef;

impl App {
    /// The grid the column commands act on and the table its layout is saved for (None: a result's, not saved). Elsewhere the status
    /// line says what to open
    fn layout_grid(&mut self) -> Option<(&mut Grid, Option<TableRef>)> {
        let Some(t) = self.tabs.get(self.active).filter(|t| matches!(t.kind, TabKind::Table(_) | TabKind::Console(_) | TabKind::Dba(_))) else {
            self.status = "Open a result or table to hide or pin its columns".into();
            return None;
        };
        let table = super::tables::table_ref(t);
        self.grid_mut().filter(|g| g.cols() > 0).map(|g| (g, table))
    }

    /// Hide Column: the selected columns (else the cursor's), never every column shown
    pub fn column_hide(&mut self) {
        let columns = match self.keys.slot_label(crate::keys::Slot { action: Action::Columns, pane: qurry_core::keymap::Scope::Grid }) {
            k if k == "—" => format!("{} › Columns…", self.keys.label(Action::Palette)),
            k => k,
        };
        let Some((g, table)) = self.layout_grid() else { return };
        let cs = g.selected_cols();
        let name = g.result.columns[g.col].name.clone();
        if cs.len() >= g.visible_cols() {
            self.status = if cs.len() == 1 { format!("{name} is the only column shown") } else { "At least one column stays shown".into() };
            return;
        }
        for &c in &cs {
            g.set_hidden(c, true);
        }
        self.status = match cs.len() {
            1 => format!("{name} hidden · Columns… ({columns}) shows it again"),
            n => format!("{n} columns hidden · Columns… ({columns}) shows them again"),
        };
        self.save_layout(table);
    }

    /// Pin Column: the selected columns (else the cursor's) pinned at the left in their order; when they all are, unpinned (the
    /// GUI's menu has Pin for the rest of a mixed selection first)
    pub fn column_pin_toggle(&mut self) {
        let Some((g, table)) = self.layout_grid() else { return };
        let cs = g.selected_cols();
        let name = g.result.columns[g.col].name.clone();
        let rest: Vec<usize> = cs.iter().copied().filter(|&c| !g.is_pinned(c)).collect();
        let pin = !rest.is_empty();
        let on = if pin { rest } else { cs };
        for &c in &on {
            g.set_pinned(c, pin);
        }
        self.status = match (on.len(), pin) {
            (1, true) => format!("{name} pinned at the left"),
            (1, false) => format!("{name} unpinned"),
            (n, true) => format!("{n} columns pinned at the left"),
            (n, false) => format!("{n} columns unpinned"),
        };
        self.save_layout(table);
    }

    /// Columns…: every column of the result with its box ([x] shown), the pinned ones marked, then Show All. Space / Enter shows or
    /// hides the row's column, ⇥ pins it, typing filters; it stays open on the same row and filter
    pub fn palette_columns(&mut self) {
        let Some((g, _)) = self.layout_grid() else { return };
        let items = column_items(g);
        let (input, sel) = match self.popup.take() {
            Some(Popup::Palette(p)) if p.title == COLUMNS => (p.input, p.sel),
            _ => (TextArea::default(), 0),
        };
        self.popup = Some(Popup::Palette(Palette { title: COLUMNS.into(), input, items, sel, loading: 0, cache: Default::default() }));
    }

    /// A row of Columns… picked: `pin` false shows or hides column `c` (None: Show All), true pins or unpins it
    pub fn pick_column(&mut self, c: Option<usize>, pin: bool) {
        let Some((g, table)) = self.layout_grid() else { return };
        match c {
            None if pin => return,
            None => g.show_all(),
            Some(c) if pin => g.set_pinned(c, !g.is_pinned(c)),
            Some(c) => {
                let hide = g.view.view.contains(&c);
                if !g.set_hidden(c, hide) {
                    self.status = format!("{} is the only column shown", g.result.columns[c].name);
                    return self.palette_columns();
                }
            }
        }
        self.save_layout(table);
        self.palette_columns();
    }

    /// A table tab's layout into settings.json for its table (Store::set_column_layout); a result's isn't saved
    fn save_layout(&mut self, table: Option<TableRef>) {
        let Some(table) = table else { return };
        let Some(layout) = self.grid_mut().map(|g| g.layout.clone()) else { return };
        let r = self.state.store.set_column_layout(&table, &layout);
        if let Some(g) = self.grid_mut() {
            g.layout_unsaved = r.is_err();
        }
        if let Err(e) = r {
            self.status = format!("Couldn't save the column layout ({e}). It applies until the tab closes; Columns… › Show All in the actions palette ({}) shows every column again.", self.keys.label(Action::Palette));
        }
    }
}

/// The Columns… palette's title (its rows are kept when it opens again over itself)
pub const COLUMNS: &str = "Columns";

fn column_items(g: &Grid) -> Vec<PItem> {
    let mut items: Vec<PItem> = g
        .result
        .columns
        .iter()
        .enumerate()
        .map(|(i, col)| {
            let shown = if g.view.view.contains(&i) { "x" } else { " " };
            let hint = if g.is_pinned(i) { format!("pinned · {}", col.type_name.to_lowercase()) } else { col.type_name.to_lowercase() };
            PItem { label: format!("[{shown}] {}", col.name), hint, action: PAction::Column(Some(i)) }
        })
        .collect();
    let hidden = match g.view.hidden {
        0 => "every column is shown".to_string(),
        n => format!("{n} hidden"),
    };
    items.push(PItem { label: "    Show All".into(), hint: hidden, action: PAction::Column(None) });
    items
}
