//! 실행 계획 (PostgreSQL EXPLAIN FORMAT JSON) → 트리 + 노드마다 자기 몫(시간·비용 %)과 경고 (Seq Scan, 추정 오차, 디스크로 넘친 정렬 …).
//! GUI 는 트리로, TUI 는 표로 그린다
use crate::db::{ColumnInfo, QueryResult};
use crate::error::{AppError, AppResult};
use serde_json::Value;

#[derive(Debug, Clone, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanNode {
    pub op: String,
    pub detail: String,
    pub startup: Option<f64>,
    pub total: Option<f64>,
    pub rows: Option<f64>,
    pub width: Option<f64>,
    pub actual_time: Option<f64>,
    pub actual_rows: Option<f64>,
    pub loops: Option<f64>,
    /// 이 노드만의 시간 (ms, 자식 몫 빼고 loops 곱해서). ANALYZE 일 때만
    pub self_time: Option<f64>,
    /// 이 노드만의 비용 (자식 몫 빼고)
    pub self_cost: Option<f64>,
    /// 전체에서 이 노드 몫 (%) — ANALYZE 면 시간, 아니면 비용
    pub pct: f64,
    pub warnings: Vec<String>,
    /// 나머지 속성 (상세 보기)
    pub props: Vec<(String, String)>,
    pub children: Vec<PlanNode>,
}

fn num(n: &serde_json::Map<String, Value>, k: &str) -> Option<f64> {
    n.get(k).and_then(Value::as_f64)
}

fn text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Array(a) => a.iter().map(text).collect::<Vec<_>>().join(", "),
        other => other.to_string(),
    }
}

/// 1234567 → "1,234,567"
fn grouped(n: f64) -> String {
    let s = (n.round() as i64).to_string();
    let (sign, digits) = s.strip_prefix('-').map(|d| ("-", d)).unwrap_or(("", &s));
    let g: Vec<&str> = digits.as_bytes().rchunks(3).rev().map(|c| std::str::from_utf8(c).unwrap()).collect();
    format!("{sign}{}", g.join(","))
}

/// parallel = Gather 아래 (워커마다 같은 시간이 동시에 흐른다: loops 를 곱하지 않는다)
fn node(n: &serde_json::Map<String, Value>, parallel: bool) -> PlanNode {
    let s = |k: &str| n.get(k).map(text).unwrap_or_default();
    let rel = if n.contains_key("Relation Name") {
        let schema = s("Schema");
        let alias = s("Alias");
        let name = s("Relation Name");
        format!("on {}{name}{}", if schema.is_empty() { String::new() } else { format!("{schema}.") }, if !alias.is_empty() && alias != name { format!(" {alias}") } else { String::new() })
    } else {
        String::new()
    };
    let detail = [
        rel,
        if n.contains_key("Index Name") { format!("using {}", s("Index Name")) } else { String::new() },
        if n.contains_key("Join Type") { format!("{} join", s("Join Type")) } else { String::new() },
        if n.contains_key("Filter") { format!("filter: {}", s("Filter")) } else { String::new() },
        if n.contains_key("Index Cond") { format!("cond: {}", s("Index Cond")) } else { String::new() },
        if n.contains_key("Hash Cond") { format!("cond: {}", s("Hash Cond")) } else { String::new() },
        if n.contains_key("Merge Cond") { format!("cond: {}", s("Merge Cond")) } else { String::new() },
        if n.contains_key("Sort Key") { format!("key: {}", s("Sort Key")) } else { String::new() },
        if n.contains_key("Group Key") { format!("group: {}", s("Group Key")) } else { String::new() },
    ]
    .into_iter()
    .filter(|x| !x.is_empty())
    .collect::<Vec<_>>()
    .join("  ");
    let op_name = s("Node Type");
    let under = parallel || op_name.starts_with("Gather");
    let children: Vec<PlanNode> = n.get("Plans").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_object).map(|c| node(c, under)).collect()).unwrap_or_default();
    let (total, actual_time, loops) = (num(n, "Total Cost"), num(n, "Actual Total Time"), num(n, "Actual Loops"));
    let spent = |t: Option<f64>, l: Option<f64>| t.map(|t| t * if under { 1.0 } else { l.unwrap_or(1.0) });
    let self_time = spent(actual_time, loops).map(|t| (t - children.iter().filter_map(|c| spent(c.actual_time, c.loops)).sum::<f64>()).max(0.0));
    let self_cost = total.map(|t| (t - children.iter().filter_map(|c| c.total).sum::<f64>()).max(0.0));
    let (rows, actual_rows) = (num(n, "Plan Rows"), num(n, "Actual Rows"));
    let op = s("Node Type");

    // 경고: 고칠 만한 곳
    let mut warnings = vec![];
    let removed = num(n, "Rows Removed by Filter").unwrap_or(0.0) * loops.unwrap_or(1.0);
    let out_rows = actual_rows.map(|r| r * loops.unwrap_or(1.0));
    if op == "Seq Scan" {
        let read = out_rows.map(|r| r + removed).unwrap_or(0.0).max(rows.unwrap_or(0.0));
        if read >= 10_000.0 {
            warnings.push(format!("Seq Scan reads the whole table (~{} rows) — an index on the filter columns may help", grouped(read)));
        }
    }
    if let (Some(e), Some(a)) = (rows, actual_rows) {
        let (e, a) = (e.max(1.0), a.max(1.0));
        let ratio = (a / e).max(e / a);
        if ratio >= 10.0 && a.max(e) >= 100.0 {
            warnings.push(format!("Rows estimate off ×{} (estimated {}, actual {}) — run ANALYZE; stale statistics lead to bad plans", grouped(ratio), grouped(e), grouped(a)));
        }
    }
    if removed >= 1000.0 {
        let share = removed / (removed + out_rows.unwrap_or(0.0)) * 100.0;
        if share >= 50.0 {
            warnings.push(format!("Filter removed {:.0}% of rows ({} rows)", share, grouped(removed)));
        }
    }
    if s("Sort Space Type") == "Disk" {
        warnings.push(format!("Sort spilled to disk ({} kB) — raise work_mem or sort fewer rows", s("Sort Space Used")));
    }
    if num(n, "Hash Batches").is_some_and(|b| b > 1.0) {
        warnings.push(format!("Hash spilled to disk ({} batches) — raise work_mem", s("Hash Batches")));
    }

    let props = n.iter().filter(|(k, _)| *k != "Plans").map(|(k, v)| (k.clone(), text(v))).collect();
    PlanNode { op, detail, startup: num(n, "Startup Cost"), total, rows, width: num(n, "Plan Width"), actual_time, actual_rows, loops, self_time, self_cost, pct: 0.0, warnings, props, children }
}

