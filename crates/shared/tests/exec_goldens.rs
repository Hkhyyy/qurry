//! qurry_shared::sql and ::exec reproduce goldens/rust/sql.json, the TUI's dump from before they moved here
//! (`cargo test -p qurry-tui parity_dump -- --ignored` writes it; same keys and shapes as the GUI's dump). `‸` in a corpus text is the cursor.
use qurry_shared::exec::{self, ExecSettings, Inside, Outside, Plan};
use qurry_shared::{sql, DbKind};
use serde_json::{json, Value};

/// Same list as parity_dump.rs and parity.dump.test.ts
const PLANS: [(&str, Inside, Outside); 6] = [
    ("ask", Inside::Ask, Outside::Nothing),
    ("smallest", Inside::Smallest, Outside::Nothing),
    ("whole", Inside::Whole, Outside::Nothing),
    ("fromCaret", Inside::FromCaret, Outside::Nothing),
    ("outside:whole", Inside::Ask, Outside::Whole),
    ("outside:below", Inside::Ask, Outside::Below),
];

fn read(path: &str) -> Value {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join(path);
    serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap()
}

fn plan(p: Plan) -> Value {
    let sqls = |v: Vec<exec::Stmt>| v.into_iter().map(|s| s.sql).collect::<Vec<_>>();
    match p {
        Plan::Nothing => Value::Null,
        Plan::Run(v) => json!({ "run": sqls(v) }),
        Plan::Ask(opts) => json!({ "ask": opts.into_iter().map(|(label, stmts)| json!({ "label": label, "stmts": sqls(stmts) })).collect::<Vec<_>>() }),
    }
}

#[test]
fn sql_and_exec_reproduce_rust_goldens() {
    let golden = read("goldens/rust/sql.json");
    let corpus = read("corpus/sql.json");
    for e in corpus.as_array().unwrap() {
        let id = e["id"].as_str().unwrap();
        let k: DbKind = serde_json::from_value(e["dialect"].clone()).unwrap();
        let raw = e["text"].as_str().unwrap();
        let at = raw.find('‸');
        let text = raw.replacen('‸', "", 1);
        let slices = |segs: Vec<(usize, usize)>| Value::from(segs.into_iter().map(|(a, b)| text[a..b].to_string()).collect::<Vec<_>>());
        let mut got = json!({ "splitStatements": slices(sql::split_statements(&text, k)), "splitScript": slices(sql::split_script(&text, k)) });
        if let Some(at) = at {
            got["statementAt"] = sql::statement_at(&text, at, k).into();
            for (name, inside, outside) in PLANS {
                got[format!("planExecution({name})")] = plan(exec::plan(&text, at, k, ExecSettings { inside, outside, ..Default::default() }));
            }
        }
        assert_eq!(got, golden[id], "{id}");
    }
}
