//! Data tools: generate test data (⌃K › Generate Test Data…) · partial copy of production data (⌃K › Copy Related Rows…) ·
//! result bar chart (⌃K › Chart Result)
use crate::app::{App, Focus, PAction, PItem, Palette, Popup, Tab, TabKind};
use crate::migrate::Flow;
use crate::transfer::thousands;
use crate::tree::NodeKind;
use qurry_core::sql;
use qurry_shared::chart;

impl App {
    /// The table picked in the explorer, otherwise the open table tab: (target, Data Source id, schema, table)
    pub(crate) fn chosen_table(&self) -> Option<(String, String, String, String)> {
        if self.focus == Focus::Tree
            && let Some(n) = self.tree.selected()
            && let NodeKind::Object(k) = &n.kind
            && ["table", "partitioned"].contains(&k.as_str())
        {
            return Some((sql::target_id(&n.conn, n.db.as_deref()), n.conn.clone(), n.schema.clone().unwrap_or_default(), n.name.clone()?));
        }
        match self.tabs.get(self.active) {
            Some(Tab { kind: TabKind::Table(tt), target, conn, .. }) => Some((target.clone(), conn.clone(), tt.schema.clone(), tt.table.clone())),
            _ => None,
        }
    }

    /// Its name, if a Production Data Source
    pub(crate) fn production_name(&self, conn: &str) -> Option<String> {
        self.conn(conn).filter(|c| c.config.options.confirm_changes).map(|c| c.config.name.clone())
    }

    /// Ask how many rows → show how each column is filled and confirm → insert in one transaction
    pub fn generate_data_start(&mut self) {
        let Some((target, conn, schema, table)) = self.chosen_table() else {
            self.status = "Select a table in the explorer (or open a table tab) to fill with test data".into();
            return;
        };
        let where_ = if schema.is_empty() { table.clone() } else { format!("{schema}.{table}") };
        self.input(format!("Generate test data into {where_} — how many rows?"), "100", move |a, n| {
            let Ok(n) = n.replace([',', '_', ' '], "").parse::<usize>() else {
                a.status = "Type a number of rows".into();
                return;
            };
            if n == 0 || n > qurry_core::fake::MAX_ROWS {
                a.status = format!("Choose 1 to {} rows", thousands(qurry_core::fake::MAX_ROWS as u64));
                return;
            }
            let st = a.state.clone();
            let (t2, s2, tb2) = (target.clone(), schema.clone(), table.clone());
            a.status = "Reading the table…".into();
            a.spawn(async move { st.fake_preview(t2, s2, tb2, 3).await }, move |a, r| {
                let pv = match r {
                    Ok(pv) => pv,
                    Err(e) => return a.status = format!("Generate test data: {e}"),
                };
                a.status.clear();
                let width = pv.plan.iter().map(|c| c.name.chars().count()).max().unwrap_or(0);
                let mut text = format!("Insert {} generated rows into {where_}?\n\n", thousands(n as u64));
                for c in &pv.plan {
                    text.push_str(&format!("  {:width$}  {}\n", c.name, c.how));
                }
                for w in &pv.warnings {
                    text.push_str(&format!("\n⚠ {w}"));
                }
                if let Some(name) = a.production_name(&conn) {
                    text.push_str(&format!("\n⚠ Production data source \"{name}\""));
                }
                text.push_str("\n\nAll rows go in one transaction — if one fails, nothing is inserted.");
                a.popup = Some(Popup::Confirm {
                    text,
                    on_yes: Box::new(move |a| {
                        let st = a.state.clone();
                        a.status = format!("Inserting {} rows…", thousands(n as u64));
                        a.spawn(async move { st.fake_fill(target, schema, table, n).await }, move |a, r| {
                            a.status = match r {
                                Ok(k) => {
                                    a.warn_ran(k.warning);
                                    format!("{} test row(s) inserted into {where_}", thousands(k.rows))
                                }
                                Err(e) => format!("Generate test data: {e}"),
                            };
                        });
                    }),
                });
            });
        });
    }

