//! Performance budget probes: frame time and key dispatch on a large grid and a large console. Timings, not assertions, so
//! ignored by default; run in release mode (scripts/perf/tui.sh):
//!   cargo test --release -p qurry-tui perf_ -- --ignored --nocapture --test-threads=1
//! A frame is ui::draw plus ratatui's diff into a TestBackend; writing escape sequences to the terminal is not included.
use crate::app::TabKind;
use crate::testkit::Harness;
use ratatui::{Terminal, backend::TestBackend};
use std::time::{Duration, Instant};

const W: u16 = 200;
const H: u16 = 50;

fn row(name: &str, mut v: Vec<Duration>) {
    v.sort();
    let us = |d: Duration| d.as_secs_f64() * 1e6;
    let p = |q: f64| us(v[((v.len() as f64 * q).ceil() as usize).max(1) - 1]);
    println!("PERF {name:<44} p50 {:>9.1} µs  p95 {:>9.1} µs  max {:>9.1} µs  n {}", p(0.5), p(0.95), us(v[v.len() - 1]), v.len());
}

/// Each key: dispatch time, then the frame drawn after it
fn keys_and_frames(h: &mut Harness, term: &mut Terminal<TestBackend>, keys: &[&str], name: &str) {
    let (mut key, mut frame) = (vec![], vec![]);
    for k in keys {
        let t = Instant::now();
        h.key(k);
        key.push(t.elapsed());
        let t = Instant::now();
        term.draw(|f| crate::ui::draw(f, &mut h.app)).unwrap();
        frame.push(t.elapsed());
    }
    row(&format!("{name}: key dispatch"), key);
    row(&format!("{name}: frame"), frame);
}

/// 10,000 rows × 30 columns, cells of 8–50 characters
#[tokio::test]
#[ignore]
async fn perf_large_grid() {
    let mut h = Harness::new().await;
    let cols: Vec<String> = (0..30).map(|c| format!("col_{c:02}")).collect();
    let cells: Vec<Vec<String>> = (0..10_000).map(|r| (0..30).map(|c| format!("v{r}_{c} {}", "x".repeat((r * 7 + c * 13) % 44))).collect()).collect();
    let cols: Vec<&str> = cols.iter().map(String::as_str).collect();
    let cells: Vec<Vec<&str>> = cells.iter().map(|r| r.iter().map(String::as_str).collect()).collect();
    let rows: Vec<&[&str]> = cells.iter().map(Vec::as_slice).collect();
    let t = Instant::now();
    let id = h.table_tab("big", &cols, &rows, &["col_00"]);
    row("grid 10k×30: open tab (Grid::new)", vec![t.elapsed()]);

    let mut term = Terminal::new(TestBackend::new(W, H)).unwrap();
    let t = Instant::now();
    term.draw(|f| crate::ui::draw(f, &mut h.app)).unwrap();
    row("grid 10k×30: first frame", vec![t.elapsed()]);
    let idle: Vec<Duration> = (0..200)
        .map(|_| {
            let t = Instant::now();
            term.draw(|f| crate::ui::draw(f, &mut h.app)).unwrap();
            t.elapsed()
        })
        .collect();
    row("grid 10k×30: redraw, nothing changed", idle);

    keys_and_frames(&mut h, &mut term, &["Down"; 1000], "grid 10k×30: Down");
    assert!(h.table(id).grid.as_ref().unwrap().row >= 999, "Down did not move the grid cursor (focus?)");
    keys_and_frames(&mut h, &mut term, &["PgDn"; 150], "grid 10k×30: PgDn");
    keys_and_frames(&mut h, &mut term, &["Right"; 29], "grid 10k×30: Right");
    keys_and_frames(&mut h, &mut term, &["PgUp"; 150], "grid 10k×30: PgUp");
}

/// Typing at the end of a console holding a 100 KB script: work per keystroke that grows with the script shows here
#[tokio::test]
#[ignore]
async fn perf_large_console() {
    let mut h = Harness::new().await;
    let stmt = "select a.col_00, b.col_01 from t_0001 a join s3.t_0601 b on b.col_00 = a.col_00 where a.col_03 > 10;\n";
    let script = stmt.repeat(100 * 1024 / stmt.len());
    let lines = script.lines().count() as u16;
    h.console_with(&script, lines, u16::MAX); // clamped: the end of the last line
    assert!(matches!(h.app.tabs[h.app.active].kind, TabKind::Console(_)));
    let mut term = Terminal::new(TestBackend::new(W, H)).unwrap();
    term.draw(|f| crate::ui::draw(f, &mut h.app)).unwrap();
    let typed: Vec<String> = " select x.col_01 from t_0002 x where x.col_00 = 1".chars().map(|c| if c == ' ' { "Space".into() } else { c.to_string() }).collect();
    let typed: Vec<&str> = typed.iter().map(String::as_str).collect();
    keys_and_frames(&mut h, &mut term, &typed, "console 100 KB: typing at the end");
    let text = h.console_text();
    assert!(text.ends_with("x.col_00 = 1"), "typed text: {:?}", &text[text.len().saturating_sub(80)..]);
}
