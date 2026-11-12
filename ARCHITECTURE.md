# Architecture

Where code goes in Qurry, and the rules new code follows so it doesn't add debt. For people and AI agents who change the code.

Not repeated here: what belongs in Qurry and the tests to run ([CONTRIBUTING.md](CONTRIBUTING.md)), the folder list and config file homes ([docs/development.md](docs/development.md#layout)), UI components and words ([src/ui/README.md](src/ui/README.md)), YAGNI / KISS / DRY and GUI–TUI parity (`.claude/agents/*.md`).

Files and functions are named as they are on `main` today; when one moves, this guide moves with it (section 4 lists the moves that are planned).

## 1. Layers

```
drivers/*  ──(stdio, one JSON per line)──▶  qurry-driver (protocol)  ◀──  qurry-core plugin host (plugin.rs, backend/plugin.rs)

qurry-shared (pure Rust) ◀── qurry-shared-wasm ◀── src/lib/shared.ts ◀── GUI (src/)
      ▲
qurry-core (I/O, async, AppState) ◀── src-tauri (Tauri commands) ◀── src/lib/api.ts ◀── GUI (src/)
                                  ◀── crates/tui
                                  ◀── crates/mcp
```

| Layer | Owns | Depends on | Never |
|---|---|---|---|
| `crates/shared` | Rules that are a function of values: SQL lexing and splitting (`sql`), statement checks (`guard`), run plans (`exec`), Explain statements (`explain`), Submit statements for grid edits (`result_edit`), a kind's label, default database and port (`kind`), completion (`complete`, `scope`), cell formats and widths (`format`), highlighting, status bar items and words (`statusbar`), keymap and themes (`keymap-table.json`, `themes.json`), error texts the UI reacts to (`errors`) | serde, regex, sqlformat | I/O, tokio, sqlx, redis (CI step "qurry-shared stays pure") |
| `crates/shared-wasm` | JS values in and out, UTF-8 ↔ UTF-16 offsets | qurry-shared | logic of its own (the module header in `lib.rs` says so) |
| `crates/core` | Connections, pools, sessions, tunnels (SSH, AWS SSM), every database operation (`backend::Backend`, one impl per kind, `dispatch!`), config and secrets files, logs, audit, import/export, backup, admin reports. `app::AppState` is the API every app calls | qurry-shared (re-exports part of it), qurry-driver | UI state, words that only one app shows |
| `crates/driver`, `drivers/*` | The plugin protocol; each extra database as its own executable | serde (+ ureq for REST drivers) | qurry-core |
| `src-tauri` | Tauri commands (each a call into `AppState`), windows, file dialogs, updater, fonts, events | qurry-core | database logic, rules |
| `src/` GUI | `ui/` components and tokens · `lib/` pure helpers plus the two entries to Rust (`api.ts` IPC, `shared.ts` WASM) · `features/<area>/` state hooks, stores, pure `ops.ts` · `components/` screens · `App.tsx` wiring | IPC and WASM only | rules that the TUI also needs |
| `crates/tui` | `kit/` atoms to organisms · `grid`, `tree`, `panes`, `scope` and feature modules (state and checks) · `app/` state and actions · `input/` keys and mouse · `ui.rs` drawing | qurry-core, qurry-shared | its own config files (they go through `store.rs`) |
| `crates/mcp` | Tool list, JSON shapes, masking of tool output | qurry-core | writes the core doesn't allow |

Direction inside the GUI: `ui/` ← `lib/` ← `features/`, `components/` ← `App.tsx`. Inside the TUI: `kit/` ← state modules ← `app/` ← `input/`, `ui.rs`. `src/ui` and `crates/tui/src/kit` import nothing from the app today; keep it that way.

### Where the code breaks the direction

| Break | Where |
|---|---|
| The TUI reaches the same shared module two ways | `qurry_core::format` (`crates/tui/src/grid.rs`) and `qurry_shared::format`; `qurry_core::statusbar` (`crates/tui/src/ui.rs`) and `qurry_shared::statusbar`; `crates/core/src/lib.rs` re-exports only some shared modules (`exec`, `keymap`, `complete`, `explain`, `format`, `statusbar`, …) |
| `lib/` imports from `components/` | `src/lib/planText.ts` (`PlanNode`) and `src/lib/resultTabs.ts` (`ResultEntry`), both from `components/ConsolePane` |
| IPC outside `api.ts` | `src/lib/shortcuts.ts` (`invoke("save_keymap")`) |
| One check, two routes into the GUI | `api.unsafeReason` over IPC (`src/lib/api.ts` → `unsafe_reason` in `src-tauri/src/commands.rs`) while WASM exports the same `unsafeStatement` (`crates/shared-wasm/src/lib.rs`), unused |
| One rule written twice, TS and Rust, held together by a comment | `src/lib/panes.ts` and `crates/tui/src/panes.rs`; `src/lib/scopeToggle.ts` / `ScopePopup.tsx` and `crates/tui/src/scope.rs` |

## 2. Patterns

The few patterns the code is built on, by name, so a reader knows what to expect and a change knows what to follow.

- **Package by feature.** The GUI groups state and actions by area: `src/features/<area>/` (`console`, `explorer`, `editorGroups`, `tabs`, `workspaces`, `search`, …), each with its hook, store and pure `ops.ts`. The TUI's feature modules are files, not layers: `app/run.rs`, `app/tables.rs`, `app/redis.rs`, `app/explorer.rs`, … A new feature is a new folder or file next to these, not a new layer.
- **Ports and adapters.** `qurry-core` is the center and `AppState` is its port: the GUI (through Tauri commands and `api.ts`), the TUI and the MCP server are adapters that call it and draw or serialize the answer. Databases are reached through `Backend` (below) and driver plugins, so core never knows which app is calling or which server answers.
- **Strategy.** One `Backend` trait (`crates/core/src/backend/mod.rs`) and one impl per kind (`pg.rs`, `mysql.rs`, `redis.rs`, `plugin.rs` for every driver). A caller picks the impl with `dispatch!` and never matches on the kind itself; a kind that lacks an operation gets the trait's default, an "isn't available" error.
- **State, input and drawing kept apart in the TUI.** For one feature there are three places with the same name: its state and actions in `app/<feature>.rs` (`app/redis.rs`, `app/explorer.rs`, `app/tables.rs`), its keys and mouse in `input/<feature>.rs` (`input/redis.rs`, `input/tree.rs`, `input/grid.rs`), its drawing in `ui.rs` (`draw_tree`, `draw_pane`, `draw_popup`, …). `app/` never reads a key, `input/` never draws, and `ui.rs` only draws: it takes `&mut App` for scroll positions and measured widths, never to run an action.

What we don't use, on purpose: Clean-Architecture layer stacks (use cases, repositories, presenters: a Tauri command is one line into `AppState`), dependency-injection containers (`AppState` is built once in `open_state` and passed), and interfaces made in advance (a trait appears when there is a second impl, as `Backend` did; `Plugin` drivers speak a protocol, not a trait).

## 3. Rules for new code

Each rule: the rule, why, an example in this repo. A pull request that breaks one says which and why.

### Where logic goes

1. **A pure rule both apps need goes in `qurry-shared`.** If it reads only values (text, `DbKind`, rows already fetched, settings) and the GUI and the TUI (or the TUI and MCP) both use it, write it once in `crates/shared`, export it in `crates/shared-wasm`, and wrap it in `src/lib/<x>.ts` (the `sharedOr` pattern in `src/lib/shared.ts`, with the fallback for a WebView that refused WebAssembly). Why: two copies drift (`crates/shared/tests/DIVERGENCES.md` lists the ones still open). Examples: `labels.rs` → `tabLabels.ts`, `exec.rs` → `sqlSplit.ts`, `statusbar.rs` → `lib/statusbar.ts`, `explain.rs` → `explain.ts`, `result_edit.rs` (`edit_statements`, `join_statements`) → `gridEdit.ts`, `kind.rs` (`default_db`, `default_port`, `DbKind::label`) → `defaultDbOf` in `api.ts`.
2. **Anything with I/O goes in `qurry-core`, behind `AppState`.** Connections, files, the config folder, timers, processes. Apps call `AppState` methods or `qurry_core::app` functions, never a driver or a file directly. Why: three apps and older versions share the same folder and servers. Example: `open_state` (`crates/core/src/lib.rs`), `crates/core/tests/boundaries.rs`.
3. **A database operation is a `Backend` method.** Add it to the trait in `backend/mod.rs` (in its section), implement it per kind, call it through `dispatch!`. A kind that lacks it gets the default "isn't available" error. A new database is a driver plugin, not a new core dependency (CONTRIBUTING question 4).
4. **Apps own only the medium.** Drawing, focus, keys, layout, menus, file dialogs, Tauri windows and the updater, TUI terminal handling, MCP tool schemas and output shaping. A Tauri command is a one-line call into `AppState`; if it grows a branch, the branch belongs in core.
5. **Words both apps show for the same thing are shared.** Status items, error hints the UI acts on, confirm reasons: `qurry_shared::statusbar`, `qurry_shared::errors` (`NEEDS_TLS`, `DROPPED_TLS`). The GUI matches an error by these constants, never by a literal it typed itself.
6. **One-app features are named exceptions** (CONTRIBUTING question 2). The code still keeps the rule in core or shared, so the other app can follow.
7. **GUI entries to Rust are `src/lib/api.ts` (IPC) and `src/lib/shared.ts` (WASM) only.** A new command is registered in `src-tauri/src/lib.rs`, typed in `api.ts` and answered in `src/lib/mock.ts` (the mock throws `not implemented`, so e2e catches a missing one).

### Size and splitting

8. **Limits:** a file at most ~800 lines of non-test code; a Rust function at most ~80 lines; a React component or hook at most ~250 lines. Section 4 lists what is over them today.
9. **Don't grow what is over the limit.** Touching a function or file above it: put the new code in a new function or module and call it. If the change needs the old code reshaped, split first.
10. **Split by feature, in a move-only commit.** `[refactor] Move … (move only)` before the behavior change, tests moved next to their code, nothing else changed, so the review diff is the move. Examples: TUI `app/mod.rs` into `app/{explorer,tabs,tables,redis,run,conns,…}.rs` (`00bf61e`), TUI `input/mod.rs` (`ef28d77`), `App.tsx` into `src/features/*` (`349674c`, `e2ff3b1`, `b4d0d2e`, …).
11. **A GUI feature folder** is `features/<area>/`: `use<Area>.ts` (state and actions), `store.ts` when state is shared, pure `ops.ts` / `<rule>.ts` with a vitest next to it. Screens stay in `components/`.

### GUI and TUI the same, without two copies

12. **Shared tables, not shared comments.** Keys: `keymap-table.json` (ids never change). Themes: `themes.json`. Status bar: `statusbar.rs`. Field coloring: `highlight.rs`. A comment saying "same rules as X" is not parity.
13. **When a rule can't move to Rust** (it needs DOM or ratatui state), pin it with the parity corpus: add cases to `crates/shared/tests/corpus/*.json`, dump both sides (`crates/tui/src/parity_dump.rs`, `src/lib/parity.dump.test.ts`, `pnpm parity:dump`) and record any accepted difference in `DIVERGENCES.md`.
14. **A user-visible change covers both apps in the same pull request**, plus Storybook (`pnpm tui-snapshots` for the TUI).

### Errors, async, timeouts

15. **Core returns `AppResult<T>`.** `AppError::Msg` text is shown to people: English, says what happened and what to do. No `unwrap`/`expect` on data, files or the network outside tests.
16. **Log with `qurry_core::log!`, nothing else.** Every line goes through `logfile::redact`. No `println!`/`eprintln!` outside tests and CLI usage text (stdout is the MCP protocol, the screen is the TUI's).
17. **Locks:** `std::sync::Mutex` for short in-memory maps, never held across `.await`; `tokio::sync::Mutex<Session>` per console tab; background work takes sessions with `try_lock` and skips busy ones (`app::housekeeping`).
18. **Periodic work goes in `app::housekeeping`**, which each app ticks (`src-tauri/src/lib.rs`, `crates/tui/src/main.rs`, `crates/mcp/src/main.rs`). Don't start a timer per app.
19. **Every network wait has a timeout, named as a const next to its use** (`TUNNEL_GRACE`, `SESSION_PING` in `app.rs`, `CURSOR_IDLE` in `session.rs`, `REPORT_TIMEOUT_S` in `dba.rs`). A bare number in `Duration::from_secs(…)` in non-test code is a review finding.
20. **Cancel by dropping the future**, and stop the server side when it keeps running: GUI `cancellable` (`src-tauri/src/commands.rs`), TUI `App::spawn_abortable` (`crates/tui/src/app/mod.rs`), `AppState::cancel_query`. Long jobs report through `transfer::Job` (progress and cancel). Blocking calls go in `spawn_blocking` (`plugin.rs` install, `rag.rs` embeddings).

### Safety boundaries

New paths go through these checks; they don't add their own.

| Boundary | The one check | Rule for new code |
|---|---|---|
| Read-only data source | `AppState::check_read_only` (`crates/core/src/app.rs`; Redis commands through `check_read_only_redis`) | Every core entry that sends user text calls it first (as `run_query`, `fetch_more`, batches, edits, `redis_exec_line`, `export_csv`, `subset.rs` and `transfer.rs` do) |
| What a statement is | `qurry_shared::guard` (`read_only_violation`, `unsafe_statement`, `mcp_execute_violation`, `is_read`) | The WASM copy is for UI decisions only; enforcement runs in core (the header of `crates/shared-wasm/src/lib.rs`) |
| MCP exposure | `ConnectionConfig::mcp_access` and `mcp_dba_report_allowed` (`store.rs`), `AppState::run_read_only` (`app.rs`) | A new tool reads only, is listed with `read_only: true` (test `new_tools_are_listed_read_only`), keeps row data and query text off for Production by default, and is in `docs/mcp-security.md` |
| Production | The person confirms in each app; core writes the Production log and audit (`audit.rs`) | Ask before any write on a source with `options.confirm_changes`, deciding with `guard::is_read`. Each call site asks in its own words (TUI `app/run.rs`, `app/tables.rs`, `app/redis.rs`, `datatools.rs`; GUI `features/console/useConsole.ts`, `components/DataTools.tsx`, `features/tabs/ops.ts`): a shared "when to ask" function was considered and declined, since the sites share only those two inputs and the wording differs per app on purpose |
| Config and secrets | `store.rs`, `secrets.rs`, `open_state` (`lib.rs`) | No other writer; `tests/boundaries.rs` fails |
| Secrets in text | `logfile::redact` (logs), `dsn::redact` (URLs in titles and messages), `history::redact` (query history), `store::without_userinfo` (MCP output) | Anything that leaves the process (log, error, MCP answer) goes through one of these |
| Files written | `export_path` (`lib.rs`, MCP), `save_dialog` (`commands.rs`, GUI) | Write only to a path the user picked or the export folder |

### Tests per kind of change

| Change | Needs |
|---|---|
| Rule in `qurry-shared` | Unit test in the module; a vitest through WASM in `src/lib/<x>.test.ts` when the GUI uses it (`.nowasm.test.ts` for the fallback); a corpus case when it replaced a TS rule |
| Core, no server | `#[cfg(test)]` test in the module |
| Core, talks to a server | `#[ignore]` test on `QURRY_TEST_*`, for PostgreSQL and MySQL (and Redis when it applies); `tests/backend_matrix.rs` for a new `Backend` operation |
| Driver | Its conformance test on `QURRY_TEST_<X>` |
| TUI behavior | `testkit::Harness` test (`crates/tui/src/tests/` or next to the key code); screen change: `pnpm tui-snapshots` |
| GUI behavior | vitest for `lib/` and `features/*/ops.ts`; Playwright against the mock (`e2e/`), with the `mock.ts` handler; shortcut: `shortcuts.test.ts` and `e2e/shortcuts.spec.ts` |
| Safety boundary | A test that the refused case is refused in core or MCP (like `mcp_hidden_source_secrets_stay_locked` in `app.rs`'s tests, `new_tools_are_listed_read_only` in `crates/mcp`) |
| Any UI | A Storybook story |

Warnings: CI runs `cargo clippy --all-targets -- -D warnings` on the workspace (the GUI crate `qurry` is checked on macOS: `cargo clippy -p qurry --all-targets`) and on `drivers/sqlite`, so a warning fails the build; `pnpm build` (tsc `strict`, `noUnusedLocals`) must be clean. A new `#[allow(…)]` has a one-line reason and is never crate-wide; a `@ts-expect-error` has a comment saying why. There is no TS lint in CI, so unused exports and the like are on the author and the reviewer.

### What a pull request says

The template (`.github/pull_request_template.md`), the five questions (CONTRIBUTING › What belongs in Qurry) and the sections the lead writes (Behavior changes, Testing; `.claude/agents/qurry-lead.md`) apply. In addition: any rule above that the change breaks, and why (for example "`ResultGrid.tsx` grew by 30 lines: the split is #…").

## 4. Gaps (cleanup batch candidates)

Where today's code makes these rules hard to follow, largest first. Sizes are rounded and as of 0.3.4.

| # | Gap | Where | Suggested fix |
|---|---|---|---|
| 1 | `AppState` god file: about 2,600 lines of code plus 1,600 of tests; pool, tunnel and session lifecycle, source CRUD, queries and transactions, Redis and import/export pass-throughs | `crates/core/src/app.rs` | Move-only split into `app/{pools,sessions,sources,run,tx}.rs`, `AppState` in `app/mod.rs`, tests with their code |
| 2 | One component of about 1,200 lines; other large screens | `src/components/ResultGrid.tsx`, `ConnectionDialog.tsx` (about 800), `ConsolePane.tsx` (about 650), `RedisBrowser.tsx` (about 1,100) | Hooks into `features/grid/`, pure parts into `lib/` (as `gridSelection.ts`) |
| 3 | All TUI drawing in one file | `crates/tui/src/ui.rs` (about 2,100 lines; `draw_pane` and `draw_popup` are each over 200) | Split per feature as `app/` and `input/` were |
| 4 | MCP server in one file; `Server::call` is one long match over the tools | `crates/mcp/src/main.rs` | `tools.rs`, one function per tool, `mask.rs` |
| 5 | Long TUI handlers | `input/popup.rs` `popup_key`, `app/run.rs` `run_stmts_checked`, `input/redis.rs` `redis_key`, `app/explorer.rs` | Split by popup / step |
| 6 | Same rule in TS and Rust with no shared code or corpus | `src/lib/panes.ts` / `crates/tui/src/panes.rs`; `src/lib/scopeToggle.ts` / `crates/tui/src/scope.rs` | Move to `qurry-shared` or add corpus cases |
| 7 | Two import paths for shared modules; partial re-exports | `crates/core/src/lib.rs` (`pub use qurry_shared::…`), TUI `qurry_core::format` vs `qurry_shared::format` | Apps import pure modules from `qurry_shared`; drop re-exports as callers move |
| 8 | Unsafe-statement check over IPC while the WASM export is unused | `src/lib/api.ts` `unsafeReason`, `src-tauri/src/commands.rs` `unsafe_reason`, `crates/shared-wasm/src/lib.rs` `unsafeStatement` | Use WASM, remove the command |
| 9 | Redis read check written twice | `AppState::run_read_only` has its own `is_read_command` check next to `check_read_only_redis` (`crates/core/src/app.rs`) | `run_read_only` calls `check_read_only_redis` |
| 10 | Other direction breaks | `src/lib/planText.ts`, `src/lib/resultTabs.ts` (types from `components/ConsolePane`), `src/lib/shortcuts.ts` (`invoke`) | Move `PlanNode` and `ResultEntry` to `api.ts` or `lib/`; `api.saveKeymap` |
| 11 | Large core modules over the limit | `audit.rs`, `backup.rs`, `ssm.rs`, `store.rs` (1,700–2,100 each), `shared/complete.rs` (about 2,300), `shared/guard.rs` (about 1,850) | Split when next changed (rule 9) |
| 12 | Core drivers sit flat next to features; stale header | `db.rs` (PostgreSQL, "Only PostgreSQL is supported for now"), `mysql.rs`, `redisdb.rs` | Move under `backend/` |
