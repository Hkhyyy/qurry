//! The completion latency budget: p95 under 2 ms per request on the 2k-table mock (2,000 tables × 20 columns, a foreign key
//! from each table to the next, a 300-statement script before the caret). Each request is what a front end asks on a key: the
//! caret's statement (complete::at_caret), then its candidates, the call's signature or the statement's unknown names.
//! Timing only means something in a release build:
//!   cargo test --release -p qurry-shared --test complete_latency -- --ignored --nocapture
use qurry_shared::DbKind;
use qurry_shared::complete::{CompletionTable, ForeignKey, Meta, Options, at_caret, marks, script_marks, signature, suggest};
use std::collections::HashSet;
use std::time::{Duration, Instant};

fn mock() -> Meta {
    Meta::new(
        (0..2000)
            .map(|i| CompletionTable {
                schema: format!("s{}", i % 40),
                name: format!("t{i}"),
                columns: (0..20).map(|c| (format!("c{i}_{c}"), "text".into())).collect(),
                fks: vec![ForeignKey { columns: vec![format!("c{i}_1")], ref_schema: format!("s{}", (i + 1) % 40), ref_table: format!("t{}", (i + 1) % 2000), ref_columns: vec![format!("c{}_0", (i + 1) % 2000)] }],
                ..Default::default()
            })
            .collect(),
    )
}

/// p95 of `f` over 200 runs, printed
fn p95(name: &str, mut f: impl FnMut()) -> Duration {
    let mut v: Vec<Duration> = (0..200)
        .map(|_| {
            let t = Instant::now();
            f();
            t.elapsed()
        })
        .collect();
    v.sort();
    let p = v[v.len() * 95 / 100];
    println!("LATENCY {name:<44} p50 {:>8.1} µs  p95 {:>8.1} µs", v[v.len() / 2].as_secs_f64() * 1e6, p.as_secs_f64() * 1e6);
    p
}

#[test]
#[ignore = "timing: run with --release"]
fn completion_p95_under_2ms() {
    let m = mock();
    let script: String = (0..300).map(|i| format!("select * from s{}.t{i} a join t{} b on b.c{}_0 = a.c{i}_0 where a.c{i}_1 = 1;\n", i % 40, i + 1, i + 1)).collect();
    // Six CTEs of ten columns, each reading the one before it: each block's columns are read once, not once per item above it
    let list = (0..10).map(|c| format!("c5_{c}")).collect::<Vec<_>>().join(", ");
    let chain = format!("with x0 as (select {list} from t5), {} select ", (1..6).map(|k| format!("x{k} as (select {list} from x{})", k - 1)).collect::<Vec<_>>().join(", "));
    // The console's text with the caret at its end
    let cases = [
        ("six chained ctes", format!("{script}{chain}")),
        ("statement start `se`", format!("{script}se")),
        ("select list", format!("{script}select  from t5 a join t6 b on true where a.")),
        ("after join", format!("{script}select * from t5 a join ")),
        ("join on", format!("{script}select * from t5 a join t6 b ")),
        ("where prefix", format!("{script}select * from t5 a join t6 b on true where c")),
        ("nested cte", format!("{script}with x as (select c5_1 k from t5) select * from x join (select c6_2 from t6) y on y.")),
    ];
    let mut worst = Duration::ZERO;
    for (name, text) in &cases {
        let at = text.len();
        for o in [Options::GUI, Options::TUI] {
            worst = worst.max(p95(&format!("{name} ({})", if o == Options::GUI { "GUI" } else { "TUI" }), || {
                let (stmt, caret) = at_caret(text, at, DbKind::Postgres);
                std::hint::black_box(suggest(&m, &stmt, caret, "", false, o));
            }));
        }
    }
    let call = format!("{script}select coalesce(c5_1, ");
    worst = worst.max(p95("signature", || {
        let (stmt, caret) = at_caret(&call, call.len(), DbKind::Postgres);
        std::hint::black_box(signature(&m, &stmt, caret, false));
    }));
    let typo = format!("{script}select a.c5_1, c5_9x, b.nope from t5 a join t6 b on b.c6_0 = a.c5_1 where c5_3 = 1");
    worst = worst.max(p95("marks (the caret's statement)", || {
        let (stmt, _) = at_caret(&typo, typo.len(), DbKind::Postgres);
        std::hint::black_box(marks(&m, &stmt, "", false, &HashSet::new()));
    }));
    let chained = format!("{script}{chain}c5_1, c5_9x from x5");
    worst = worst.max(p95("marks (six chained ctes)", || {
        let (stmt, _) = at_caret(&chained, chained.len(), DbKind::Postgres);
        std::hint::black_box(marks(&m, &stmt, "", false, &HashSet::new()));
    }));
    // The GUI marks the whole console after a pause in typing (debounced), the TUI the lines around the caret: shown, not budgeted
    p95("marks (whole 300-statement script)", || {
        std::hint::black_box(script_marks(&m, &typo, DbKind::Postgres, ""));
    });
    assert!(worst < Duration::from_millis(2), "p95 {worst:?}");
}