/// 전체 대비 몫 (%) 을 단다: ANALYZE 면 시간, 아니면 비용
fn shares(n: &mut PlanNode, whole: f64, by_time: bool) {
    let own = if by_time { n.self_time } else { n.self_cost };
    n.pct = if whole > 0.0 { (own.unwrap_or(0.0) / whole * 100.0).min(100.0) } else { 0.0 };
    for c in &mut n.children {
        shares(c, whole, by_time);
    }
}

/// EXPLAIN (FORMAT JSON) 결과 글 → 트리
pub fn parse_pg(json: &str) -> AppResult<PlanNode> {
    let v: Value = serde_json::from_str(json.trim()).map_err(|e| AppError::Msg(format!("Not an EXPLAIN (FORMAT JSON) result: {e}")))?;
    let root = v.get(0).unwrap_or(&v).get("Plan").and_then(Value::as_object).ok_or_else(|| AppError::Msg("No \"Plan\" in the EXPLAIN result".into()))?;
    let mut p = node(root, false);
    let by_time = p.actual_time.is_some();
    let whole = if by_time { p.actual_time.unwrap_or(0.0) * p.loops.unwrap_or(1.0) } else { p.total.unwrap_or(0.0) };
    shares(&mut p, whole, by_time);
    Ok(p)
}

