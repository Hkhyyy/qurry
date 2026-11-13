//! The status line's items for the focused grid (#132): which are on (settings.json "statusBarItems", shared with the GUI's status
//! bar and Settings › Appearance › Data grid) and what they say (qurry_shared::statusbar, the GUI's words). The TUI's selection is the marked rows
//! (m) with the cursor's column, or a ⇧-arrow range; with neither, the cursor's cell
use super::*;
use qurry_core::editable::{ColumnEdit, Editable};
use qurry_core::statusbar::{self, ColumnType, Item, Segment, Span, Stats, Status, ITEMS, MAX_CELLS};
use std::hash::{Hash, Hasher};

/// A table's columns and their declared types for Size (while they load or when they couldn't be read, Size shows the bytes only),
/// and the columns the database numbers (ColumnMeta::auto_increment: Submit's hint when their sequence is behind)
pub enum TableCols {
    Loading,
    Loaded(Vec<(String, ColumnType)>, Vec<String>),
    /// Asked again once this long has passed (the connection may be back)
    Failed(std::time::Instant),
}

/// How long a table whose columns couldn't be read waits before they are asked for again
const RETRY: std::time::Duration = std::time::Duration::from_secs(30);

impl App {
    /// ⌃k › Status Bar Items…: a checklist of the items by group, then Restore defaults. Enter or Space toggles and it stays open on
    /// the same row. Reads settings.json again, so a change made in the GUI since the TUI started shows here
    pub fn palette_status_items(&mut self, sel: usize) {
        self.status_items = self.state.store.status_bar_items();
        let mut items: Vec<PItem> = ITEMS
            .iter()
            .map(|i| {
                let on = self.status_items.contains(&i.id);
                let hint = if i.hint.is_empty() { i.group.to_lowercase() } else { format!("{}: {}", i.group.to_lowercase(), i.hint) };
                PItem { label: format!("[{}] {:<16}", if on { "x" } else { " " }, i.label), hint, action: PAction::StatusItem(Some(i.id)) }
            })
            .collect();
        let defaults = statusbar::DEFAULTS.iter().filter_map(|d| ITEMS.iter().find(|i| i.id == *d)).map(|i| i.label).collect::<Vec<_>>().join(", ");
        items.push(PItem { label: format!("    {:<16}", "Restore defaults"), hint: defaults, action: PAction::StatusItem(None) });
        self.popup = Some(Popup::Palette(Palette { title: "Status Bar Items".into(), input: TextArea::default(), items, sel, loading: 0, cache: Default::default() }));
    }

    /// Turn an item on or off (None: Restore defaults) for both apps, on what settings.json holds now; the checklist stays open there
    pub(super) fn toggle_status_item(&mut self, item: Option<Item>, sel: usize) {
        let store = &self.state.store;
        let r = match item {
            Some(i) => store.set_status_bar_item(i, !self.status_items.contains(&i)),
            None => store.reset_status_bar_items(),
        };
        match r {
            Ok(next) => {
                self.status_items = next;
                self.status = "Status bar items — saved for the GUI too".into();
            }
            Err(e) => self.status = format!("Couldn't save the status bar items: {e}"),
        }
        self.palette_status_items(sel);
    }