    /// ⌃K › Copy Related Rows…: start-row condition → child depth → destination → confirm preview → copy (personal data masked)
    pub fn copy_rows_start(&mut self) {
        let Some((target, conn, schema, table)) = self.chosen_table() else {
            self.status = "Select a table in the explorer (or open a table tab) to copy rows from".into();
            return;
        };
        if !matches!(self.kind_of(&conn), qurry_core::store::DbKind::Postgres | qurry_core::store::DbKind::Mysql) {
            self.status = "Copying related rows works for PostgreSQL and MySQL".into();
            return;
        }
        let where_ = if schema.is_empty() { table.clone() } else { format!("{schema}.{table}") };
        self.input(format!("Copy related rows from {where_} — WHERE (empty = all rows)"), "", move |a, filter| {
            let spec = qurry_core::subset::SubsetSpec { schema, table, filter, depth: 1, mask: true };
            let items = [("0", "None — only these rows and the rows they point to"), ("1", "1 level of child rows (e.g. a customer's orders)"), ("2", "2 levels (… and the order items)"), ("3", "3 levels")]
                .into_iter()
                .map(|(v, l)| PItem { label: l.into(), hint: String::new(), action: PAction::Pick(v.into()) })
                .collect();
            a.flow = Some(Flow::CopyDepth(Box::new((target, conn, spec))));
            a.popup = Some(Popup::Palette(Palette { title: "Copy Related Rows — child rows".into(), input: tui_textarea::TextArea::default(), items, sel: 1, loading: 0, cache: Default::default() }));
        });
    }

    /// Whether to mask personal data (default: mask)
    pub(crate) fn copy_rows_mask(&mut self, (source, conn, mut spec): (String, String, qurry_core::subset::SubsetSpec), depth: String) {
        spec.depth = depth.parse().unwrap_or(1);
        let items = [("mask", "Mask personal data (email, phone, people's names, address, ID / card numbers, passwords, birth dates, IP)"), ("raw", "Copy as is — no masking")]
            .into_iter()
            .map(|(v, l)| PItem { label: l.into(), hint: String::new(), action: PAction::Pick(v.into()) })
            .collect();
        self.flow = Some(Flow::CopyMask(Box::new((source, conn, spec))));
        self.popup = Some(Popup::Palette(Palette { title: "Copy Related Rows — personal data".into(), input: tui_textarea::TextArea::default(), items, sel: 0, loading: 0, cache: Default::default() }));
    }

    pub(crate) fn copy_rows_target(&mut self, (source, conn, mut spec): (String, String, qurry_core::subset::SubsetSpec), mask: String) {
        spec.mask = mask != "raw";
        let kind = self.kind_of(&conn);
        // Non-production targets first (so nothing goes into production by mistake)
        let mut cands: Vec<_> = self.conns.iter().filter(|c| c.config.kind == kind && c.config.id != conn).collect();
        cands.sort_by_key(|c| c.config.options.confirm_changes);
        let items: Vec<PItem> = cands
            .into_iter()
            .map(|c| PItem { label: c.config.name.clone(), hint: if c.config.options.confirm_changes { "Production".into() } else { String::new() }, action: PAction::Pick(c.config.id.clone()) })
            .collect();
        if items.is_empty() {
            self.status = "No other data source of the same kind to copy into — add one first".into();
            return;
        }
        self.flow = Some(Flow::CopyTarget(Box::new((source, conn, spec))));
        self.popup = Some(Popup::Palette(Palette { title: "Copy Related Rows — into".into(), input: tui_textarea::TextArea::default(), items, sel: 0, loading: 0, cache: Default::default() }));
    }