/// TUI 표: 들여쓴 연산 · % · 자기 시간/비용 · 추정/실제 행 · 경고 · 상세
pub fn to_result(plan: &PlanNode) -> QueryResult {
    let col = |n: &str, t: &str| ColumnInfo { name: n.into(), type_name: t.into() };
    let columns = vec![col("operation", "TEXT"), col("%", "TEXT"), col("self", "TEXT"), col("est rows", "INT8"), col("actual rows", "INT8"), col("loops", "INT8"), col("warnings", "TEXT"), col("detail", "TEXT")];
    let mut rows = vec![];
    fn walk(n: &PlanNode, depth: usize, rows: &mut Vec<Vec<Value>>) {
        let int = |v: Option<f64>| v.map(|x| Value::from(x.round() as i64)).unwrap_or(Value::Null);
        let own = match (n.self_time, n.self_cost) {
            (Some(t), _) => format!("{t:.1} ms"),
            (None, Some(c)) => format!("cost {c:.1}"),
            _ => String::new(),
        };
        let op = if depth == 0 { n.op.clone() } else { format!("{}└ {}", "  ".repeat(depth), n.op) };
        rows.push(vec![Value::from(op), Value::from(format!("{:.0}%", n.pct)), Value::from(own), int(n.rows), int(n.actual_rows), int(n.loops), Value::from(n.warnings.join(" · ")), Value::from(n.detail.clone())]);
        for c in &n.children {
            walk(c, depth + 1, rows);
        }
    }
    walk(plan, 0, &mut rows);
    QueryResult { columns, rows, rows_affected: 0, truncated: false, elapsed_ms: 0, has_result_set: true, in_transaction: false }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ANALYZED: &str = r#"[{"Plan": {"Node Type": "Sort", "Startup Cost": 100.0, "Total Cost": 110.0, "Plan Rows": 10, "Plan Width": 8,
        "Actual Total Time": 50.0, "Actual Rows": 5000, "Actual Loops": 1, "Sort Key": ["id"], "Sort Space Type": "Disk", "Sort Space Used": 2048,
        "Plans": [{"Node Type": "Seq Scan", "Relation Name": "orders", "Schema": "public", "Alias": "orders", "Startup Cost": 0.0, "Total Cost": 90.0,
            "Plan Rows": 10, "Plan Width": 8, "Actual Total Time": 40.0, "Actual Rows": 5000, "Actual Loops": 1,
            "Filter": "(status = 'paid')", "Rows Removed by Filter": 95000}]},
        "Execution Time": 50.5}]"#;

    #[test]
    fn self_time_share_and_warnings() {
        let p = parse_pg(ANALYZED).unwrap();
        assert_eq!(p.op, "Sort");
        assert_eq!(p.self_time, Some(10.0));
        assert_eq!(p.children[0].self_time, Some(40.0));
        assert_eq!((p.pct.round(), p.children[0].pct.round()), (20.0, 80.0));
        assert_eq!(p.children[0].detail, "on public.orders  filter: (status = 'paid')");
        let w = |n: &PlanNode| n.warnings.join(" | ");
        assert!(w(&p).contains("Sort spilled to disk"), "{}", w(&p));
        let scan = w(&p.children[0]);
        assert!(scan.contains("Rows estimate off ×500"), "{scan}");
        assert!(scan.contains("Filter removed 95% of rows"), "{scan}");
        assert!(scan.contains("Seq Scan"), "{scan}");
        assert!(p.props.iter().any(|(k, v)| k == "Sort Space Used" && v == "2048"));
    }

    /// 병렬: Gather 아래 노드는 워커들이 동시에 돈다 → 시간을 워커 수만큼 곱하지 않는다 (100% 를 넘지 않게)
    #[test]
    fn parallel_workers_are_not_counted_twice() {
        let p = parse_pg(r#"[{"Plan": {"Node Type": "Gather Merge", "Total Cost": 100, "Actual Total Time": 45.0, "Actual Loops": 1, "Plan Rows": 12, "Actual Rows": 766,
            "Plans": [{"Node Type": "Sort", "Total Cost": 90, "Actual Total Time": 25.8, "Actual Loops": 2, "Plan Rows": 12, "Actual Rows": 383,
              "Plans": [{"Node Type": "Seq Scan", "Relation Name": "big_t", "Total Cost": 80, "Actual Total Time": 25.2, "Actual Loops": 2, "Plan Rows": 12, "Actual Rows": 383}]}]}}]"#).unwrap();
        let scan = &p.children[0].children[0];
        assert_eq!(scan.self_time, Some(25.2));
        assert!(scan.pct <= 100.0 && (scan.pct - 56.0).abs() < 1.0, "{}", scan.pct);
        assert!((p.self_time.unwrap() - 19.2).abs() < 0.01);
    }

    #[test]
    fn cost_share_without_analyze_and_table_for_tui() {
        let p = parse_pg(r#"[{"Plan": {"Node Type": "Hash Join", "Total Cost": 200, "Plan Rows": 1, "Join Type": "Inner", "Hash Cond": "(a.id = b.id)",
            "Plans": [{"Node Type": "Seq Scan", "Relation Name": "a", "Total Cost": 150, "Plan Rows": 50000}, {"Node Type": "Hash", "Total Cost": 30, "Plan Rows": 10,
              "Plans": [{"Node Type": "Index Scan", "Relation Name": "b", "Index Name": "b_pkey", "Total Cost": 30, "Plan Rows": 10}]}]}}]"#).unwrap();
        assert_eq!(p.self_cost, Some(20.0));
        assert_eq!(p.children[0].pct.round(), 75.0);
        assert!(p.children[0].warnings.iter().any(|w| w.contains("Seq Scan")), "big table scan");
        let t = to_result(&p);
        let names: Vec<_> = t.columns.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["operation", "%", "self", "est rows", "actual rows", "loops", "warnings", "detail"]);
        let ops: Vec<String> = t.rows.iter().map(|r| crate::db::cell_text(&r[0])).collect();
        assert_eq!(ops, ["Hash Join", "  └ Seq Scan", "  └ Hash", "    └ Index Scan"]);
        assert!(parse_pg("not json").is_err());
    }
}
