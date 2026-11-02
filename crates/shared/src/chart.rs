//! Result table → chart data: the GUI chart (ResultChart, through WASM) and the TUI bar list (⌃K › Chart Result).
//! Same X values are merged: each Y is summed over their rows, or the rows are counted. Where the two differ, `Options` names it (D2).

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::cmp::Ordering;
use std::collections::HashMap;

/// A result column ({ name, typeName } as the GUI has it)
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Column {
    pub name: String,
    pub type_name: String,
}

/// X order when X is neither a date/time nor a numeric column (those are always ascending)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Order {
    /// As the rows first show each X (GUI line and bar charts)
    FirstSeen,
    /// Largest first by the first Y (TUI bar list)
    LargestFirst,
}

/// What differs between the GUI and the TUI chart
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    /// A YEAR column is numeric by its type (GUI); otherwise its values decide (TUI)
    pub year_is_number: bool,
    pub order: Order,
}

impl Options {
    pub const GUI: Options = Options { year_is_number: true, order: Order::FirstSeen };
    pub const TUI: Options = Options { year_is_number: false, order: Order::LargestFirst };
}

/// Initial axes: the X column, and Y (None = the row count)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Axes {
    pub x: usize,
    pub y: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Series {
    pub name: String,
    pub values: Vec<f64>,
}

/// One label per X value, one series per Y (values in label order)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChartData {
    pub labels: Vec<String>,
    pub series: Vec<Series>,
}

/// Type-name prefixes of numeric columns (case-insensitive; "int" also covers int2 · int4 · int8 · integer)
const NUM_TYPES: &[&str] = &["int", "smallint", "bigint", "tinyint", "mediumint", "numeric", "decimal", "float", "double", "real", "money"];

fn is_time(type_lower: &str) -> bool {
    type_lower.contains("date") || type_lower.contains("time")
}

/// A cell as a number, as the GUI's JavaScript read it: JSON numbers, and strings through Number() once thousands commas are dropped
pub fn number(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => js_string_number(&s.replace(',', "")),
        _ => None,
    }
    .filter(|f| f.is_finite())
}

/// JavaScript's Number(string): surrounding whitespace ignored, blank → 0, 0x · 0o · 0b integer literals
fn js_string_number(s: &str) -> Option<f64> {
    // JavaScript whitespace: Unicode White_Space except U+0085, plus the BOM
    let t = s.trim_matches(|c: char| (c.is_whitespace() && c != '\u{85}') || c == '\u{feff}');
    if t.is_empty() {
        return Some(0.0);
    }
    let radix = match t.get(..2) {
        Some("0x" | "0X") => 16,
        Some("0o" | "0O") => 8,
        Some("0b" | "0B") => 2,
        _ => return t.parse().ok(),
    };
    let digits = &t[2..];
    if digits.is_empty() {
        return None;
    }
    digits.chars().try_fold(0.0, |n, c| c.to_digit(radix).map(|d| n * f64::from(radix) + f64::from(d)))
}

/// Numeric by type, or (not a date/time column) at least 90% of the first 200 non-empty values are numbers
/// (the GUI sends only those 200 rows for this: SAMPLE in src/lib/chart.ts)
fn is_numeric(cols: &[Column], rows: &[Vec<Value>], i: usize, opts: Options) -> bool {
    let t = cols[i].type_name.to_lowercase();
    if NUM_TYPES.iter().any(|p| t.starts_with(p)) || (opts.year_is_number && t.starts_with("year")) || t.contains("unsigned") {
        return true;
    }
    if is_time(&t) {
        return false;
    }
    let vals: Vec<&Value> = rows.iter().take(200).filter_map(|r| r.get(i)).filter(|v| !v.is_null() && v.as_str() != Some("")).collect();
    !vals.is_empty() && vals.iter().filter(|v| number(v).is_some()).count() * 10 >= vals.len() * 9
}

/// Columns that can be charted as numbers
pub fn numeric_columns(cols: &[Column], rows: &[Vec<Value>], opts: Options) -> Vec<usize> {
    (0..cols.len()).filter(|&i| is_numeric(cols, rows, i, opts)).collect()
}

/// X = the first non-numeric column (else the first column); Y = the first numeric column other than X, id-like names last (else the row count)
pub fn default_axes(cols: &[Column], rows: &[Vec<Value>], opts: Options) -> Axes {
    let nums = numeric_columns(cols, rows, opts);
    let x = (0..cols.len()).find(|i| !nums.contains(i)).unwrap_or(0);
    let id = |i: &usize| {
        let n = cols[*i].name.to_lowercase();
        n == "id" || n.ends_with("_id") || n.starts_with("id_")
    };
    let cands: Vec<usize> = nums.into_iter().filter(|&i| i != x).collect();
    Axes { x, y: cands.iter().copied().find(|i| !id(i)).or(cands.first().copied()) }
}