    pub(crate) fn copy_rows_preview(&mut self, (source, _conn, spec): (String, String, qurry_core::subset::SubsetSpec), into: String) {
        let st = self.state.clone();
        let (s2, sp2) = (source.clone(), spec.clone());
        self.status = "Reading the rows to copy…".into();
        self.spawn(async move { st.subset_preview(s2, sp2).await }, move |a, r| {
            let plan = match r {
                Ok(p) => p,
                Err(e) => return a.status = format!("Copy related rows: {e}"),
            };
            a.status.clear();
            let name = a.conn(&into).map(|c| c.config.name.clone()).unwrap_or_default();
            let width = plan.tables.iter().map(|t| t.table.chars().count()).max().unwrap_or(0);
            let mut text = format!("Copy {} rows into \"{name}\"? (in insert order)\n\n", thousands(plan.total as u64));
            for t in &plan.tables {
                let masked: Vec<&str> = t.masked.iter().map(|m| m.column.as_str()).collect();
                text.push_str(&format!("  {:width$}  {:>7}  {}{}\n", t.table, thousands(t.rows as u64), t.reason, if masked.is_empty() { String::new() } else { format!(" · masked: {}", masked.join(", ")) }));
            }
            if let Some(p) = a.production_name(&into) {
                text.push_str(&format!("\n⚠ Production data source \"{p}\"\n"));
            }
            if !spec.mask {
                text.push_str("\n⚠ Personal data is copied as is (not masked)\n");
            }
            text.push_str("\nOne transaction; rows already there are skipped. The tables must exist in the target.");
            a.popup = Some(Popup::Confirm {
                text,
                on_yes: Box::new(move |a| {
                    let st = a.state.clone();
                    a.status = "Copying…".into();
                    a.spawn(async move { st.subset_copy(source, spec, into).await }, move |a, r| {
                        a.status = match r {
                            Ok(p) => {
                                a.warn_ran(p.warning);
                                format!("{} related row(s) copied into {name} (rows already there were skipped)", thousands(p.total as u64))
                            }
                            Err(e) => format!("Copy related rows: {e}"),
                        };
                    });
                }),
            });
        });
    }

    /// ⌃K › Chart Result: the current result table as horizontal bars (X = first non-numeric column, value = sum of the first numeric non-id column, else row count)
    pub fn chart_result(&mut self) {
        let Some(g) = self.grid_mut() else {
            self.status = "Run a query (or open a table) to chart its result".into();
            return;
        };
        match bar_chart(&g.result, 40, 30) {
            Some((title, text)) => self.popup = Some(Popup::Message { title, text }),
            None => self.status = "Nothing to chart — the result has no rows".into(),
        }
    }
}

/// The result's columns as qurry_shared::chart reads them
pub(crate) fn chart_columns(r: &qurry_core::db::QueryResult) -> Vec<chart::Column> {
    r.columns.iter().map(|c| chart::Column { name: c.name.clone(), type_name: c.type_name.clone() }).collect()
}

