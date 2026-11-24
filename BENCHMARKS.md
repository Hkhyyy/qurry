# Benchmarks

How fast Qurry starts, how much memory it keeps, and how long a 1,000-row and a 1,000,000-row query take, measured the same way
every time with `scripts/bench.sh`, so the numbers can be checked and repeated before a release.

## Results

- Date: 2026-11-24 16:08 KST
- Machine: Mac16,7, Apple M4 Pro, 14 cores, 24 GB; macOS 26.3.1
- Qurry: qurry-tui 0.3.1 built from commit 5467c277 (release profile)
- Database: PostgreSQL 16.15 (Debian 16.15-1.pgdg13+2) in Docker 28.5.1, `shop.orders` 1,000,000 rows (scripts/bench/setup.sql)
- Median of 5 runs after one warm-up run; load average = the 1-minute load when each run started (min – max)

The machine was also building other checkouts (load average 4–6 on 14 cores); a run a few minutes earlier at load 6–10
gave the same numbers within their ranges.

### TUI, MCP server and core (headless)

| Measurement | Median | Min – max | Load average |
|---|---|---|---|
| TUI: launch → first frame | **14.8 ms** | 11.5 ms – 15.9 ms | 3.8 – 6.1 |
| TUI: idle memory, one PostgreSQL source connected | **4.6 MB** | 4.5 MB – 4.7 MB | 3.8 – 6.1 |
| TUI: 1,000-row query end to end (run → all rows in the grid) | **47.8 ms** | 42.3 ms – 57.3 ms | 3.8 – 6.1 |
| TUI: 1M-row query → first 500 rows on screen | **34.9 ms** | 27.1 ms – 41.5 ms | 3.8 – 6.1 |
| TUI: 1M-row query → grid filled to its 100,000-row cap | **1,584 ms** | 1,470 ms – 1,615 ms | 3.8 – 6.1 |
| TUI: peak memory (whole run, 100,000 rows loaded) | **70.7 MB** | 70.5 MB – 70.8 MB | 3.8 – 6.1 |
| MCP: spawn → initialize reply | **5.1 ms** | 5.0 ms – 5.3 ms | 4.1 – 4.6 |
| MCP: idle memory | **3.0 MB** | 2.9 MB – 3.0 MB | 4.1 – 4.6 |
| MCP: `query` 1,000 rows, first call (connects) | **25.4 ms** | 19.4 ms – 38.3 ms | 4.1 – 4.6 |
| MCP: `query` 1,000 rows, warm | **9.7 ms** | 7.7 ms – 10.2 ms | 4.1 – 4.6 |
| MCP: peak memory | **7.7 MB** | 7.5 MB – 8.6 MB | 4.1 – 4.6 |
| core: 1M rows → first page (500 rows) | **19.0 ms** | 13.8 ms – 22.6 ms | 4.0 – 4.7 |
| core: 1M rows → all fetched (500 a page, kept in memory) | **6,932 ms** | 6,290 ms – 7,280 ms | 4.0 – 4.7 |
| core: 1M rows, peak memory | **626 MB** | 626 MB – 626 MB | 4.0 – 4.7 |

How to read it: the TUI times are on screen, end to end (from the key press to the words on the status line), not the server's
time. "1,000-row query end to end" runs it (the first 500 rows) and loads the second page into the grid, as scrolling does. The
grid of the GUI and the TUI holds at most 100,000 rows (beyond that, export); the core row reads all 1,000,000 through the same
session and pages the apps use, and keeps them in memory, so its peak is what 1M rows cost in Qurry's engine.

### GUI and comparison (to be measured)

These open windows, so they are measured by a person at the machine, not by CI or an agent:

```sh
scripts/bench.sh --gui        # Qurry GUI (/Applications/Qurry.app, or QURRY_APP=/path/to/Qurry.app)
scripts/bench.sh --compare    # DataGrip and DBeaver (DATAGRIP_APP, DBEAVER_APP; a missing app is skipped with a note)
```