/// X label of a cell, as the GUI prints it (null → "(null)", numbers as JavaScript writes them)
fn label(v: Option<&Value>) -> String {
    match v {
        None | Some(Value::Null) => "(null)".into(),
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) if n.is_f64() => js_number(n.as_f64().unwrap_or(f64::NAN)),
        Some(other) => other.to_string(),
    }
}

/// Sums each Y (None = count rows) per X value. Date/time X in text order, numeric X ascending, others per `opts.order`
pub fn aggregate(cols: &[Column], rows: &[Vec<Value>], x: usize, ys: &[Option<usize>], opts: Options) -> ChartData {
    let mut order: Vec<String> = vec![];
    let mut sums: HashMap<String, Vec<f64>> = HashMap::new();
    for row in rows {
        let k = label(row.get(x));
        let acc = sums.entry(k).or_insert_with_key(|k| {
            order.push(k.clone());
            vec![0.0; ys.len()]
        });
        for (a, y) in acc.iter_mut().zip(ys) {
            *a += match y {
                None => 1.0,
                Some(y) => row.get(*y).and_then(number).unwrap_or(0.0),
            };
        }
    }
    let t = cols.get(x).map(|c| c.type_name.to_lowercase()).unwrap_or_default();
    if is_time(&t) {
        order.sort();
    } else if x < cols.len() && is_numeric(cols, rows, x, opts) {
        let n = |k: &str| number(&Value::String(k.into())).unwrap_or(0.0);
        order.sort_by(|a, b| n(a).partial_cmp(&n(b)).unwrap_or(Ordering::Equal));
    } else if opts.order == Order::LargestFirst && !ys.is_empty() {
        order.sort_by(|a, b| sums[b][0].total_cmp(&sums[a][0]));
    }
    let series = ys
        .iter()
        .enumerate()
        .map(|(j, y)| Series {
            name: match y {
                None => "count".into(),
                Some(y) => cols.get(*y).map_or("?", |c| &c.name).into(),
            },
            values: order.iter().map(|k| sums[k][j]).collect(),
        })
        .collect();
    ChartData { labels: order, series }
}

/// Pie slices from the first series: positive values, largest first; past max_slices the smallest are folded into "Other"
pub fn pie(data: &ChartData, max_slices: usize) -> ChartData {
    let Some(first) = data.series.first() else {
        return ChartData { labels: vec![], series: vec![] };
    };
    let mut items: Vec<(&String, f64)> = data.labels.iter().zip(first.values.iter().copied()).filter(|(_, v)| *v > 0.0).collect();
    items.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(Ordering::Equal));
    let keep = if items.len() > max_slices { max_slices.saturating_sub(1) } else { items.len() };
    let rest: f64 = items[keep..].iter().fold(0.0, |s, i| s + i.1);
    let (mut labels, mut values): (Vec<String>, Vec<f64>) = items[..keep].iter().map(|(k, v)| ((*k).clone(), *v)).unzip();
    if rest > 0.0 {
        labels.push("Other".into());
        values.push(rest);
    }
    ChartData { labels, series: vec![Series { name: first.name.clone(), values }] }
}

/// Axis ticks covering min..max in 1 · 2 · 5 × 10ⁿ steps (about `count` of them)
pub fn nice_ticks(mut min: f64, mut max: f64, count: f64) -> Vec<f64> {
    if min == max {
        max = min + 1.0;
        min = min.min(0.0);
    }
    let raw = (max - min) / count.max(1.0);
    // Parsed, not powf: on wasm32 powf(10, -5) is not the double nearest 1e-5, and 2 × it then falls short of raw
    let mag: f64 = format!("1e{}", raw.log10().floor()).parse().unwrap_or(f64::NAN);
    let step = [1.0, 2.0, 5.0, 10.0].iter().map(|m| m * mag).find(|s| *s >= raw).unwrap_or(10.0 * mag);
    let lo = (min / step).floor() * step;
    let hi = (max / step).ceil() * step;
    let mut out = vec![];
    let mut v = lo;
    while v <= hi + step / 2.0 {
        // 12 significant digits drop the float error the steps add up (0.30000000000000004 → 0.3); + 0.0 turns -0 into 0
        out.push(format!("{v:.11e}").parse::<f64>().unwrap_or(v) + 0.0);
        v += step;
    }
    out
}

