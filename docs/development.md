# Development

English | [한국어](ko/development.md)

Building from source, how the code is laid out, and the tests.

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
drivers/*     driver programs (SQLite, DuckDB, SQL Server, ClickHouse, MongoDB, Oracle, Qdrant, Chroma, Milvus)
```

## Tests

Write the failing test first: a bug fix starts with a test that reproduces it, a feature with a test for the new behavior (screens and terminal keys that can't be tested are checked by hand). Every pull request runs the CI workflow: Rust unit tests, integration tests against PostgreSQL (with pgvector) / MySQL / Redis / ClickHouse / MongoDB / Qdrant / Chroma / Milvus containers, the same tests against older servers (PostgreSQL 12, MySQL 5.7, ClickHouse 22.8, MongoDB 5, SQL Server 2017), vitest and the web build.

```bash
cargo test -p qurry-core -p qurry-tui -p qurry-mcp   # unit tests (no database)
pnpm test                                               # GUI logic (src/lib/*.test.ts, vitest)
cargo test --workspace -- --include-ignored --skip tunnel   # needs a local PostgreSQL and the containers below
QURRY_TEST_PG=127.0.0.1:15499:postgres:shop cargo test -p qurry-core session -- --ignored   # PostgreSQL elsewhere (host:port:user:db)
```

Containers for the integration tests:

```bash
docker run -d --name dbtool-mysql-test -e MYSQL_ROOT_PASSWORD=dbtool -e MYSQL_DATABASE=shop -p 13306:3306 mysql:8.4
docker run -d --name dbtool-redis-test -p 16379:6379 redis:7-alpine
# TLS (16391), Sentinel (26392 → 16392) and Cluster (17001-17003) servers: see the test comments in crates/core/src/redisdb.rs
# (TLS certificate folder: QURRY_TEST_TLS_DIR, default /tmp/dbtool-tls)
```

Driver and vector database tests run when their server is given as `host:port:user:password:database` (and the driver is built — `cargo build -p qurry-driver-<name>`): `QURRY_TEST_CH`, `QURRY_TEST_MSSQL`, `QURRY_TEST_MONGO`, `QURRY_TEST_ORACLE`, `QURRY_TEST_QDRANT`, `QURRY_TEST_CHROMA`, `QURRY_TEST_MILVUS` (e.g. `QURRY_TEST_QDRANT=127.0.0.1:6333:::qdrant`). The pgvector and RAG tests use `QURRY_TEST_PG` when the server has the `vector` extension (`pgvector/pgvector:pg16`); RAG tests use a fake embedding server, so no model is needed.

TUI tests can drive the app without a database: `crates/tui/src/testkit.rs` opens table tabs with fixed rows, sends keys (`h.key("Ctrl+A")`, `h.type_str("ban")`) and renders the screen to text (`h.screen(120, 30)`). See the tests at the end of `crates/tui/src/input.rs`.

## UI components and Storybook

The GUI is built from the shared components in `src/ui` (buttons, toolbars, dialogs, confirm, form fields, tabs, notices) on design tokens (spacing, type sizes, colors per theme); the rules are in [src/ui/README.md](../src/ui/README.md). The TUI uses the same names and colors from `crates/tui/src/kit.rs`.

```bash
pnpm storybook        # http://localhost:6006 — every GUI component in each theme, and the TUI screens
pnpm tui-snapshots    # redraw the TUI screens for Storybook after changing the TUI
```

## Docs

The user guide is in English (`docs/`) and Korean (`docs/ko/`), chapter for chapter. A change to a feature updates both. After changing a shortcut, regenerate the tables in both languages' [shortcuts-gui.md](shortcuts-gui.md) and [shortcuts-tui.md](shortcuts-tui.md) with `python3 scripts/gen-keymap-docs.py`.

---

[← User guide](README.md)