| Measurement | Qurry GUI | DataGrip | DBeaver |
|---|---|---|---|
| Launch → first window | to be measured | to be measured | to be measured |
| Idle memory, one PostgreSQL source connected | to be measured | to be measured | to be measured |
| 1,000-row query: time the app reports | to be measured | to be measured | to be measured |
| Memory with the 1,000 rows loaded | to be measured | to be measured | to be measured |
| 1M-row query: time the app reports (first page) | to be measured | to be measured | to be measured |
| Peak memory (sum of the processes' peaks) | to be measured | to be measured | to be measured |

## Method

### Data

`scripts/bench.sh` starts `qurry-demo-bench` (`postgres:16`, local connections trusted, no password) on `127.0.0.1:54329`
(`BENCH_PORT` to change it), loads [`scripts/bench/setup.sql`](scripts/bench/setup.sql) and removes the container and its volume
when it ends, also on ctrl-c. The table is `shop.orders`: 1,000,000 made-up orders (Acme customers, `@shop.example` emails,
statuses, amounts, timestamps, a note on every fourth row and a two-item `jsonb`), about 300 MB with its primary key, the same
rows on every run. The queries:

- 1,000 rows: `SELECT * FROM shop.orders ORDER BY id LIMIT 1000`
- 1M rows: `SELECT * FROM shop.orders`

### Runs and numbers

- Every measurement runs once to warm up (page cache, the server's buffers) and then 5 times (`--runs N`); the table shows the
  median and the range.
- Memory is `phys_footprint`, the number Activity Monitor's Memory column shows, read with `footprint(1)`. Idle memory is its
  value after 10 s without input; peak memory is the kernel's high-water mark of it (`phys_footprint_peak`). For an app with
  helper processes it is the sum over them (the app and its children; for Qurry also the WebKit processes that started with it).
- The load average is the 1-minute load when each run started. Other work on the machine slows the runs down: when the range is
  wide or the load is high, run it again on a quiet machine.
- The header lists the date, the machine (`sysctl`), macOS, the commit the binaries were built from (release profile) and the
  PostgreSQL and Docker versions.

### Headless: TUI, MCP server, core

Nothing appears on screen; it runs in a terminal or over SSH.

- **TUI** (`qurry-tui` in a pseudo-terminal, 160 × 40, a new empty `--config-dir` each run, started with the database URL so it
  connects at once). A small screen model reads what the TUI draws, so each step ends when its words are on screen: launch →
  the first complete frame; idle memory 10 s after the explorer shows the database; then, in a new console (`⌃l`, the query
  pasted, `F5`), the 1,000-row query until the status line says `1000 rows` (`⇧Tab` to the grid, then `G` loads the next page),
  and the 1M-row query until its first 500 rows show and until the grid reaches its 100,000-row cap (`G` again and again). Peak
  memory is read at the end of the run.
- **MCP server** (`qurry-mcp` over stdio, as an AI tool runs it, with a scratch `--config-dir` whose only data source is shared
  as Read): spawn → the reply to `initialize`; idle memory 10 s later; the `query` tool with `limit: 1000` on the 1,000-row
  query: the first call (it connects) and the median of the next five; peak memory at the end.
- **core** ([`crates/core/examples/fetch_bench.rs`](crates/core/examples/fetch_bench.rs)): the 1M-row query in a console
  session, then `fetch_more` 500 rows at a time until the last row, keeping every row, as the apps' grids do.

### GUI and comparison

`--gui` (Qurry) and `--compare` (DataGrip, DBeaver) open app windows on the screen, so they run only when asked, print a warning
and wait for Enter first. Quit the apps before. Each app is launched once to warm up, then 5 times; in each run:

1. **Launch → first window** is measured by the script: from starting the app's executable to its first normal window of at
   least 700 × 450 points on screen (a splash screen is smaller), read from the window list (`CGWindowListCopyWindowInfo`, which
   needs no Screen Recording permission; [`scripts/bench/winwait.swift`](scripts/bench/winwait.swift)). It is when the window
   shows, not when its content is drawn.
2. The script asks you to connect to `acme-bench` and press Enter, waits 10 s and reads the idle memory.
3. It asks you to run the 1,000-row query, scroll to the last row and type the time the app reports; it reads the memory.
4. It asks you to run the 1M-row query and type the time the app reports for its first page, then reads the peak memory.
5. It quits the app.

The query times are the apps' own reports, so the three apps are compared on the same definition (the server's work and the
transfer of the first page, not drawing it). They aren't comparable with the TUI's on-screen times above. Where each app shows it:

| App | Data source | Query time |
|---|---|---|
| Qurry GUI | Started with a scratch home folder (`HOME`, `CFFIXED_USER_HOME`) whose config holds only `acme-bench`; your data sources, tabs and settings are neither read nor changed. The update check is off (`QURRY_NO_UPDATE_CHECK=1`) | The status bar and Output: `500+ rows retrieved in N ms` |
| DataGrip | Its own settings (so its license works), opening a scratch project whose `.idea/dataSources.xml` holds only `acme-bench`. The first time, DataGrip may ask to download the PostgreSQL driver | Output (Services): `500 rows retrieved starting from 1 in N ms (execution: …, fetching: …)`: type N. DataGrip has no command that runs a query (its command line opens files and projects, diffs, merges, formats and inspects), so this step is manual |
| DBeaver | `acme-bench` passed with `-con` (not saved) | The line under the results: `N row(s) fetched - N ms` |

Each app keeps its default page: 500 rows in Qurry and DataGrip, 200 in DBeaver, so the 1M-row query's time is its first page.

## Repeat before a release

```sh
scripts/bench.sh              # headless, about 4 minutes after the build; paste the output into Results above
scripts/bench.sh --gui        # at the machine
scripts/bench.sh --compare    # at the machine, when DataGrip or DBeaver have a new version
```

The script needs macOS, Docker, Python 3 and the Rust toolchain (`--gui` / `--compare`: the Xcode command line tools for
`swiftc`). It builds what it runs from the checkout (`CARGO_TARGET_DIR` is honored) and keeps nothing: the container, its volume
and the scratch folders are removed when it ends. For budgets on single screens (completion per keystroke, frame times, bundle
sizes) see [`scripts/perf`](scripts/perf/README.md).
