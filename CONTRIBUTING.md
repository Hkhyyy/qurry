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
```

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
- Pull requests go to `main`. CI runs the Rust tests with the real-server tests, vitest, the web and Storybook builds and Playwright; it has to pass before a merge.