/// Axis label: 1,200 · 15.3K · 1.2M · 2.5B · 0.25
pub fn short_number(n: f64) -> String {
    let a = n.abs();
    for (limit, div, unit) in [(1e9, 1e9, "B"), (1e6, 1e6, "M"), (1e4, 1e3, "K")] {
        if a >= limit {
            return format!("{}{unit}", js_number(to_fixed(n / div, 1)));
        }
    }
    if n.is_finite() && n.fract() == 0.0 {
        // Below 10,000 here, so at most one comma
        let i = a as u64;
        let digits = if i >= 1000 { format!("{},{:03}", i / 1000, i % 1000) } else { i.to_string() };
        return format!("{}{digits}", if n.is_sign_negative() { "-" } else { "" });
    }
    js_number(to_fixed(n, 2))
}

/// `+x.toFixed(digits)`: rounds half away from zero on the exact value (Rust's `{:.N}` rounds a tie to even: 0.125 → 0.12, here 0.13)
fn to_fixed(x: f64, digits: usize) -> f64 {
    if !x.is_finite() || x.abs() >= 1e21 {
        return x;
    }
    // Exact digits (Rust prints them exactly); a tie is a 5 followed only by zeros. 25 more digits are enough for the
    // magnitudes a tie can have here (one or two decimals)
    let exact = format!("{:.*}", digits + 25, x.abs());
    let tail = &exact[exact.len() - 25..];
    let tie = tail.starts_with('5') && tail[1..].bytes().all(|b| b == b'0');
    let y = if tie { x + x.signum() * 0.5 / 10f64.powi(digits as i32) } else { x };
    format!("{y:.digits$}").parse().unwrap_or(x)
}

