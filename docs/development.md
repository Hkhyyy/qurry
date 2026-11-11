# Development

English | [한국어](ko/development.md)

Building from source, how the code is laid out, and the tests.

## Prerequisites

Rust, Node.js, pnpm and the WebAssembly tools: [CONTRIBUTING › Setup](../CONTRIBUTING.md#setup).

## Build & run

```bash
# GUI (dev mode)
pnpm install
pnpm tauri dev

# GUI installer (.dmg on macOS, .AppImage / .deb on Linux)
pnpm tauri build

# TUI / MCP
cargo build --release -p qurry-tui -p qurry-mcp
ln -sf "$PWD/target/release/qurry-tui" ~/.local/bin/qurry-tui
ln -sf "$PWD/target/release/qurry-mcp" ~/.local/bin/qurry-mcp

qurry-tui            # press F1 for the keymap
qurry-tui --help     # print the keymap
```

## Layout

```
crates/core   qurry-core: drivers (PostgreSQL, MySQL, Redis), sessions, SSH tunnels, storage, AppState (every feature)
src-tauri     GUI backend: Tauri commands call AppState methods directly
src           GUI frontend (React + Monaco)
crates/tui    TUI (ratatui)
crates/mcp    MCP server (stdio JSON-RPC)
crates/driver qurry-driver: the driver protocol (one JSON request / response per line over stdio) + REST helpers
crates/shared qurry-shared: pure Rust logic shared with the GUI (no I/O, builds for wasm32)
crates/shared-wasm  qurry-shared-wasm: wasm-bindgen wrapper the GUI calls through src/lib/shared.ts
drivers/*     driver programs (SQLite, DuckDB, SQL Server, ClickHouse, MongoDB, Oracle, Qdrant, Chroma, Milvus)
```

The data sources and saved passwords are shared by the GUI, the TUI and the MCP server (and by older app versions running side by side), so each config file has one home in `crates/core`:

- `store.rs` reads and writes `connections.json` and `connection-access.json`
- `secrets.rs` reads and writes `secrets.json` and the password key (`SecretFile`)
- `lib.rs` `open_state` opens the app state; every app calls it instead of building `Store` and `SecretFile` itself

`crates/core/tests/boundaries.rs` fails when any other code (test code aside) writes those files or constructs a `SecretFile`.

## Tests

How to run the unit tests and the tests against real PostgreSQL, MySQL and Redis containers: [CONTRIBUTING › Tests](../CONTRIBUTING.md#tests). Every pull request runs the CI workflow (`.github/workflows/ci.yml`):

- `rust`: Rust unit tests and integration tests against PostgreSQL (with pgvector) / MySQL / Redis / ClickHouse / MongoDB / Qdrant / Chroma / Milvus containers
- `compat`: the same tests against older servers (PostgreSQL 12, MySQL 5.7, ClickHouse 22.8, MongoDB 5, SQL Server 2017)
- `web`: vitest, the web build and the Storybook build
- `e2e (chromium)` and `e2e (webkit)`: Playwright against the mock app, each browser in two shards that run side by side; each check passes when both of its shards pass

A pull request that only changes files under `docs/` or `.md` files (other than `SECURITY.md`, which a Rust test reads) skips `rust`, `compat` and e2e; their checks still pass. A new push to a pull request cancels the run of the previous push; runs on `main` always run everything and never cancel each other.

The Audit workflow (`.github/workflows/audit.yml`) checks the lockfiles for known vulnerabilities (`cargo deny check advisories`, `pnpm audit --prod`) on pull requests that change a lockfile and every week on `main`. It is not a required check. What to do when it fails: [CONTRIBUTING › Dependencies](../CONTRIBUTING.md#dependencies). Dependabot opens weekly update pull requests for Cargo, npm and GitHub Actions.

Redis TLS (16391), Sentinel (26392 → 16392) and Cluster (17001-17003) servers are described in the test comments in `crates/core/src/redisdb.rs` (TLS certificate folder: `QURRY_TEST_TLS_DIR`, default `/tmp/dbtool-tls`).

Driver and vector database tests run when their server is given as `host:port:user:password:database` (and the driver is built — `cargo build -p qurry-driver-<name>`): `QURRY_TEST_CH`, `QURRY_TEST_MSSQL`, `QURRY_TEST_MONGO`, `QURRY_TEST_ORACLE`, `QURRY_TEST_QDRANT`, `QURRY_TEST_CHROMA`, `QURRY_TEST_MILVUS` (e.g. `QURRY_TEST_QDRANT=127.0.0.1:6333:::qdrant`). The pgvector and RAG tests use `QURRY_TEST_PG` when the server has the `vector` extension (`pgvector/pgvector:pg16`); RAG tests use a fake embedding server, so no model is needed.

The SQLite driver tests (`sqlite_driver_conformance`, and the ignored `form_listing_never_creates_a_file_db`) need no server, only the driver. `drivers/sqlite` is its own Cargo workspace with its own `Cargo.lock` (its SQLite needs a newer `libsqlite3-sys` than sqlx allows in the root lock), so `cargo build -p qurry-driver-sqlite` from the root fails ("did not match any packages"). Build it with `cargo build --manifest-path drivers/sqlite/Cargo.toml --target-dir target` (pass your `$CARGO_TARGET_DIR` instead of `target` if you set one), so the binary lands in `target/debug` next to the tests.

TUI tests can drive the app without a database: `crates/tui/src/testkit.rs` opens table tabs with fixed rows, sends keys (`h.key("Ctrl+A")`, `h.type_str("ban")`) and renders the screen to text (`h.screen(120, 30)`). See the tests in `crates/tui/src/tests/` and next to the key handling in `crates/tui/src/input/`.

## UI components and Storybook

The GUI is built from the shared components in `src/ui` (buttons, toolbars, dialogs, confirm, form fields, tabs, notices) on design tokens (spacing, type sizes, colors per theme); the rules are in [src/ui/README.md](../src/ui/README.md). The TUI uses the same names and colors from `crates/tui/src/kit`. Both are split into atoms, molecules and organisms. Running Storybook: [CONTRIBUTING › UI and Storybook](../CONTRIBUTING.md#ui-and-storybook).

## Docs

The user guide is in English (`docs/`) and Korean (`docs/ko/`), chapter for chapter. A change to a feature updates both. After changing a shortcut, regenerate the tables in both languages' [shortcuts-gui.md](shortcuts-gui.md) and [shortcuts-tui.md](shortcuts-tui.md) with `python3 scripts/gen-keymap-docs.py`. Rebindable commands of both apps (ids, names, default keys) are one table, `crates/shared/src/keymap-table.json`; an id is what keymap.json stores, so it never changes.

---

[← User guide](README.md)
