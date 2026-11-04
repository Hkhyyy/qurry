//! The shared grid helpers (format, labels, extract, fuzzy, keytree, automap) reproduce both dumps from before they moved here:
//! goldens/rust with the default options (what the TUI did) and goldens/ts with the GUI's options (D2: the GUI keeps its rules).
//! `cargo test -p qurry-tui parity_dump -- --ignored` and `pnpm parity:dump` write the goldens; inputs are in corpus/.
use qurry_shared::extract::{self, Format, Rows};
use qurry_shared::{automap, format, fuzzy, keytree, labels, DbKind};
use serde_json::{json, Map, Value};

fn read(path: &str) -> Value {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join(path);
    serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap()
}

fn strings(v: &Value) -> Vec<String> {
    v.as_array().unwrap().iter().map(|x| x.as_str().unwrap().to_string()).collect()
}

/// What the GUI passes (src/lib/inList.ts, extract.ts …)
const GUI: extract::Options = extract::Options { strict_numbers: true, js_numbers: true };

/// Only the functions both goldens have, compared on what this side computed (a golden may hold functions not moved here)
fn check(file: &str, golden: &Value, id: &str, got: &Map<String, Value>) {
    for (f, v) in got {
        let want = &golden[id][f];
        // Scores are compared as numbers (the TUI wrote integers, the shared function returns f64)
        let same = match (want.as_f64(), v.as_f64()) {
            (Some(a), Some(b)) => a == b,
            _ => want == v,
        };
        assert!(same, "{file} {id} {f}:\n  want {want}\n  got  {v}");
    }
}

fn names_types(e: &Value) -> (Vec<String>, Vec<String>) {
    let cols = e["columns"].as_array().unwrap();
    (cols.iter().map(|c| c["name"].as_str().unwrap().to_string()).collect(), cols.iter().map(|c| c["typeName"].as_str().unwrap().to_string()).collect())
}

fn rows_of(e: &Value) -> Vec<Vec<Value>> {
    e["rows"].as_array().unwrap().iter().map(|r| r.as_array().unwrap().clone()).collect()
}

fn column(rows: &[Vec<Value>], c: usize) -> Vec<Vec<Value>> {
    rows.iter().map(|r| vec![r[c].clone()]).collect()
}

fn frac_widths(names: &[String], types: &[String], rows: &[Vec<Value>]) -> Vec<usize> {
    (0..names.len()).map(|c| if format::is_time_type(&types[c]) { format::frac_width(rows.iter().filter_map(|r| r[c].as_str())) } else { 0 }).collect()
}

fn pad_fracs(names: &[String], types: &[String], rows: &[Vec<Value>], widths: &[usize]) -> Value {
    let m: Map<String, Value> = (0..names.len())
        .filter(|&c| format::is_time_type(&types[c]))
        .map(|c| (names[c].clone(), rows.iter().map(|r| r[c].as_str().map(|s| Value::from(format::pad_frac(s, widths[c]))).unwrap_or_else(|| r[c].clone())).collect()))
        .collect();
    m.into()
}

