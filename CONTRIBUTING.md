# Contributing to Qurry

Thanks for helping. Bug reports and feature ideas go in [issues](https://github.com/Hkhyyy/qurry/issues/new/choose); security problems don't — see [SECURITY.md](SECURITY.md).

How the code is laid out (core / tui / mcp / drivers / src) and where each config file is written: [Development › Layout](docs/development.md#layout).

## Setup

You need Rust (stable), Node.js 22 or newer, pnpm 10 and, for the tests against real servers, Docker.

The GUI runs part of the Rust code as WebAssembly (`crates/shared`), so add the wasm32 target and the `wasm-bindgen` command-line tool at exactly the version in `Cargo.lock` (`crates/shared-wasm/Cargo.toml` pins it):

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.129 --locked
```

If the version changes, `pnpm wasm` stops and prints the `cargo install` line to run. If [binaryen](https://github.com/WebAssembly/binaryen)'s `wasm-opt` is installed, the module is also shrunk with it (optional; CI and release builds install it with `npm install -g binaryen@132.0.0`).

```bash
pnpm install
pnpm wasm            # builds src/wasm/pkg (not committed)
```

Run `pnpm wasm` again after changing `crates/shared` or `crates/shared-wasm`. `pnpm dev`, `pnpm build`, `pnpm test` and the Storybook scripts run it first; anything else that reads the GUI code, such as `npx tsc --noEmit` or `npx playwright test`, needs it to have run once.

Running the GUI, TUI and MCP server from source: [Development › Build & run](docs/development.md#build--run).

## Tests

Write the failing test first: a bug fix starts with a test that reproduces it, a feature with a test for the new behavior. Screens and keys that can't be tested are checked by hand; say how in the pull request.

```bash
cargo test -p qurry-core -p qurry-tui -p qurry-mcp -p qurry-shared -p qurry-shared-wasm   # Rust, no database
pnpm test                                     # GUI logic (vitest)
pnpm build                                    # tsc and the web build
npx playwright install chromium
npx playwright test --project=chromium        # GUI end to end, against the mock backend
npx playwright test --project=chromium --workers=2   # same, two tests at a time (one by default)
npx playwright test --project=webkit --shard=1/2     # one half of the spec files, as a CI shard runs them
```

A shortcut's default keys are checked in two places: `src/lib/shortcuts.test.ts` pins every command's default keys and checks that none collide or can't be pressed, and `e2e/shortcuts.spec.ts` presses every default key in its scope in the running app and checks what the command does (one app start per scope, the checks in a row; a new command needs a check there). Changing a default key means changing it in `crates/shared/src/keymap-table.json` and in the vitest table.

### Against real servers

Tests that need a server are marked `#[ignore]` and read the server from environment variables. Start throwaway containers named `qurry-demo-…`, reachable only from this computer (if a port is taken, change it in `-p` and in the variable below):

```bash
docker run -d --name qurry-demo-pg -e POSTGRES_HOST_AUTH_METHOD=trust -p 127.0.0.1:55432:5432 pgvector/pgvector:pg16
docker run -d --name qurry-demo-mysql -e MYSQL_ROOT_PASSWORD=dbtool -e MYSQL_DATABASE=shop -p 127.0.0.1:13306:3306 mysql:8.4
docker run -d --name qurry-demo-redis -p 127.0.0.1:16379:6379 redis:7-alpine
```

Then run the ignored tests the way CI does (the SSL, TLS, Sentinel, Cluster and SSH tunnel tests need their own servers; see the comments in `crates/core/src/redisdb.rs` and `crates/core/src/db.rs`):

```bash
export QURRY_TEST_PG=127.0.0.1:55432:postgres:postgres     # host:port:user:db
export QURRY_TEST_MY=127.0.0.1:13306:root:dbtool:shop      # host:port:user:password:db
export QURRY_TEST_REDIS=127.0.0.1:16379                    # host:port
cargo test -p qurry-core -- --ignored --skip ssl --skip tls --skip sentinel --skip cluster --skip ssh_tunnel
```

MySQL takes about half a minute to accept connections after it starts. The Redis tests empty databases 0, 3–8, 11 and 12 (`FLUSHDB`), so don't point `QURRY_TEST_REDIS` at a Redis you care about. Remove the containers when you are done:

```bash
docker rm -f qurry-demo-pg qurry-demo-mysql qurry-demo-redis
```

Driver and vector database tests (`QURRY_TEST_CH`, `QURRY_TEST_MSSQL`, `QURRY_TEST_MONGO`, `QURRY_TEST_QDRANT`, …) and older server versions are described in [Development › Tests](docs/development.md#tests); `.github/workflows/ci.yml` has the exact images.

## UI and Storybook

GUI components live in `src/ui` (rules: [src/ui/README.md](src/ui/README.md)), the TUI's in `crates/tui/src/kit`. New UI in either app gets a story.

```bash
pnpm storybook        # http://localhost:6006 — GUI components in each theme, and the TUI screens
pnpm tui-snapshots    # redraw the TUI screens for Storybook after changing the TUI
pnpm build-storybook  # what CI builds
```

## Language

Everything in the repository is in English: code, comments, UI text, commit messages and pull requests. The one exception is the Korean user guide in `docs/ko`, which follows `docs/` chapter for chapter: a change to a page in `docs/` changes its `docs/ko` page in the same pull request.

Shortcuts are written with the ⌘ ⌥ ⌃ ⇧ glyphs (`⇧⌘A`). After changing a shortcut, regenerate the tables with `python3 scripts/gen-keymap-docs.py`.

## Commits and pull requests

- Commit messages start with the kind of change in brackets, then a short summary: `[feat] Copy a query plan as text`, `[fix] Keep the selection after refresh`. Also used: `[test]`, `[docs]`, `[refactor]`, `[perf]`, `[ci]`, `[chore]`.
- One topic per commit. Don't commit screenshots, generated files (`src/wasm/pkg`, `storybook-static`) or local config.
- Pull requests go to `main`. CI runs the Rust tests with the real-server tests, vitest, the web and Storybook builds and Playwright (two shards per browser, one `e2e (<browser>)` check each); it has to pass before a merge. A new push to the pull request cancels the run of the previous one. A pull request that only changes `docs/` or `.md` files (other than `SECURITY.md`) skips the Rust and Playwright jobs; their checks still pass.

## Dependencies

Dependabot opens weekly pull requests for Cargo, npm and the GitHub Actions (pinned by commit SHA), minor and patch updates grouped per ecosystem.

The Audit workflow (`.github/workflows/audit.yml`) fails on a known vulnerability in `Cargo.lock` (`cargo deny check advisories`) or in the production npm dependencies (`pnpm audit --prod`). It runs on pull requests that change a lockfile and every week on `main`, so it can fail with nothing changed here. When it does:

1. Update the affected package if a fixed version is in range (`cargo update -p <crate>`; for npm, update the package that brings it in, or add an override in `package.json`).
2. If it can't be fixed yet, accept it with the reason it can't reach Qurry or why it waits: Rust advisories go in `ignore` in `deny.toml` (`{ id = "RUSTSEC-…", reason = "…" }`), npm ones in `auditConfig.ignoreGhsas` in `pnpm-workspace.yaml` (the GHSA ID with a comment). Remove the entry once the fix is in the lockfile.

Run the checks locally with [cargo-deny](https://github.com/EmbarkStudios/cargo-deny) (`cargo deny check advisories`, the version is in the workflow) and `pnpm audit --prod`.
