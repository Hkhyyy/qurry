# Handover: Qurry 0.3.0 architecture batch

Written 2026-11-02 for a new session on another machine. Start that session as **qurry-lead** and read this file first.

Files next to this one:
- `run5-args.json`: the exact Workflow args for the interrupted run 5. Each item's task starts with a STATE note saying where its branch stands.
- `open-findings.md`: every review finding left open in runs 1–4.
- `plan.md`: the original lead plan for items 15–24, with order, risks and out-of-scope items.

Delete `.claude/handover/` in the last commit before the PR is opened, or keep it out of the PR some other way. The repo is public.

---

## 1. Where things stand

**Goal.** Ship the 0.3.0 restructure as **one PR** from `feature/architecture` into `main`. The user merges it; you never do.
- A shared Rust crate `qurry-shared`, used by the TUI directly and by the GUI through WASM (`qurry-shared-wasm`).
- App.tsx split with zustand.
- The TUI's `app.rs` / `input.rs` split, with an action table.
- A core `Backend` trait.
- Beta/stable releases.
- A few GitHub issues.

**Done (runs 1–4, merged into `feature/architecture`, pushed):**
- shared-wasm-pipeline, audit-hardening, gui-e2e-harness, core-backend-characterization, parity-corpus, tui-characterization, dba-report-fixes, mysqldump-flags
- shared-sql-guard, tools-followups
- tls-optin-core, core-security-single-home, dba-gui-switch
- shared-stmt-kind-exec, shared-completion, shared-grid-format, shared-chart, shared-keymap, core-backend-trait

The last full integration (run 4) passed:
- cargo tests for shared, shared-wasm, core, tui and mcp
- the ignored real-server tests, including backend_matrix
- tsc and vitest
- Playwright: 50/50 on chromium and 50/50 on webkit
- clippy (no new warnings)
- `cargo build -p qurry`
- unchanged TUI snapshots and keymap docs

After run 4, two commits were added on top: one shares `.claude/agents`, and the other adds `.claude/workflows` and this handover.

**Interrupted:** run 5 was stopped soon after it started. Branches with work on them:

| Branch (all pushed) | State |
|---|---|
| `fix/audit-followups` | **Not merged** (held three times for MySQL transaction-tracking bugs). Run 5 fixed every open finding: 6e8607f (ignored tests pass on MySQL 5.7 / MariaDB), 461a7d0 (only a bare BEGIN counts as BEGIN; MariaDB `BEGIN NOT ATOMIC` no longer logged as COMMIT (implicit); one helper decides when to ask the server), 318bc9f (GUI Production Log fallback when WASM is not loaded), 102daa9 (TUI failed-Commit notice no longer replaces an open popup). These fixes have **not been reviewed yet**. |
| `fix/run4-guard-fixes` | One unverified `[wip]` commit, 9dfd17c. |
| `fix/run4-ui-fixes` | Four finished commits plus an unverified `[wip]` commit, 72f8ad7. |
| `feature/core-backend-connect-run` | A tests-first commit e70fbbb plus an unfinished `[wip]` commit, 4cdb2c0. |

Not started: core-backend-jobs, tls-optin-ui, tui-color-modes, release-channels, repo-docs, aws-ssm-tunnel.

## 2. Remaining runs

The user merged the original runs 5+7 and 6+8, so **two runs are left**.

### Run 5 (relaunch)
Use `run5-args.json` after setting `repo` (see §5). It has 10 items:
1. **audit-followups**: verify, review and merge the branch as is.
2. **run4-guard-fixes**: run-4 medium findings on the guard side:
   - `is_read` must check each statement, so a write hidden in DO or PREPARE asks again;
   - Explain Analyse must say when a rollback can't undo a write (nextval, dblink_exec, INTO OUTFILE);
   - IF/ELSE/WHILE/EXEC start a statement only in T-SQL;
   - plus the lows.