#[test]
fn results_reproduce_both_goldens() {
    let (rust, ts) = (read("goldens/rust/results.json"), read("goldens/ts/results.json"));
    for e in read("corpus/results.json").as_array().unwrap() {
        let id = e["id"].as_str().unwrap();
        let (names, types) = names_types(e);
        let rows = rows_of(e);
        let widths = frac_widths(&names, &types, &rows);
        let fracs: Map<String, Value> = names.iter().zip(&widths).map(|(n, w)| (n.clone(), (*w).into())).collect();
        let col_lists = |o: extract::Options| -> Value { names.iter().enumerate().map(|(c, n)| (n.clone(), Value::from(extract::in_list(&column(&rows, c), &types[c..=c], DbKind::Postgres, o).0))).collect::<Map<_, _>>().into() };

        let mut tui = Map::new();
        tui.insert("inList".into(), extract::in_list(&rows, &types, DbKind::Postgres, extract::Options::default()).0.into());
        tui.insert("inList(mysql)".into(), extract::in_list(&rows, &types, DbKind::Mysql, extract::Options::default()).0.into());
        tui.insert("inList(column)".into(), col_lists(extract::Options::default()));
        tui.insert("fracWidth".into(), fracs.clone().into());
        tui.insert("padFrac".into(), pad_fracs(&names, &types, &rows, &widths));
        if !rows.is_empty() {
            let x = Rows { vals: rows.clone(), ..Default::default() };
            tui.insert("extract(TSV, no header)".into(), extract::rows(Format::Tsv, &x, DbKind::Postgres, extract::Options::default()).into());
        }
        check("rust/results", &rust, id, &tui);

        let id_col = names.iter().position(|n| n == "id");
        let keys = rows.iter().map(|r| id_col.map(|i| vec![("id".to_string(), r[i].clone())]).unwrap_or_default()).collect();
        let base = Rows { names: names.clone(), types: types.clone(), vals: rows.clone(), keys, table_ref: "MY_TABLE".into(), header: true };
        let mut gui = Map::new();
        gui.insert("inList".into(), extract::in_list(&rows, &types, DbKind::Postgres, GUI).0.into());
        gui.insert("inList(mysql)".into(), extract::in_list(&rows, &types, DbKind::Mysql, GUI).0.into());
        gui.insert("inList(column)".into(), col_lists(GUI));
        gui.insert("fracWidth".into(), fracs.into());
        gui.insert("padFrac".into(), pad_fracs(&names, &types, &rows, &widths));
        gui.insert("extract(TSV, no header)".into(), extract::rows(Format::Tsv, &Rows { header: false, ..base.clone() }, DbKind::Postgres, GUI).into());
        gui.insert("extract(SQL Inserts, mysql)".into(), extract::rows(Format::SqlInserts, &base, DbKind::Mysql, GUI).into());
        for f in Format::ALL {
            gui.insert(format!("extract({})", f.label()), extract::rows(f, &base, DbKind::Postgres, GUI).into());
        }
        check("ts/results", &ts, id, &gui);
    }
}

#[test]
fn tab_labels_reproduce_both_goldens() {
    for golden in ["rust", "ts"] {
        let g = read(&format!("goldens/{golden}/tablabels.json"));
        for e in read("corpus/tablabels.json").as_array().unwrap() {
            let tabs: Vec<labels::TableTab> = serde_json::from_value(e["tabs"].clone()).unwrap();
            let got: Vec<String> = labels::table_tab_labels(&tabs).into_iter().zip(&tabs).map(|(l, t)| l.unwrap_or_else(|| t.table.clone())).collect();
            check(golden, &g, e["id"].as_str().unwrap(), &Map::from_iter([("tableTabLabels".to_string(), json!(got))]));
        }
    }
}

fn tree(nodes: &[keytree::KeyNode], keys: &[String]) -> Value {
    nodes
        .iter()
        .map(|n| match n {
            keytree::KeyNode::Key { item } => json!({ "key": keys[*item] }),
            keytree::KeyNode::Folder { name, path, count, children } => json!({ "folder": name, "path": path, "count": count, "children": tree(children, keys) }),
        })
        .collect()
}

#[test]
fn key_trees_reproduce_both_goldens() {
    for (golden, opts) in [("rust", keytree::Options::default()), ("ts", keytree::Options { utf16_order: true })] {
        let g = read(&format!("goldens/{golden}/rediskeys.json"));
        for e in read("corpus/rediskeys.json").as_array().unwrap() {
            let keys = strings(&e["keys"]);
            let got = tree(&keytree::key_tree(&keys, opts), &keys);
            check(golden, &g, e["id"].as_str().unwrap(), &Map::from_iter([("keyTree".to_string(), got)]));
        }
    }
}

#[test]
fn auto_map_reproduces_both_goldens() {
    for golden in ["rust", "ts"] {
        let g = read(&format!("goldens/{golden}/columns.json"));
        for e in read("corpus/columns.json").as_array().unwrap() {
            let id = e["id"].as_str().unwrap();
            let table = (!e["table"].is_null()).then(|| strings(&e["table"]));
            // The TUI only maps onto an existing table (its golden has no entry for a new one)
            if golden == "rust" && table.is_none() {
                continue;
            }
            let got = automap::auto_map(&strings(&e["file"]), table.as_deref());
            check(golden, &g, id, &Map::from_iter([("autoMap".to_string(), json!(got))]));
        }
    }
}