    /// The status line's segments for the focused grid (none while focus is elsewhere). Size asks for the table's column types the
    /// first time a table column's cell needs them (a table tab gets them as it loads)
    pub fn status_segments(&mut self) -> Vec<Segment> {
        if self.focus != Focus::Grid || self.status_items.is_empty() {
            return vec![];
        }
        let Some(tab) = self.tabs.get(self.active) else { return vec![] };
        let (grid, table) = match &tab.kind {
            TabKind::Console(c) => (c.grid(), None),
            TabKind::Table(t) => (t.grid.as_ref(), Some((t.schema.clone(), t.table.clone()))),
            TabKind::Dba(d) => (d.grid.as_ref(), None),
            _ => return vec![],
        };
        let Some(g) = grid else { return vec![] };
        let (target, kind) = (tab.target.clone(), self.kind_of(&tab.conn));
        let mut cache = self.stats_cache;
        let mut s = Status { kind: Some(kind), shown: g.shown, ..Default::default() };
        if g.rows() > 0 && g.cols() > 0 {
            let (rows, cols) = selection(g);
            let (r, c) = (Span::of(rows.iter().copied()), Span::of(cols.iter().copied()));
            s.pos = r.zip(c);
            if rows.len() * cols.len() > 1 {
                // Over the cells (at most MAX_CELLS, as the GUI) only while one of the selection's items is on, and again only when the
                // grid, the selection or an edit changed (it is drawn on every key)
                let on = ITEMS.iter().any(|i| i.group == "Selection" && self.status_items.contains(&i.id));
                let mut h = std::collections::hash_map::DefaultHasher::new();
                (on, g.id, g.row, g.col, g.anchor, &g.marked, &g.edits, g.rows()).hash(&mut h);
                let key = h.finish();
                let st = match cache {
                    Some((k, st)) if k == key => st,
                    _ => {
                        let mut st = Stats::default();
                        if on {
                            let cells = rows.iter().flat_map(|&r| cols.iter().map(move |&c| (r, c))).take(MAX_CELLS);
                            for (r, c) in cells {
                                st.add(&g.value(r, c).map_or(Value::Null, Value::String));
                            }
                        }
                        cache = Some((key, st));
                        st
                    }
                };
                s.stats = Some(st);
            } else if self.status_items.contains(&Item::Size) {
                let col = &g.result.columns[g.col];
                // The table column the cell is: a table tab's own, a console result of one table or a join's column of a table
                let of = match (&table, &g.result.editable) {
                    (Some((sc, t)), _) => Some((sc.clone(), t.clone(), col.name.clone())),
                    (None, Some(Editable::Table(e))) => e.columns.get(g.col).map(|c| (e.schema.clone(), e.table.clone(), c.clone())),
                    (None, Some(Editable::Join(j))) => match j.columns.get(g.col) {
                        Some(ColumnEdit::To { table, column }) => j.tables.get(*table).map(|t| (t.schema.clone(), t.table.clone(), column.clone())),
                        _ => None,
                    },
                    _ => None,
                };
                let value = g.value(g.row, g.col).map_or(Value::Null, Value::String);
                let type_name = col.type_name.clone();
                let column = of.and_then(|(sc, t, name)| self.table_cols(&target, &sc, &t)?.iter().find(|(n, _)| *n == name).map(|(_, ty)| ty.clone()));
                s.cell = Some(statusbar::Cell { type_name, value, column });
            }
        }
        self.stats_cache = cache;
        statusbar::segments(&self.status_items, &s)
    }

    /// A table's column types, loaded the first time they are asked for (a table tab's load fills them in again each time), and
    /// asked again a while after they couldn't be read
    fn table_cols(&mut self, target: &str, schema: &str, table: &str) -> Option<&Vec<(String, ColumnType)>> {
        let key = (target.to_string(), schema.to_string(), table.to_string());
        let ask = match self.table_cols.get(&key) {
            None => true,
            Some(TableCols::Failed(at)) => at.elapsed() >= RETRY,
            Some(_) => false,
        };
        if ask {
            self.table_cols.insert(key.clone(), TableCols::Loading);
            let st = self.state.clone();
            let (t, sc, tb) = key.clone();
            let k = key.clone();
            self.spawn(async move { st.table_detail(t, sc, tb).await.ok() }, move |a, d| {
                a.table_cols.insert(k, d.map_or(TableCols::Failed(std::time::Instant::now()), |d| loaded(d.columns)));
            });
        }
        match self.table_cols.get(&key)? {
            TableCols::Loaded(c, _) => Some(c),
            _ => None,
        }
    }
}

/// The table's columns as Size and Submit's hint read them
pub fn loaded(cols: Vec<qurry_core::meta::ColumnMeta>) -> TableCols {
    let filled = cols.iter().filter(|c| c.auto_increment).map(|c| c.name.clone()).collect();
    TableCols::Loaded(cols.into_iter().map(|c| (c.name, ColumnType { data_type: c.data_type, charset: c.charset })).collect(), filled)
}

/// The selected rows and columns (in grid order): the marked rows (and a range's rows) in the cursor's column, else the range, else the
/// cursor's cell
fn selection(g: &Grid) -> (Vec<usize>, Vec<usize>) {
    let rect = g.sel_rect();
    if !g.marked.is_empty() {
        let mut rows = g.marked.clone();
        if let Some((r0, _, r1, _)) = rect {
            rows.extend(r0..=r1);
        }
        return (rows.into_iter().filter(|&r| r < g.rows()).collect(), vec![g.col]);
    }
    match rect {
        Some((r0, c0, r1, c1)) => ((r0..=r1).collect(), (c0..=c1).collect()),
        None => (vec![g.row], vec![g.col]),
    }
}
