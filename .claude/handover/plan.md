# Lead plan, items 15-24 (from the original 24-item plan)

Items 1-14 are done and merged. Specs below were written before the run plan was merged into fewer runs; see HANDOVER.md for the current order.

```text
15. slug: core-backend-connect-run
  owner: qurry-security (connect/TLS and read-only enforcement live here)
  branch: feature/core-backend-connect-run
  depends on: core-backend-trait, tls-optin-core
  task: Move connect_via (db.rs:93-235, TLS/plaintext handling), server_version/close/export/run_query/ping (db.rs:239-300), sessions (session.rs: PgSession, the generic Session, cancel, execute_batch*), mysql.rs connect, the redisdb dispatch, and app.rs's 22 matches (plaintext_note, ensure_session, run paths around the read_only_violation calls at :419/:463/:845) behind the Backend trait. Error texts and guard call order are unchanged; this is a move, not new logic.
  files: crates/core/src/backend/{mod,pg,mysql,redis,plugin}.rs (new impl blocks only; no other item edits these functions), crates/core/src/{db,session,mysql,redisdb,app}.rs
  tests first: backend_matrix; the existing read-only tests (pg_read_only_rejects_writes, the MCP execute tests, app.rs tests); a test that the guard runs before the backend on every run path (a fake backend that panics if called for a violating statement).
  verification: `cargo test -p qurry-core -p qurry-tui -p qurry-mcp`, every ignored test including the TLS ones against qurry-demo-* containers, `cargo build -p qurry`, clippy.
  who could notice: nobody.
  
  16. slug: core-backend-jobs
  owner: implementer
  branch: feature/core-backend-jobs
  depends on: core-backend-trait
  task: Move the matches in migrate.rs (9), transfer.rs (5) and audit.rs (2) behind Backend. backup.rs has no Pool match and is not touched.
  files: crates/core/src/{migrate,transfer,audit}.rs, crates/core/src/backend/{pg,mysql}.rs (new impl blocks in separate `impl` sections from item 15; merge conflicts would be adjacent insertions, kept both)
  tests first: backend_matrix export/migrate/audit cases.
  verification: cargo tests plus the ignored tests.
  who could notice: nobody.
  
  --- PHASE 3: splitting the god files (serial chains; nothing else edits App.tsx, tui app.rs or tui input.rs while they run) ---
  
  17. slug: gui-split-shell-stores
  owner: qurry-ui (re-render numbers checked by qurry-perf)
  branch: feature/gui-split-shell-stores
  depends on: all of phase 2 merged (shared-stmt-kind-exec edits App.tsx), gui-e2e-harness
  task:
  - Step a (move only): move the Tab types (App.tsx:59-126), SettingsDialog, PasswordSettings and StatusBar (:2718-2863) to src/features/settings/ and src/features/tabs/types.ts.
  - Step b: add zustand stores. useTabsStore (tabs, activeIds, panes, focused, mru, recent, closedStack; liveSql stays outside React state as it is today) replaces the useState/useRef pairs (:167-200, :309-313). useWorkspacesStore covers :181-188 and the sync/restore code (:407-727) moved to src/features/workspaces/.
  - Components subscribe with selectors. `getState()` replaces tabsRef, activeIdsRef, panesRef, focusedRef, workspacesRef and curWsRef. zustand's `set` is synchronous, matching how the refs work today.
  - Why zustand over reducer+context: context re-renders every consumer on any change, and App.tsx already works around whole-app re-renders with refs (the liveSql comment at :312). Selector subscriptions fix that structurally. src/lib/statusbar.ts already uses the same useSyncExternalStore pattern; zustand is that pattern with selectors and equality checks.
  files: src/App.tsx, src/features/{tabs,workspaces,settings}/**, package.json (zustand)
  tests first: all Playwright specs green before; a store unit test for tab/pane operations (vitest); the workspace-gui.json fixture round-trip (restore, then save, gives an identical file).
  verification: Playwright on both engines, vitest, tsc, `pnpm build`; React Profiler commit counts for typing in the console and a 4-pane run, compared to the item-1 baseline (must not increase).
  who could notice: nobody (same files, same behavior).
  
  18. slug: gui-split-features
  owner: qurry-ui
  branch: feature/gui-split-features
  depends on: gui-split-shell-stores
  task: Move into feature modules, logic unchanged and checked with `git diff --color-moved`:
  - console/ (switchDatabase/Schema, run, resultTitle, executeConsole, fetchMore*, setTxMode, txEnd, cancel, submitEdits; :1184-1471, about 290 lines)
  - explorer/ (openTable/openTableTab/openDdl/generate/dangerous/redis/dba/erd/migrate/rag/backup/restore/ddl/quickDoc/rename/duplicate/deactivate and the tree context menu; :728-1183, about 450 lines)
  - dialogs/ (one useDialogsStore for settings/scheme/keymap/history/prodLog/export/import/gen/objEdit/copy/backup/quickDoc/update state)
  - jobs/ (runJob, startExport/Import; :799-856)
  - conns/ (setColor/setScope/export/import/remove; :2023-2076)
  files: src/App.tsx, src/features/{console,explorer,dialogs,jobs,conns}/**
  tests first: Playwright green before and after; vitest for the dialogs store.
  verification: Playwright on both engines, vitest, tsc, pnpm build, the Storybook build.
  who could notice: nobody.
  
  19. slug: gui-split-groups-shortcuts
  owner: qurry-ui
  branch: feature/gui-split-groups-shortcuts
  depends on: gui-split-features, shared-keymap
  task: Move the editor group rendering and tab drag/drop (:1472-1669, :2077-2416) to editorGroups/; switcher, recent, Search Everywhere, find and FTS (:1682-1754) to search/; and the global key handler (:1842-2022) to shortcuts/useGlobalShortcuts.ts. The handler becomes a lookup over the shared action table (action id to handler map) instead of an if-chain, with the same ids and the same order of precedence. App.tsx keeps only composition and layout, about 400 lines or fewer.
  files: src/App.tsx, src/features/{editorGroups,search,shortcuts}/**
  tests first: a Playwright spec that presses every default GUI shortcut in its scope and asserts the effect (from the action table); the existing specs.
  verification: Playwright on both engines, vitest, tsc, pnpm build, Storybook Screens stories render.
  who could notice: nobody.
  
  20. slug: tui-split-app
  owner: qurry-ui
  branch: feature/tui-split-app
  depends on: shared-completion, shared-grid-format, shared-chart, shared-stmt-kind-exec (all edit app.rs function bodies), tui-characterization
  task: Split app.rs (3310 lines) into app/ as follows, moving code only:
  - mod.rs: types and App::new/spawn
  - explorer.rs: :768-1117
  - tabs.rs: :1118-1443
  - tables.rs: :1444-1598 and submit :2130-2212
  - redis.rs: :1599-1730
  - run.rs: :1731-2129
  - conns.rs: :2213-2401
  - palette.rs: :2402-2598
  - splits.rs: :2891-2979
  - completion.rs: :2980-3236
  run_action stays in mod.rs for now.
  files: crates/tui/src/app.rs, which becomes crates/tui/src/app/** (git mv, then cut)
  tests first: tui-characterization green before.
  verification: `cargo test -p qurry-tui`, the snapshot golden test, clippy, and a tmux smoke run of qurry-tui against qurry-demo-pg.
  who could notice: nobody.
  
  21. slug: tui-split-input
  owner: qurry-ui
  branch: feature/tui-split-input
  depends on: tui-split-app
  task: Split input.rs (2332 lines, of which ~990 are tests) into input/: mouse.rs (:7-427), mod.rs (on_event/on_key/plain, :345-498), tree.rs (:499-596), clipboard.rs (:597-643), editor.rs (:644-693), grid.rs (:694-858), redis.rs (:859-1077), popup.rs (:1078-1311), form.rs (:1312-1341). Tests move next to their code.
  files: crates/tui/src/input.rs, which becomes crates/tui/src/input/**
  tests first: the existing input tests plus tui-characterization.
  verification: as in item 20.
  who could notice: nobody.
  
  22. slug: tui-action-dispatch
  owner: qurry-perf
  branch: feature/tui-action-dispatch
  depends on: tui-split-input, shared-keymap
  task: Add app/actions.rs with `ACTIONS: &[ActionDef { action, run: fn(&mut App) }]`, so titles and ids come from shared::keymap. It replaces the run_action match (:2599-2890) and name_of. The palette action list and help are built from it. Key lookup becomes one HashMap from spec to Action, built when the keymap loads.
  files: crates/tui/src/app/{mod,actions}.rs, crates/tui/src/input/mod.rs, crates/tui/src/keys.rs
  tests first: a Harness test that every Action in the enum has exactly one ActionDef and fires through its default key; the palette list snapshot is unchanged.
  verification: `cargo test -p qurry-tui`, the snapshot golden test, and a key-to-applied-state latency micro-benchmark before and after (Harness, 10k keys).
  who could notice: nobody.
  
  --- PHASE 4: the feature, then the final measurements ---
  
  23. slug: tls-optin-ui
  owner: qurry-ui
  branch: feature/tls-optin-ui
  depends on: tls-optin-core, gui-split-features, tui-split-app
  task:
  - Storybook first: a molecule `TlsOptIn` in src/ui/molecules.tsx with stories for dev (button) and Production (no button; it explains where the setting is).
  - GUI, following the existing DRIVER_MISSING pattern (SchemaTree.tsx:139-171 Status, ConnectionDialog.tsx:401):
    - When a tree load error satisfies tls_opt_in (wasm) and the source isn't Production, show "Allow unencrypted for this data source". The detail line says "The password will be sent unencrypted to <host>. Use only on a network you trust." Clicking calls api.allowPlaintext(id) and then onRetry.
    - The same component appears under a console/table connection error. The retry there only reconnects (api.connect) and says "Connected. Run again." It never re-runs the statement automatically.
    - In ConnectionDialog, a failed Test Connection with this error offers a button that ticks the form's Allow unencrypted checkbox (unsaved form state), with no store write.
  - TUI: a tree load error (app/explorer.rs, the load_children Err branch, next to the plugin::MISSING popup) or a run error opens Popup::Confirm with the same wording; on yes it calls allow_plaintext, re-expands the node or reconnects, and updates the status line.
  - Production: a status message pointing to the data source form, with no confirm.
  - Docs: docs/data-sources.md, docs/safety.md and their docs/ko/ versions describe the button and the Production rule.
  - Why not for Production: Production sources are the ones where interception matters most and where Qurry already makes every risky action deliberate (row-count confirmations, MCP Off by default). Turning off encryption there should be a choice made in the form, with the setting visible, not a reflex click on an error. Dev and LAN sources get the fast path the user asked for. The core refuses Production too, so a UI bug can't bypass this.
  files: src/ui/molecules.tsx, src/ui/stories/Molecules.stories.tsx, src/components/SchemaTree.tsx (Status only), src/features/console/** (error view), src/components/ConnectionDialog.tsx (test result block only), crates/tui/src/app/{explorer,run}.rs (error branches only), src/lib/mock.ts (a "LAN dev" source that fails with NEEDS_TLS until allow_plaintext is called), e2e/tls-optin.spec.ts, crates/tui/src/tests/tls_optin.rs, src/ui/stories/tui-snapshots.json (new scene), the docs above
  tests first (failing):
  - Playwright: the button appears on the dev mock and is absent on the Production mock; a click calls allow_plaintext once, the tree loads, the console doesn't re-run the statement; the dialog Test button ticks the checkbox.
  - TUI Harness with an injected error: the popup appears, yes calls the command (fake state), Production shows only the status text.
  - Ignored real-server: PG at "127.1" against qurry-demo-pg; then GUI-less end to end through AppState: error, allow_plaintext, connect OK with the saved password.
  - A kit snapshot scene for the popup.
  verification: Playwright on both engines, vitest, tsc, `cargo test -p qurry-core -p qurry-tui`, the ignored TLS tests against qurry-demo-pg and qurry-demo-my8, Storybook build, a manual tmux check of the TUI popup.
  who could notice: users who connect to a remote PG/MySQL without TLS. They see a new button or popup in place of a bare error. Nothing changes for anyone else. If someone turned it on by mistake, they turn it off again in the data source form (Options ▸ Allow unencrypted connection).
  
  24. slug: perf-budget
  owner: qurry-perf
  branch: perf/perf-budget
  depends on: everything above
  task: Re-measure against the item-1 baseline and fix regressions inside the touched code:
  - WASM raw and brotli size (budget 1 MB raw / 350 KB brotli; regex compiled lazily with LazyLock)
  - WASM init time in the release build
  - completion suggest p50/p95 on the 2k-table mock in WKWebView and Chromium
  - React commit counts (typing; a 4-pane run; tab switch)
  - JS bundle size
  - TUI key dispatch micro-benchmark and frame time on a large grid
  - GUI and TUI cold start
  files: only what a fix needs, plus a bench under crates/shared/benches (not run in CI)
  tests first: the measurement scripts committed under scripts/perf (no results files).
  verification: a before/after table in the PR.
  who could notice: nobody.
  
  == Order and phases ==
  Phase 0 (parallel): 1 shared-wasm-pipeline (the GO/NO-GO gate), 2 gui-e2e-harness, 3 parity-corpus, 4 tui-characterization, 5 core-backend-characterization.
    Gate A: WASM works in WKWebView (macOS) and WebKitGTK (ubuntu-22.04) under the CSP, or we switch to the IPC fallback. The divergence report goes to the user for D1/D2.
  Phase 1: 6 shared-sql-guard, then 7 tls-optin-core and 8 core-security-single-home (7 and 8 in parallel).
  Phase 2 (parallel): 9 shared-stmt-kind-exec, 10 shared-completion, 11 shared-grid-format, 12 shared-chart, 13 shared-keymap, 14 core-backend-trait, then 15 core-backend-connect-run (after 7 and 14) and 16 core-backend-jobs (after 14).
  Phase 3: GUI chain 17, 18, 19 (19 also needs 13). TUI chain 20, 21, 22 (22 also needs 13). The two chains run in parallel; the core items from phase 2 can still be finishing.
  Phase 4: 23 tls-optin-ui, then 24 perf-budget.
  Integration: each item is merged into feature/architecture with `Merge branch '<branch>' into feature/architecture`. Then the full verification: `cargo test -p qurry-core -p qurry-tui -p qurry-mcp`, `cargo test -p qurry-shared`, clippy on touched code, `pnpm wasm`, `npx tsc --noEmit -p .`, `npx vitest run`, `npx playwright test`, `cargo build -p qurry`, `pnpm build-storybook`, and the ignored real-server tests against qurry-demo-* containers (free ports checked with lsof, never 127.0.0.1:15432/:25432, stopped afterwards). One PR; the user merges.
  
  == Risks and handling ==
  1. WASM in the Tauri WebView. WKWebView needs 'wasm-unsafe-eval' (Safari 16+). Tauri's default minimum macOS (10.13) allows older WebKit, and WebKitGTK versions vary.
     Handling: item 1 tests both real WebViews plus the Playwright WebKit project under the production CSP before anything depends on WASM. The fallback (the same functions over one IPC command, with Monaco's provider kept async) is designed from the start, so a no-go doesn't sink the batch. The supported OS range does not change.
  2. Bundle size and startup.
     Handling: the wasm profile is opt-level "s" with lto and wasm-opt -Oz. Regexes are compiled once, lazily. regex is not swapped for regex-lite, because that could change guard semantics. Budgets are measured in item 24. The module loads from local disk in Tauri.
  3. Monaco completion latency and offsets.
     Handling: CompletionIndex keeps metadata inside WASM; only the current statement text crosses per keystroke. UTF-16-to-byte conversion is done in one place and tested with Korean and emoji. The p95 budget is checked in items 10 and 24.
  4. Merge conflicts on App.tsx, tui app.rs and input.rs.
     Handling: in phase 2 only item 9 touches App.tsx (executeConsole), and the other adoptions keep the TS facade signatures. app.rs edits in phase 2 are limited to named function bodies (tab_label, fuzzy, completion call paths). The splits run as serial chains after phase 2, nothing else edits those files during a chain, and the splits are move-only (reviewed with `git diff --color-moved`). Item 23's UI waits until the splits land, so it is written in the new modules.
  5. Hidden behavior drift.
     Handling: goldens from both current implementations; Playwright on two engines; the TUI snapshot golden test; Harness flows; the workspace-gui.json and keymap.json round-trip fixtures. Divergences are kept behind options (D2) unless the user approves otherwise (D1).
  6. Security regression while moving guard or connect code.
     Handling: qurry-security owns 6, 7, 8, 9 and 15. The guard is moved, not rewritten, with its tests. The WASM result is used for UI only, and backend enforcement is unchanged. A test proves the guard runs before any backend call. The boundary test pins the single homes. The CSP change is explicit (D3).
  7. Async trait design in core.
     Handling: static enum dispatch with Rust 2024 async fn in traits. No dyn and no async-trait boxing, so no per-call allocation. The public core API stays as wrappers, so the TUI, MCP and Tauri don't change.
  8. Build pipeline breakage.
     Handling: the wasm-bindgen-cli version is pinned to equal the crate version. The wasm step is in predev/prebuild/pretest, so `cargo tauri build` (beforeBuildCommand `pnpm build`) works. release.yml changes, so its PR build runs on this PR. The generated pkg is gitignored, so `tsc` needs `pnpm wasm` first; this is documented and done in CI.
  9. CI time.
     Handling: rust-cache plus caching of the wasm-bindgen install. Playwright runs as a separate job.
  10. Review size of one big PR.
      Handling: one PR section per item, move-only commits separated from logic commits, and the divergence table and perf table in the body.
  
  == Behavior changes (for the PR) ==
  - Item 23: the one-click "Allow unencrypted for this data source" button, not offered for Production.
  - Item 9 (if D1 is approved): the GUI Production confirmation and the schema-change refresh follow the core guard and match the TUI.
  - CSP adds 'wasm-unsafe-eval'.
  - Contributors need the wasm32 target and wasm-bindgen-cli.
  
  == Follow-ups explicitly out of scope ==
  - Unifying the GUI/TUI differences kept behind options (completion, splitting, chart numeric detection, extractor literals).
  - Moving GUI unsafe_reason and preview_impact decisions from IPC to WASM.
  - Generating api.ts types from Rust (ts-rs or specta).
  - Splitting src-tauri/commands.rs (97 commands).
  - Splitting the remaining large components (ResultGrid 992, RedisBrowser 878, SchemaTree 655 lines) and core files (backup.rs 1420, redisdb.rs 1343, store.rs 1050).
  - Pane-tree unification (the TUI tracks pct; the GUI doesn't).
  - Connection-form validation shared between ConnectionDialog and the TUI form_config.
  - MCP restructuring.
  - Bringing driver plugins behind the same Backend trait in-process.
  - erdLayout and planText.
  - A minor version bump (0.3.0) for this release: the user's call; otherwise the release workflow does patch +1.
  
  Key files read: src/App.tsx, crates/tui/src/app.rs, crates/tui/src/input.rs, crates/tui/src/complete.rs, crates/tui/src/exec.rs, crates/tui/src/keys.rs, crates/core/src/guard.rs, crates/core/src/sql.rs, crates/core/src/db.rs, crates/core/src/mysql.rs, crates/core/src/app.rs, crates/core/src/store.rs, src/components/SchemaTree.tsx, src/lib/completion.ts, src/lib/shortcuts.ts, src-tauri/tauri.conf.json, src-tauri/src/lib.rs, .github/workflows/ci.yml, .github/workflows/release.yml
```