/// Rules the TUI and the MCP server took, with no option, when these helpers moved here. The goldens hold the new output
/// (corpus ids loose-dates, korean-gap and bom-nel)
#[test]
fn rules_the_tui_and_mcp_changed_to() {
    // TUI time fractions take the GUI's pattern: a date before the time is 4+ digits-dd-dd at the start. The TUI padded any time
    // after the first space or T when the 3rd char before it was - (+2026-06-06 10:00:00 and abcd-ef-gh 10:00:00 too)
    assert_eq!(format::pad_frac("+2026-06-06 10:00:00", 3), "+2026-06-06 10:00:00");
    assert_eq!(format::pad_frac("abcd-ef-gh 10:00:00", 3), "abcd-ef-gh 10:00:00");
    // The TUI palette takes MCP search_objects' gap between letters: skipped chars. It counted UTF-8 bytes from the byte after
    // the matched letter's first, so a skipped or matched non-ASCII letter cost more (197 and 198 here)
    let plain = |q: &str, t: &str| fuzzy::score(q, t, fuzzy::Options::default());
    assert_eq!(plain("ac", "a가c"), Some(199.0));
    assert_eq!(plain("가c", "가bc"), plain("ac", "abc"));
    assert_eq!(plain("가 나", "가나x"), Some(200.0));
    // The TUI's import column matching takes the GUI's spaces (JavaScript's \s): a BOM is one, NEL is not (it used char::is_whitespace)
    let cols = ["a".to_string()];
    assert_eq!(automap::auto_map(&["a\u{feff}".into(), "a\u{85}".into()], Some(&cols)), ["a", ""]);
}

/// Value class and default alignment of a grid cell: one golden, the same for the TUI (here) and the GUI (src/lib/gridAlign.test.ts, through WASM).
/// "column" = the column's values (only an untyped column's NULL reads them), else the cell alone
#[test]
fn value_classes_match_the_golden() {
    for e in read("goldens/valueclass.json").as_array().unwrap() {
        let t = e["type"].as_str().unwrap();
        let class = format::value_class(t, &e["value"]);
        let column = e.get("column").and_then(|c| c.as_array()).cloned().unwrap_or_else(|| vec![e["value"].clone()]);
        let got = json!({ "class": class, "align": format::cell_align(format::column_class(t, column.iter()), class, &format::Alignments::new()) });
        assert_eq!(got, json!({ "class": e["class"], "align": e["align"] }), "{e}");
    }
}

/// JSON colors of a grid cell: one golden for the TUI (here) and the GUI (src/lib/jsonSpans.test.ts, through WASM), as text pieces
/// so byte (Rust) and UTF-16 (GUI) offsets compare alike
#[test]
fn json_spans_match_the_golden() {
    for e in read("goldens/jsonspans.json").as_array().unwrap() {
        let text = e["text"].as_str().unwrap();
        let got: Vec<Value> = format::json_spans(text, e["max"].as_u64().unwrap() as usize).into_iter().map(|(s, end, k)| json!([&text[s..end], k])).collect();
        assert_eq!(json!(got), e["spans"], "{}", e["id"]);
    }
}

#[test]
fn fuzzy_reproduces_both_goldens() {
    for (golden, opts) in [("rust", fuzzy::Options::default()), ("ts", fuzzy::Options { word_starts: true })] {
        let g = read(&format!("goldens/{golden}/fuzzy.json"));
        for e in read("corpus/fuzzy.json").as_array().unwrap() {
            let got = fuzzy::score(e["query"].as_str().unwrap(), e["text"].as_str().unwrap(), opts);
            check(golden, &g, e["id"].as_str().unwrap(), &Map::from_iter([("fuzzyScore".to_string(), json!(got))]));
        }
    }
}