/// A number as JavaScript's String(n) writes it (0 for -0, exponent outside 1e-6..1e21: 1e+21, 1.5e-7)
fn js_number(x: f64) -> String {
    if x.is_nan() {
        return "NaN".into();
    }
    if x.is_infinite() {
        return if x > 0.0 { "Infinity" } else { "-Infinity" }.into();
    }
    if x == 0.0 {
        return "0".into();
    }
    if (1e-6..1e21).contains(&x.abs()) {
        return x.to_string();
    }
    let s = format!("{x:e}");
    if s.contains("e-") { s } else { s.replacen('e', "e+", 1) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn cols(spec: &[(&str, &str)]) -> Vec<Column> {
        spec.iter().map(|(n, t)| Column { name: n.to_string(), type_name: t.to_string() }).collect()
    }

    fn rows(v: Value) -> Vec<Vec<Value>> {
        serde_json::from_value(v).unwrap()
    }

    /// Result corpus entry → (columns, rows)
    fn result(e: &Value) -> (Vec<Column>, Vec<Vec<Value>>) {
        (serde_json::from_value(e["columns"].clone()).unwrap(), rows(e["rows"].clone()))
    }

    fn golden(text: &str) -> serde_json::Map<String, Value> {
        serde_json::from_str(text).unwrap()
    }

    /// Every number as an f64, so 250 (golden) and 250.0 (computed) compare equal
    fn norm(v: Value) -> Value {
        match v {
            Value::Number(n) => json!(n.as_f64().unwrap()),
            Value::Array(a) => a.into_iter().map(norm).collect(),
            Value::Object(o) => o.into_iter().map(|(k, v)| (k, norm(v))).collect(),
            v => v,
        }
    }

    /// The GUI's goldens (goldens/ts, written by the TS code this replaced) come out the same with Options::GUI
    #[test]
    fn matches_the_gui_goldens() {
        let corpus: Vec<Value> = serde_json::from_str(include_str!("../tests/corpus/results.json")).unwrap();
        let ts = golden(include_str!("../tests/goldens/ts/results.json"));
        for e in &corpus {
            let id = e["id"].as_str().unwrap();
            let g = &ts[id];
            let (c, r) = result(e);
            let names = |is: &[usize]| is.iter().map(|&i| c[i].name.clone()).collect::<Vec<_>>();
            assert_eq!(json!(names(&numeric_columns(&c, &r, Options::GUI))), g["numericColumns"], "{id}: numericColumns");
            let a = default_axes(&c, &r, Options::GUI);
            assert_eq!(json!({ "x": c.get(a.x).map(|c| &c.name), "y": a.y.map(|y| &c[y].name) }), g["defaultAxes"], "{id}: defaultAxes");
            let data = aggregate(&c, &r, a.x, &[a.y], Options::GUI);
            assert_eq!(norm(serde_json::to_value(&data).unwrap()), norm(g["buildChart(bar)"].clone()), "{id}: bar");
            assert_eq!(norm(serde_json::to_value(pie(&data, 10)).unwrap()), norm(g["buildChart(pie)"].clone()), "{id}: pie");
        }
        for (key, g) in golden(include_str!("../tests/goldens/ts/numbers.json")) {
            if let Some(t) = key.strip_prefix("ticks:") {
                let t: Vec<f64> = serde_json::from_str(t).unwrap();
                assert_eq!(norm(json!(nice_ticks(t[0], t[1], t.get(2).copied().unwrap_or(5.0)))), norm(g["niceTicks"].clone()), "{key}");
            } else {
                let n: f64 = key.strip_prefix("short:").unwrap().parse().unwrap();
                assert_eq!(json!(short_number(n)), g["shortNumber"], "{key}");
            }
        }
    }

    /// The TUI's dump (goldens/rust, parity_dump.rs) comes out the same with Options::TUI
    #[test]
    fn matches_the_tui_goldens() {
        let corpus: Vec<Value> = serde_json::from_str(include_str!("../tests/corpus/results.json")).unwrap();
        let rust = golden(include_str!("../tests/goldens/rust/results.json"));
        for e in &corpus {
            let id = e["id"].as_str().unwrap();
            let (c, r) = result(e);
            let nums: Vec<&str> = numeric_columns(&c, &r, Options::TUI).iter().map(|&i| c[i].name.as_str()).collect();
            assert_eq!(json!(nums), rust[id]["numericColumns"], "{id}: numericColumns");
            let a = default_axes(&c, &r, Options::TUI);
            assert_eq!(json!({ "x": c[a.x].name, "y": a.y.map(|y| &c[y].name) }), rust[id]["defaultAxes"], "{id}: defaultAxes");
        }
    }

    #[test]
    fn year_is_numeric_by_type_only_for_the_gui() {
        let c = cols(&[("yr", "YEAR"), ("n", "text")]);
        let r = rows(json!([[null, "1"]]));
        assert_eq!(numeric_columns(&c, &r, Options::GUI), [0, 1]);
        assert_eq!(numeric_columns(&c, &r, Options::TUI), [1]);
        // Without rows the type alone decides
        assert_eq!(default_axes(&cols(&[("a", "text"), ("b", "int4")]), &[], Options::TUI), Axes { x: 0, y: Some(1) });
    }

    #[test]
    fn order_is_first_seen_or_largest_first() {
        let c = cols(&[("k", "text"), ("v", "int4")]);
        let r = rows(json!([["a", 1], ["b", 5], ["a", 1], ["c", 3]]));
        let gui = aggregate(&c, &r, 0, &[Some(1), None], Options::GUI);
        assert_eq!(gui.labels, ["a", "b", "c"]);
        assert_eq!(gui.series, [Series { name: "v".into(), values: vec![2.0, 5.0, 3.0] }, Series { name: "count".into(), values: vec![2.0, 1.0, 1.0] }]);
        assert_eq!(aggregate(&c, &r, 0, &[Some(1)], Options::TUI).labels, ["b", "c", "a"]);
        // Out-of-range indexes (from JS) don't panic
        let odd = aggregate(&c, &r, 7, &[Some(9)], Options::GUI);
        assert_eq!((odd.labels, odd.series[0].name.as_str(), odd.series[0].values.clone()), (vec!["(null)".to_string()], "?", vec![0.0]));
    }

    #[test]
    fn numbers_as_javascript_writes_them() {
        assert_eq!(number(&json!(" 1,200.5 ")), Some(1200.5));
        assert_eq!(number(&json!("NaN")), None);
        assert_eq!(number(&json!(true)), None);
        // As JavaScript's Number(): blank is 0, JavaScript whitespace (BOM, line separator) is trimmed, non-decimal literals unsigned only
        assert_eq!(number(&json!(" ")), Some(0.0));
        assert_eq!(number(&json!(",")), Some(0.0));
        assert_eq!(number(&json!("\u{feff}7\u{2028}")), Some(7.0));
        assert_eq!(number(&json!("\u{85}7")), None);
        assert_eq!(number(&json!("0x10")), Some(16.0));
        assert_eq!(number(&json!("0O7")), Some(7.0));
        assert_eq!(number(&json!("0b11")), Some(3.0));
        assert_eq!(number(&json!("-0x10")), None);
        assert_eq!(number(&json!("0x")), None);
        assert_eq!(number(&json!("1_0")), None);
        assert_eq!(number(&json!("Infinity")), None);
        assert_eq!(short_number(-1000.0), "-1,000");
        assert_eq!(short_number(f64::NAN), "NaN");
        assert_eq!(short_number(f64::INFINITY), "InfinityB");
        assert_eq!(label(Some(&json!(1.0))), "1");
        assert_eq!(label(Some(&json!(12345678901234567890u64))), "12345678901234567890");
        assert_eq!(js_number(1.5e-7), "1.5e-7");
    }
}