3. **run4-ui-fixes**: completion value quoting, alias resolution in the caret's statement, one PG literal rule, keymap order and labels, plus the lows.
4. **core-backend-connect-run**: move connect, run and sessions behind `Backend`. It is a move only: guard order and error texts stay the same, and `MySession::run`'s body stays in mysql.rs.
5. **core-backend-jobs**: move the Pool matches in migrate, transfer and audit behind `Backend`.
6. **tls-optin-ui**: the one-click "Allow unencrypted for this data source" button in GUI and TUI. It is never offered for Production. The full spec is item 23 in `plan.md`; write it in the current files, because the splits come in run 6.
7. **tui-color-modes** (issue #60): NO_COLOR; truecolor, 256 and 16 colors; small terminals. tmux ⌃b alternatives go in the docs only.
8. **release-channels** (issue #58):
   - a merge to main publishes a beta pre-release, and a version tag publishes stable;
   - an update-channel setting, default stable;
   - CHANGELOG.md;
   - D7 (macOS 13).
9. **repo-docs** (issue #70): SECURITY.md, CONTRIBUTING.md, issue and PR templates, and an MCP threat model (en + ko).
10. **aws-ssm-tunnel**: a new feature the user asked for. It adds AWS SSM Session Manager as a third tunnel option next to SSH.
    - Fields: profile, region and target instance id.
    - Qurry spawns `aws ssm start-session --document-name AWS-StartPortForwardingSessionToRemoteHost` on a free local port and kills it on drop.
    - Qurry stores no AWS secrets.
    - It works in the GUI, the TUI and MCP.
    - Errors are clear when the CLI is missing, the plugin is missing, or SSO has expired (`aws sso login --profile <p>`).
    - Tests use a fake `aws` script. **The user verifies on real AWS**, so say so in the report and the PR.
    - SSH-over-SSM is out of scope.

**Every new GUI or TUI UI gets Storybook coverage:** GUI stories in `src/ui/stories`, and TUI scenes in `src/ui/stories/tui-snapshots.json` (produced by `cargo test -p qurry-tui kit_snapshots -- --ignored`). This covers the TLS button, the SSM tab, the update channel and the TUI color modes.

### Run 6 (last; ends with the PR)
Spec: `plan.md` items 17–22 and 24.
- **GUI chain 17 → 18 → 19:** move the types and settings out, add the zustand stores, then the feature modules.
- **TUI chain 20 → 21 → 22:** split app.rs and input.rs, then dispatch through the action table.
- **Splits are move-only.** Check them with `git diff --color-moved`. Nothing else may edit App.tsx, tui app.rs or input.rs while a chain runs.
- **perf-budget:**
  - WASM size and init time
  - completion p50/p95 on the 2k-table mock
  - React commit counts
  - JS bundle size
  - TUI key dispatch and frame time
  - cold start
  - D8 (whether to use regex-lite) is decided here.
- **Integration:** see `plan.md` "Integration"; also run `pnpm build-storybook`. Then **open one PR** (`pr: true`). The PR:
  - Title: `[feat] One shared Rust core for GUI and TUI (WASM), split App.tsx and the TUI app, common driver interface, beta/stable releases`.
  - Body in English, with one section per item, "Behavior changes" (see §4), "Testing" and "Follow-ups" (whatever in `open-findings.md` is still open, plus the known issues in §4).
  - Bump the version to 0.3.0 in the manifests in this PR; the release workflow takes a higher source version as is.
- **Then tell the user:**
  - Run `pnpm storybook` and open http://localhost:6006 to see both GUI and TUI UI.
  - The SSM tunnel needs a check on real AWS.
- Stop and remove the `qurry-demo-*` containers.

## 3. Decisions (made by the user, don't reopen them)

| ID | Decision |
|---|---|
| D1 | The GUI Production confirmation uses the core guard, so it behaves like the TUI. |
| D2 | Other GUI/TUI differences stay behind named options, listed in `crates/shared/tests/DIVERGENCES.md`. Don't silently unify them. |
| D3 | The CSP adds `'wasm-unsafe-eval'` (connect-src 'self' is needed too). |
| D4 | GUI state uses zustand. |
| D5 | No TLS one-click for Production sources. The core refuses too. |
| D6 | Contributors need the wasm32 target and wasm-bindgen-cli **0.2.129** (pinned to Cargo.lock). |
| D7 | `bundle.macOS.minimumSystemVersion = 13.0`, because WASM under the CSP needs Safari 16+. Document it in getting-started (en + ko) and under Behavior changes. |
| D8 | Measure the WASM size (1.07 MB raw / 372 KB gzip, mostly `regex`) in perf-budget, and decide there whether to switch to regex-lite. |

Why WASM and not IPC:
- Monaco completion must be synchronous.
- mock mode, Storybook and Playwright run without Tauri.

Enforcement (read-only, MCP limits) stays in the backend; WASM answers are for UI decisions only.

Two D2 exceptions happened in run 4 and must be recorded in the PR (or reverted by run4-guard-fixes): the flat-comment split option was dropped, and the `statementAt` TS test was deleted.

## 4. For the PR

**Behavior changes so far:**
- **GUI Production confirmation (D1):**
  - A keyword inside a string, a quoted name or a MySQL `#` comment no longer asks.
  - These now ask: settings under which a statement could write, side-effect functions (nextval, setval, dblink_exec, lo_*), and `SELECT … INTO`.
- **Console:**
  - A note line starting with If/Else/While/Exec right above a statement is kept with it. run4-guard-fixes limits this to T-SQL.
  - PostgreSQL nested comments split the way the server reads them.
  - While WASM loads, ⌘↩ shows a loading bar and runs once it arrives.
- **Completion:**
  - MySQL value hints double backslashes.
  - Without WASM, a notice appears once when a console opens.
- **Grid copy:** PostgreSQL strings with a backslash are copied as `E'…'`. run4-ui-fixes uses the same rule for filters and edit SQL.
- **TUI and MCP:** Korean fuzzy ranking is fixed, and Redis key folders stop at 64 levels.
- **TUI keymap:** the command order changed. run4-ui-fixes restores the old order.
- **Driver plugins:** errors read "<op> isn't available for the <driver> driver".
- **Platform and contributors:** CSP `wasm-unsafe-eval` (D3), contributor toolchain (D6), macOS 13+ (D7), the TLS one-click (D5), and from run 5 the beta/stable channels and the SSM tunnel.

**Known issues for Follow-ups:**
- Redis tests sometimes fail in parallel (INFO commandstats).
- The boundary scanner doesn't exempt `<...>` generics.
- On a MySQL server with NO_BACKSLASH_ESCAPES, value hints double backslashes.
- GUI completion p95 exceeds 2 ms on very large scripts.
- Out of scope by plan (see `plan.md`): commands.rs split, ts-rs/specta types, other large files, MCP restructure, plugins behind Backend in-process.

## 5. Setting up the new machine

**Toolchain.** This machine had rustc 1.93, node 26, pnpm 10.25 and Storybook 10.6.
```sh
git clone git@github.com:Hkhyyy/qurry.git && cd qurry
git fetch origin feature/architecture fix/audit-followups fix/run4-guard-fixes fix/run4-ui-fixes feature/core-backend-connect-run
git checkout --detach origin/feature/architecture   # the main checkout stays DETACHED (see rules)
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.129 --locked
pnpm install && pnpm wasm && npx playwright install chromium webkit
gh auth status                                       # gh is used for issues #58 #60 #70 and the final PR
```
For SSM work you need AWS CLI v2. The real-AWS check (done by the user) also needs `brew install --cask session-manager-plugin`.

**Shared test servers.** Agents use these instead of starting their own. Check that the ports are free first.
```sh
docker run -d --name qurry-demo-pg    -p 127.0.0.1:55433:5432 -e POSTGRES_HOST_AUTH_METHOD=trust pgvector/pgvector:pg16
docker run -d --name qurry-demo-my    -p 127.0.0.1:13306:3306 -e MYSQL_ROOT_PASSWORD=dbtool -e MYSQL_DATABASE=shop mysql:8.4
docker run -d --name qurry-demo-redis -p 127.0.0.1:16379:6379 redis:7-alpine
```
The matching test variables:
- `QURRY_TEST_PG=127.0.0.1:55433:postgres:postgres`
- `QURRY_TEST_MY=127.0.0.1:13306:root:dbtool:shop`
- `QURRY_TEST_REDIS=127.0.0.1:16379`

These are already in `run5-args.json` → `sharedDbs`. If you use other ports, change them there.

**Launching a run:**
1. Set `"repo"` in `run5-args.json` to the absolute path of the main checkout (`git rev-parse --show-toplevel`).
2. Optionally set `"targetDir"`; the default is `<repo>/target`.
3. Call `Workflow({ scriptPath: '.claude/workflows/qurry-batch.js', args: <that JSON> })`.

Run IDs from this machine can't be resumed elsewhere, so this is a fresh run. The items' STATE notes tell the agents to continue the pushed branches; the other items start fresh.

## 6. The harness

- **Agents.** `.claude/agents/`:
  - `qurry-lead`: plans and integrates.
  - `qurry-security`: credentials, TLS/SSH/SSM, guards, MCP.
  - `qurry-ui`: Storybook first, both apps.
  - `qurry-perf`: measures before and after.
  - `implementer`: maps to general-purpose.
- **Workflow.** `.claude/workflows/qurry-batch.js` runs Plan → Build → Review → Fix → Integrate:
  - **Plan** is skipped when `items` are passed.
  - **Build** runs each item on its own branch in its own worktree.
  - **Review** uses security, UI and perf lenses. A skeptic verifies medium and high findings only; lows go unverified to the fixer and the follow-up list.
  - **Fix** gets one round by default (`fixRounds`).
  - **Integrate**: the lead merges with merge commits, runs the full verification, and opens the PR only when `pr` is not false.
  - Items with an open **high** finding are held back (not merged).
  - `parallel` (default 3) limits how many items run at once. 3–4 suited a 24 GB laptop.
  - The user asked that batches go through this workflow. One batch in this repo means one PR.
- **After every run, clean up:**
  - If an agent switched the main checkout to a branch, detach it again (`git checkout --detach`). Agents kept doing this, which blocks their own `git worktree add`.
  - Remove `.claude/worktrees/*`, after checking each is clean or pushed.
  - Delete any stray `*target*` build dirs that agents created in scratch dirs, against the rules. They reached about 20 GB per run.
  - Empty `target/debug` if it grows past ~50 GB.
  - Report disk usage to the user.
  - Make sure no vite, storybook or playwright servers are left running.

## 7. Working with this user (rules they set)

**Language and formatting**
- **Reply in Korean, casual speech (반말)**, every message including progress notes. Say what you are doing when you've been quiet for a while.
- Everything in the repo is **English**: code comments, commits, PR text, UI strings. The only exception is the Korean guide `docs/ko`, and `docs/x.md` and `docs/ko/x.md` always change together.
- Write shortcuts with ⌘⌥⌃⇧ glyphs, never "Ctrl+" / "Alt+".

**Commits and PRs**
- One PR per batch. Every merge to main is a release. Never push to main, never merge.
- Commits are `[feat] …`, `[fix] …`, `[chore] …`, `[test] …` with no Co-Authored-By or AI attribution. Stage files by name; screenshots, probes and scratch files never go into commits.

**Way of working**
- Principles: YAGNI, KISS, DRY, TDD.
- Verify for real: real servers in Docker, Playwright against the mock app (`?mock`), the TUI harness, and tmux for the TUI.

**Safety and environment**
- **Ports 127.0.0.1:15432 and 25432 are the user's own DB tunnels: never use them.** Check a port is free with `lsof -iTCP:<port> -sTCP:LISTEN` before using it.
- Docker: touch only containers named `qurry-demo-*`. The user runs other containers for unrelated work; leave them alone.
- Never automate the GUI while the user's own Qurry.app is running. Delete any screenshot that captures other apps. Back up the clipboard before overwriting it.
- Never touch the user's real Qurry config (`~/Library/Application Support/com.hyunkyo.dbtool/`); use a scratch `--config-dir`.
- Never print or commit signing keys or other secrets.
- Ask before destructive commands (docker prune, rm of anything that isn't yours).
