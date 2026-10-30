//! Parity dump (`cargo test -p qurry-tui parity_dump -- --ignored`): runs today's TUI · core functions over crates/shared/tests/corpus
//! and writes crates/shared/tests/goldens/rust/*.json — same ids, keys (the GUI function names) and output shapes as src/lib/parity.dump.test.ts.
//! `pnpm parity:dump` then compares both sides into crates/shared/tests/DIVERGENCES.md. A `‸` in a corpus text marks the cursor (byte offset here).
use crate::app::{Tab, TabKind, TableTab};
use crate::complete::{self, HintAt, HintMode, Meta};
use crate::exec::{self, ExecSettings, Inside, Outside, Plan};
use crate::grid::{self, Grid};
use crate::tree::{self, Node, NodeKind};
use qurry_core::db::{ColumnInfo, QueryResult};
use qurry_core::hints::Hint;
use qurry_core::meta::CompletionTable;
use qurry_core::store::DbKind;
use qurry_core::{guard, sql};
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::path::PathBuf;

type Golden = Map<String, Value>;

const CARET: &str = "‸";

/// Settings combinations dumped for every input with a cursor (same list in parity.dump.test.ts)
const PLANS: [(&str, Inside, Outside); 6] = [
    ("ask", Inside::Ask, Outside::Nothing),
    ("smallest", Inside::Smallest, Outside::Nothing),
    ("whole", Inside::Whole, Outside::Nothing),
    ("fromCaret", Inside::FromCaret, Outside::Nothing),
    ("outside:whole", Inside::Ask, Outside::Whole),
    ("outside:below", Inside::Ask, Outside::Below),
];

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../shared/tests")
}

fn corpus(name: &str) -> Value {
    let text = std::fs::read_to_string(root().join("corpus").join(format!("{name}.json"))).unwrap();
    serde_json::from_str(&text).unwrap()
}

fn arr(v: &Value) -> &Vec<Value> {
    v.as_array().unwrap()
}

fn st(v: &Value) -> &str {
    v.as_str().unwrap()
}

fn strings(v: &Value) -> Vec<String> {
    arr(v).iter().map(|x| st(x).to_string()).collect()
}

fn kind(d: &str) -> DbKind {
    serde_json::from_value(json!(d)).unwrap()
}

fn slices(text: &str, segs: &[(usize, usize)]) -> Value {
    segs.iter().map(|&(a, b)| text[a..b].to_string()).collect::<Vec<_>>().into()
}

fn plan(p: Plan) -> Value {
    match p {
        Plan::Nothing => Value::Null,
        Plan::Run(v) => json!({ "run": v }),
        Plan::Ask(opts) => json!({ "ask": opts.into_iter().map(|(label, stmts)| json!({ "label": label, "stmts": stmts })).collect::<Vec<_>>() }),
    }
}

fn sql_golden() -> Golden {
    let mut out = Golden::new();
    for e in arr(&corpus("sql")) {
        let k = kind(st(&e["dialect"]));
        let at = st(&e["text"]).find(CARET);
        let text = st(&e["text"]).replacen(CARET, "", 1);
        let mut o = Golden::new();
        o.insert("splitStatements".into(), slices(&text, &sql::split_statements(&text, k)));
        o.insert("splitScript".into(), slices(&text, &sql::split_script(&text, k)));
        if let Some(at) = at {
            o.insert("statementAt".into(), sql::statement_at(&text, at, k).into());
            for (name, inside, outside) in PLANS {
                o.insert(format!("planExecution({name})"), plan(exec::plan(&text, at, k, ExecSettings { inside, outside, ..Default::default() })));
            }
        }
        out.insert(st(&e["id"]).into(), o.into());
    }
    out
}

fn stmt_golden() -> Golden {
    arr(&corpus("statements"))
        .iter()
        .map(|e| {
            let (q, k) = (st(&e["sql"]), kind(st(&e["dialect"])));
            (st(&e["id"]).to_string(), json!({ "isReadStmt": guard::is_read(q, k), "DDL_RE": guard::is_ddl(q, k) }))
        })
        .collect()
}

fn completion_golden() -> Golden {
    let c = corpus("completion");
    let tables = arr(&c["tables"])
        .iter()
        .map(|t| CompletionTable { schema: st(&t["schema"]).into(), name: st(&t["name"]).into(), columns: arr(&t["columns"]).iter().map(|p| (st(&p[0]).into(), st(&p[1]).into())).collect() })
        .collect();
    let m = Meta::new(tables);
    let found = |r: &str| Value::from(complete::find_table(&m, r, "").map(|t| format!("{}.{}", t.schema, t.name)));
    let mut out = Golden::new();
    for e in arr(&c["cases"]) {
        let refs = complete::tables_in_statement(st(&e["text"]));
        out.insert(
            st(&e["id"]).into(),
            json!({
                "tablesInStatement": refs.iter().map(|(r, a)| json!({ "ref": r, "alias": a })).collect::<Vec<_>>(),
                "findTable": refs.iter().map(|(r, _)| found(r)).collect::<Vec<_>>(),
            }),
        );
    }
    for r in arr(&c["refs"]) {
        out.insert(format!("ref:{}", st(r)), json!({ "findTable": found(st(r)) }));
    }
    out
}