/// Result → (title, horizontal bar text): qurry_shared::chart with the TUI's options (axes, sums, date · numeric X in order,
/// others largest first), drawn here as up to `rows` lines
pub(crate) fn bar_chart(r: &qurry_core::db::QueryResult, width: usize, rows: usize) -> Option<(String, String)> {
    if r.rows.is_empty() || r.columns.is_empty() {
        return None;
    }
    let cols = chart_columns(r);
    let axes = chart::default_axes(&cols, &r.rows, chart::Options::TUI);
    let data = chart::aggregate(&cols, &r.rows, axes.x, &[axes.y], chart::Options::TUI);
    let total = data.labels.len();
    let bars: Vec<(String, f64)> = data.labels.iter().map(|k| crate::grid::cell_text(&serde_json::Value::String(k.clone()))).zip(data.series[0].values.iter().copied()).take(rows).collect();
    let max = bars.iter().map(|(_, v)| v.abs()).fold(0.0, f64::max);
    let lw = bars.iter().map(|(k, _)| k.chars().count()).max().unwrap_or(0).min(24);
    // 1,234 · 1,234.50
    let fmt = |v: f64| {
        let s = if v.fract() == 0.0 { format!("{:.0}", v.abs()) } else { format!("{:.2}", v.abs()) };
        let (int, frac) = s.split_once('.').map(|(a, b)| (a, format!(".{b}"))).unwrap_or((&s, String::new()));
        let int = int.parse::<u64>().map(crate::transfer::thousands).unwrap_or_else(|_| int.to_string());
        format!("{}{int}{frac}", if v < 0.0 { "-" } else { "" })
    };
    let mut text = String::new();
    for (k, v) in &bars {
        let n = if max > 0.0 { ((v.abs() / max) * width as f64).round() as usize } else { 0 };
        let label: String = if k.chars().count() > lw { format!("{}…", k.chars().take(lw - 1).collect::<String>()) } else { k.clone() };
        text.push_str(&format!("{label:<lw$} │{} {}\n", "█".repeat(n.max(usize::from(*v != 0.0))), fmt(*v)));
    }
    if total > rows {
        text.push_str(&format!("\n… {} more", total - rows));
    }
    let what = axes.y.map(|y| format!("sum of {}", r.columns[y].name)).unwrap_or_else(|| "row count".into());
    let loaded = if r.truncated { format!(" · first {} loaded rows", r.rows.len()) } else { String::new() };
    Some((format!("Chart: {what} by {}{loaded}", r.columns[axes.x].name), text.trim_end().to_string()))
}

#[cfg(test)]
mod tests {
    use qurry_core::db::{ColumnInfo, QueryResult};
    use serde_json::json;

    #[test]
    fn bar_chart_sums_by_category() {
        let r = QueryResult {
            columns: [("id", "INT8"), ("region", "TEXT"), ("amount", "NUMERIC")].iter().map(|(n, t)| ColumnInfo { name: n.to_string(), type_name: t.to_string() }).collect(),
            rows: vec![vec![json!(1), json!("Seoul"), json!("100")], vec![json!(2), json!("Busan"), json!("40")], vec![json!(3), json!("Seoul"), json!("20")], vec![json!(4), json!(null), json!(null)]],
            rows_affected: 0,
            truncated: false,
            elapsed_ms: 0,
            has_result_set: true,
            in_transaction: false,
            warning: None,
        };
        let (title, text) = super::bar_chart(&r, 10, 30).unwrap();
        assert_eq!(title, "Chart: sum of amount by region");
        assert_eq!(text, "Seoul  │██████████ 120\nBusan  │███ 40\n(null) │ 0");
        let (_, text) = super::bar_chart(&r, 10, 1).unwrap();
        assert!(text.ends_with("… 2 more"), "{text}");
        let big = QueryResult { rows: vec![vec![json!(1), json!("a"), json!("1332117.67")]], ..r };
        assert!(super::bar_chart(&big, 10, 30).unwrap().1.ends_with("1,332,117.67"));
        // A float X reads as the grid next to it shows the cell (2.0, not JavaScript's 2)
        let floats = QueryResult {
            columns: [("price", "FLOAT8"), ("n", "INT8")].iter().map(|(n, t)| ColumnInfo { name: n.to_string(), type_name: t.to_string() }).collect(),
            rows: vec![vec![json!(2.5), json!(3)], vec![json!(2.0), json!(1)]],
            ..big
        };
        assert_eq!(super::bar_chart(&floats, 10, 30).unwrap().1, "2.0 │███ 1\n2.5 │██████████ 3");
    }

    #[test]
    fn bar_chart_needs_rows() {
        let r = QueryResult {
            columns: [("a", "text"), ("b", "int4")].iter().map(|(n, t)| ColumnInfo { name: n.to_string(), type_name: t.to_string() }).collect(),
            rows: vec![],
            rows_affected: 0,
            truncated: false,
            elapsed_ms: 0,
            has_result_set: true,
            in_transaction: false,
            warning: None,
        };
        assert!(super::bar_chart(&r, 10, 30).is_none());
    }
}
