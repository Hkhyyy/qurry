# Qurry

A free DataGrip alternative for PostgreSQL, MySQL/MariaDB and Redis — plus SQL Server, Oracle, SQLite, DuckDB, ClickHouse, MongoDB and vector databases (pgvector, Qdrant, Chroma, Milvus) through drivers added when you need them. query + ferry. Formerly **dbtool**.

![Qurry GUI](docs/screenshots/gui.png)

![qurry-tui](docs/screenshots/tui.png)

- **Free and open source.** DataGrip-style explorer, query console, data editor, explain plan, DDL, full-text search and data export (CSV, TSV, JSON, SQL INSERT, Markdown) and import (CSV, TSV, JSON), with DataGrip key bindings.
- **GUI, terminal and AI from one engine.** The desktop app, the terminal UI (great over SSH) and the MCP server share the same data sources and passwords.
- **Safe for AI.** The MCP server is read-only by default, and the engine enforces that: one reading statement, in a READ ONLY transaction that is rolled back.
- **Small and fast.** Rust + Tauri, no JVM. SSH tunnels, SSL/TLS, Redis Sentinel and Cluster built in.

| Binary | What it is |
|---|---|
| **Qurry** (GUI, Tauri) | Desktop app modeled on DataGrip |
| **qurry-tui** | Terminal UI — works on its own, no GUI needed |
| **qurry-mcp** | MCP server so AI tools such as Claude Code can explore and query your databases |

All three share the same data sources and passwords, so a connection created in one shows up in the others.

## Install

```bash
brew install hkhyyy/qurry/qurry      # TUI + MCP (macOS, Linux)
```

The desktop app (macOS `.dmg`, Linux `.AppImage` / `.deb`) and the TUI archives are on the [Releases](https://github.com/Hkhyyy/qurry/releases) page. Both apps tell you when a new version is out and update themselves. Details, including the first-launch warning on macOS: [Getting started](docs/getting-started.md).

## User guide

Also in Korean: [한국어 사용설명서](docs/ko/README.md)

- [Getting started](docs/getting-started.md) — install, first data source, first query
- [Data sources](docs/data-sources.md) — supported databases, SSH / SSL, Production mode, drivers
- [Workspaces](docs/workspaces.md)
- [Database tools](docs/database-tools.md) — admin reports, diagrams, compare & migrate, backup, history, explain
- [Vector databases and AI search (RAG)](docs/vector-and-rag.md)
- [MCP server](docs/mcp.md) — for Claude Code and other AI tools
- [Safety](docs/safety.md)
- Keyboard shortcuts: [GUI](docs/shortcuts-gui.md) · [TUI](docs/shortcuts-tui.md)
- [Development](docs/development.md) — build from source, tests

## License

[MIT](LICENSE)
