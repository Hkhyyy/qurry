//! 데이터 도구: 테스트 데이터 만들기 (⌃K › Generate Test Data…) · 운영 데이터 일부 복사 (⌃K › Copy Related Rows…) ·
//! 결과 막대 차트 (⌃K › Chart Result)
use crate::app::{App, Focus, PAction, PItem, Palette, Popup, Tab, TabKind};
use crate::migrate::Flow;
use crate::transfer::thousands;
use crate::tree::NodeKind;
use qurry_core::sql;

impl App {
    /// 탐색기에서 고른 테이블, 아니면 열린 표 탭: (대상, Data Source id, 스키마, 테이블)
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

    /// Production Data Source 면 그 이름
    pub(crate) fn production_name(&self, conn: &str) -> Option<String> {
        self.conn(conn).filter(|c| c.config.options.confirm_changes).map(|c| c.config.name.clone())
    }

    /// 몇 행 만들지 묻고 → 컬럼마다 채우는 법을 보여 주고 확인 → 한 트랜잭션으로 넣는다
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
                                Ok(k) => format!("{} test row(s) inserted into {where_}", thousands(k)),
                                Err(e) => format!("Generate test data: {e}"),
                            };
                        });
                    }),
                });
            });
        });
    }

    /// ⌃K › Copy Related Rows…: 시작 행 조건 → 자식 단계 → 넣을 곳 → 미리보기 확인 → 복사 (개인정보는 가린다)
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

    /// 개인정보를 가릴지 (기본: 가린다)
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
        // 운영이 아닌 곳을 먼저 (실수로 운영에 넣지 않게)
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
                            Ok(p) => format!("{} related row(s) copied into {name} (rows already there were skipped)", thousands(p.total as u64)),
                            Err(e) => format!("Copy related rows: {e}"),
                        };
                    });
                }),
            });
        });
    }

    /// ⌃K › Chart Result: 지금 결과 표를 가로 막대로 (X = 수가 아닌 첫 컬럼, 값 = id 가 아닌 첫 수 컬럼의 합, 없으면 행 수)
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

const NUM_TYPES: &[&str] = &["INT2", "INT4", "INT8", "INT", "INTEGER", "SMALLINT", "BIGINT", "TINYINT", "MEDIUMINT", "NUMERIC", "DECIMAL", "FLOAT", "FLOAT4", "FLOAT8", "DOUBLE", "REAL", "MONEY"];

fn num(v: &serde_json::Value) -> Option<f64> {
    match v {
        serde_json::Value::Number(n) => n.as_f64(),
        serde_json::Value::String(s) => s.replace(',', "").parse().ok().filter(|f: &f64| f.is_finite()),
        _ => None,
    }
}

/// 결과 → (제목, 가로 막대 글). 같은 X 는 합친다. 날짜 · 수 X 는 순서대로, 나머지는 큰 것부터. rows 줄까지
pub(crate) fn bar_chart(r: &qurry_core::db::QueryResult, width: usize, rows: usize) -> Option<(String, String)> {
    if r.rows.is_empty() || r.columns.is_empty() {
        return None;
    }
    let is_num = |i: usize| {
        let t = r.columns[i].type_name.to_uppercase();
        if NUM_TYPES.iter().any(|n| t.starts_with(n)) || t.contains("UNSIGNED") {
            return true;
        }
        let vals: Vec<&serde_json::Value> = r.rows.iter().take(200).map(|x| &x[i]).filter(|v| !v.is_null()).collect();
        !vals.is_empty() && !t.contains("DATE") && !t.contains("TIME") && vals.iter().filter(|v| num(v).is_some()).count() * 10 >= vals.len() * 9
    };
    let nums: Vec<usize> = (0..r.columns.len()).filter(|&i| is_num(i)).collect();
    let x = (0..r.columns.len()).find(|i| !nums.contains(i)).unwrap_or(0);
    let id = |i: &usize| {
        let n = r.columns[*i].name.to_lowercase();
        n == "id" || n.ends_with("_id") || n.starts_with("id_")
    };
    let cands: Vec<usize> = nums.iter().copied().filter(|&i| i != x).collect();
    let y = cands.iter().copied().find(|i| !id(i)).or(cands.first().copied());
    let mut order: Vec<String> = vec![];
    let mut sums: std::collections::HashMap<String, f64> = Default::default();
    for row in &r.rows {
        let k = if row[x].is_null() { "(null)".to_string() } else { crate::grid::cell_text(&row[x]) };
        let add = y.map(|y| num(&row[y]).unwrap_or(0.0)).unwrap_or(1.0);
        if !sums.contains_key(&k) {
            order.push(k.clone());
        }
        *sums.entry(k).or_default() += add;
    }
    let xt = r.columns[x].type_name.to_uppercase();
    if xt.contains("DATE") || xt.contains("TIME") {
        order.sort();
    } else if nums.contains(&x) {
        order.sort_by(|a, b| num(&serde_json::Value::String(a.clone())).unwrap_or(0.0).total_cmp(&num(&serde_json::Value::String(b.clone())).unwrap_or(0.0)));
    } else {
        order.sort_by(|a, b| sums[b].total_cmp(&sums[a]));
    }
    let total = order.len();
    order.truncate(rows);
    let max = order.iter().map(|k| sums[k].abs()).fold(0.0, f64::max);
    let lw = order.iter().map(|k| k.chars().count()).max().unwrap_or(0).min(24);
    // 1,234 · 1,234.50
    let fmt = |v: f64| {
        let s = if v.fract() == 0.0 { format!("{:.0}", v.abs()) } else { format!("{:.2}", v.abs()) };
        let (int, frac) = s.split_once('.').map(|(a, b)| (a, format!(".{b}"))).unwrap_or((&s, String::new()));
        let int = int.parse::<u64>().map(crate::transfer::thousands).unwrap_or_else(|_| int.to_string());
        format!("{}{int}{frac}", if v < 0.0 { "-" } else { "" })
    };
    let mut text = String::new();
    for k in &order {
        let v = sums[k];
        let n = if max > 0.0 { ((v.abs() / max) * width as f64).round() as usize } else { 0 };
        let label: String = if k.chars().count() > lw { format!("{}…", k.chars().take(lw - 1).collect::<String>()) } else { k.clone() };
        text.push_str(&format!("{label:<lw$} │{} {}\n", "█".repeat(n.max(usize::from(v != 0.0))), fmt(v)));
    }
    if total > rows {
        text.push_str(&format!("\n… {} more", total - rows));
    }
    let what = y.map(|y| format!("sum of {}", r.columns[y].name)).unwrap_or_else(|| "row count".into());
    let loaded = if r.truncated { format!(" · first {} loaded rows", r.rows.len()) } else { String::new() };
    Some((format!("Chart: {what} by {}{loaded}", r.columns[x].name), text.trim_end().to_string()))
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
        };
        let (title, text) = super::bar_chart(&r, 10, 30).unwrap();
        assert_eq!(title, "Chart: sum of amount by region");
        assert_eq!(text, "Seoul  │██████████ 120\nBusan  │███ 40\n(null) │ 0");
        let (_, text) = super::bar_chart(&r, 10, 1).unwrap();
        assert!(text.ends_with("… 2 more"), "{text}");
        let big = QueryResult { rows: vec![vec![json!(1), json!("a"), json!("1332117.67")]], ..r };
        assert!(super::bar_chart(&big, 10, 30).unwrap().1.ends_with("1,332,117.67"));
    }
}
