//! Benchmark harness (scripts/bench.sh): runs a statement in a console session, as the GUI and the TUI do, and loads the rest
//! of its result page by page (AppState::fetch_more) until the last row, keeping every row as a grid would.
//! Prints one JSON line: rows, the first page's time and the time to the last row; then waits for stdin to close, so the
//! caller can read its peak memory while it still holds the rows.
//!
//! Run:  cargo run --release -p qurry-core --example fetch_bench -- <config dir> <data source id> <sql>
use std::time::Instant;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let [_, dir, id, sql] = args.as_slice() else {
        return Err("usage: fetch_bench <config dir> <data source id> <sql>".into());
    };
    // The GUI's and the TUI's page
    let page = 500;
    qurry_core::QUIET.store(true, std::sync::atomic::Ordering::Relaxed);
    let state = qurry_core::open_state(Some(dir.into()))?;
    let tab = "bench".to_string();
    let t0 = Instant::now();
    let first = state.run_query(id.clone(), sql.clone(), Some(page), Some(tab.clone())).await?;
    let first_ms = t0.elapsed().as_secs_f64() * 1000.0;
    let mut more = first.truncated;
    let mut rows = first.rows;
    while more {
        let r = state.fetch_more(tab.clone(), Some(page)).await?;
        more = r.truncated;
        rows.extend(r.rows);
    }
    let all_ms = t0.elapsed().as_secs_f64() * 1000.0;
    state.close_session(tab).await?;
    println!(r#"{{"rows":{},"first_page_ms":{first_ms:.1},"all_rows_ms":{all_ms:.1}}}"#, rows.len());
    // Still holding the rows: the caller reads this process's peak memory now, then closes stdin
    std::io::stdin().read_line(&mut String::new())?;
    Ok(())
}