fn hint_golden() -> Golden {
    let mut out = Golden::new();
    for e in arr(&corpus("valuehints")) {
        let (before, ty, value) = (st(&e["before"]), st(&e["typeName"]), st(&e["value"]));
        let mysql = e.get("mysql").and_then(Value::as_bool).unwrap_or(false);
        let mut o = Golden::new();
        let Some(c) = complete::context(before, mysql) else {
            o.insert("hintContext".into(), Value::Null);
            out.insert(st(&e["id"]).into(), o.into());
            continue;
        };
        let mode = match c.mode {
            HintMode::Values => "values",
            HintMode::Keys => "keys",
            HintMode::Pairs => "pairs",
        };
        let op = (!c.op.is_empty()).then(|| c.op.clone());
        o.insert("hintContext".into(), json!({ "ref": c.r#ref, "column": c.column, "path": c.path, "mode": mode, "quoted": c.quoted, "typed": c.typed, "op": op }));
        // hint_items builds the inserted text (typed left empty: its prefix filter isn't part of hintInsert)
        let h = HintAt { schema: String::new(), table: String::new(), column: c.column.clone(), type_name: ty.into(), path: c.path.clone(), mode: c.mode, quoted: c.quoted, typed: String::new(), op: c.op.clone() };
        let hints = [Hint { value: value.into(), count: 1 }];
        o.insert("hintInsert".into(), complete::hint_items(&h, &hints, false, false)[0].insert.clone().into());
        // hint_at decides hintable: a one-table statement whose only column is the hinted one
        let name = c.r#ref.clone().unwrap_or_else(|| "t".into());
        let m = Meta::new(vec![CompletionTable { schema: "public".into(), name: name.clone(), columns: vec![(c.column.clone(), ty.into())] }]);
        o.insert("hintable".into(), complete::hint_at(&m, &format!("select * from \"{name}\""), before, "", mysql).is_some().into());
        if c.mode == HintMode::Keys {
            o.insert("pairsForKeys".into(), complete::hint_items(&h, &hints, false, true)[0].insert.clone().into());
        }
        out.insert(st(&e["id"]).into(), o.into());
    }
    out
}

fn result_of(e: &Value) -> QueryResult {
    QueryResult {
        columns: arr(&e["columns"]).iter().map(|c| ColumnInfo { name: st(&c["name"]).into(), type_name: st(&c["typeName"]).into() }).collect(),
        rows: arr(&e["rows"]).iter().map(|r| arr(r).clone()).collect(),
        rows_affected: 0,
        truncated: false,
        elapsed_ms: 0,
        has_result_set: true,
        in_transaction: false,
    }
}

/// Whole result selected (anchor top-left, cursor bottom-right)
fn all_selected(e: &Value) -> Grid {
    let mut g = Grid::new(result_of(e));
    g.anchor = Some((0, 0));
    g.row = g.rows().saturating_sub(1);
    g.col = g.cols().saturating_sub(1);
    g
}

fn result_golden() -> Golden {
    let mut out = Golden::new();
    for e in arr(&corpus("results")) {
        let r = result_of(e);
        let names: Vec<&str> = r.columns.iter().map(|c| c.name.as_str()).collect();
        // The TUI's chart picks its axes inside bar_chart: read them back from its title ("Chart: sum of Y by X" / "Chart: row count by X")
        let axes = crate::datatools::bar_chart(&r, 40, 30).map(|(title, _)| {
            let (what, x) = title.trim_start_matches("Chart: ").split_once(" by ").unwrap();
            json!({ "x": x, "y": what.strip_prefix("sum of ") })
        });
        let col = |c: usize| r.rows.iter().map(move |row| &row[c]);
        let frac = |c: usize| col(c).filter_map(|v| v.as_str().and_then(grid::time_frac)).map(|x| x.1).max().unwrap_or(0);
        let column_g = Grid::new(result_of(e));
        let mut o = json!({
            "defaultAxes": axes,
            "inList": all_selected(e).in_list().0,
            "inList(column)": names.iter().enumerate().map(|(c, n)| (n.to_string(), Value::from(column_g.column_in_list(c).0))).collect::<Map<_, _>>(),
            "fracWidth": names.iter().enumerate().map(|(c, n)| (n.to_string(), Value::from(frac(c)))).collect::<Map<_, _>>(),
            "padFrac": names
                .iter()
                .enumerate()
                .filter(|(c, _)| {
                    let t = r.columns[*c].type_name.to_uppercase();
                    t.contains("TIME") && !t.contains("INTERVAL")
                })
                .map(|(c, n)| {
                    let w = frac(c);
                    (n.to_string(), col(c).map(|v| v.as_str().map(|s| Value::from(grid::pad_frac(s, w))).unwrap_or_else(|| v.clone())).collect::<Value>())
                })
                .collect::<Map<_, _>>(),
        });
        // An empty grid has no cells to select (selecting "all" there would copy a stray tab)
        if !r.rows.is_empty() {
            o["extract(TSV, no header)"] = all_selected(e).selection_tsv().into();
        }
        out.insert(st(&e["id"]).into(), o);
    }
    out
}

fn tab_golden(h: &mut crate::testkit::Harness) -> Golden {
    let mut out = Golden::new();
    for e in arr(&corpus("tablabels")) {
        h.app.tabs = arr(&e["tabs"])
            .iter()
            .map(|t| {
                let table = st(&t["table"]).to_string();
                let tt = TableTab { schema: st(&t["schema"]).into(), table: table.clone(), kind: "table".into(), where_: String::new(), order: String::new(), session: String::new(), grid: None, running: false, pk: vec![], error: None };
                Tab { id: st(&t["id"]).parse().unwrap(), title: table, conn: "c".into(), db: st(&t["db"]).into(), target: String::new(), kind: TabKind::Table(tt), group: 0, stale: false }
            })
            .collect();
        let labels: Vec<String> = h.app.tabs.iter().map(|t| h.app.tab_label(t)).collect();
        out.insert(st(&e["id"]).into(), json!({ "tableTabLabels": labels }));
    }
    h.app.tabs.clear();
    out
}

fn key_tree(nodes: &[Node], ch: &HashMap<String, Vec<Node>>, prefix: &str) -> Value {
    nodes
        .iter()
        .map(|n| {
            if n.kind != NodeKind::Group {
                return json!({ "key": n.label });
            }
            let path = if prefix.is_empty() { n.label.clone() } else { format!("{prefix}:{}", n.label) };
            json!({ "folder": n.label, "path": path, "count": n.hint.parse::<u64>().unwrap(), "children": key_tree(&ch[&n.key], ch, &path) })
        })
        .collect()
}

fn key_golden() -> Golden {
    let key = |name: String| Node { key: format!("g|o:{name}"), label: name.clone(), hint: "string".into(), kind: NodeKind::Object("key".into()), conn: "c".into(), db: Some("0".into()), schema: Some("0".into()), name: Some(name), expandable: false };
    arr(&corpus("rediskeys"))
        .iter()
        .map(|e| {
            let mut ch = HashMap::new();
            let top = tree::key_folders(&mut ch, "g", strings(&e["keys"]).into_iter().map(key).collect());
            (st(&e["id"]).to_string(), json!({ "keyTree": key_tree(&top, &ch, "") }))
        })
        .collect()
}

fn column_golden() -> Golden {
    let mut out = Golden::new();
    for e in arr(&corpus("columns")) {
        // A new table (null) has no counterpart: the TUI only maps onto an existing table
        if e["table"].is_null() {
            out.insert(st(&e["id"]).into(), json!({}));
            continue;
        }
        let table = strings(&e["table"]);
        let mapped: Vec<String> = strings(&e["file"]).iter().map(|f| crate::transfer::match_column(f, &table).cloned().unwrap_or_default()).collect();
        out.insert(st(&e["id"]).into(), json!({ "autoMap": mapped }));
    }
    out
}

fn spec_golden() -> Golden {
    strings(&corpus("keyspecs")).into_iter().map(|s| (s.clone(), json!({ "keyLabel": crate::keys::lower_unshifted(&s), "formatSpec": crate::keys::disp(&s) }))).collect()
}

fn fuzzy_golden() -> Golden {
    arr(&corpus("fuzzy")).iter().map(|e| (st(&e["id"]).to_string(), json!({ "fuzzyScore": crate::app::fuzzy(st(&e["query"]), st(&e["text"])) }))).collect()
}

#[tokio::test]
#[ignore = "writes goldens: cargo test -p qurry-tui parity_dump -- --ignored"]
async fn parity_dump() {
    let mut h = crate::testkit::Harness::new().await;
    let goldens = [
        ("sql", sql_golden()),
        ("statements", stmt_golden()),
        ("completion", completion_golden()),
        ("valuehints", hint_golden()),
        ("results", result_golden()),
        ("tablabels", tab_golden(&mut h)),
        ("rediskeys", key_golden()),
        ("columns", column_golden()),
        ("keyspecs", spec_golden()),
        ("fuzzy", fuzzy_golden()),
    ];
    let dir = root().join("goldens/rust");
    std::fs::create_dir_all(&dir).unwrap();
    for (name, g) in goldens {
        std::fs::write(dir.join(format!("{name}.json")), serde_json::to_string_pretty(&g).unwrap() + "\n").unwrap();
    }
}
