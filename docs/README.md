# Qurry user guide

English | [한국어](ko/README.md)

Everything Qurry does, chapter by chapter. Start with **Getting started** if you are new; the shortcut chapters are references to come back to.

| Chapter | What's in it |
|---|---|
| [Getting started](getting-started.md) | Install, updates, your first data source and query (GUI and TUI), where settings are kept |
| [Updates](updates.md) | Stable and beta update channels, in-app updates, macOS 12, how releases are made |
| [Data sources](data-sources.md) | Supported databases, SSH / SSL and connection options, Production mode, drivers |
| [Workspaces](workspaces.md) | Separate sets of tabs and data sources, each with its own explorer |
| [Database tools](database-tools.md) | Admin reports (sessions, locks, slow queries, sizes, vacuum, privileges), diagrams, compare & migrate, backup & restore, creating and modifying tables / views / schemas, import (CSV / JSON / Excel), test data, copying related rows with masking, charts, history & snippets, explain plans |
| [Vector databases and AI search](vector-and-rag.md) | Qdrant, Chroma, Milvus, pgvector, and searching a table by meaning (RAG) |
| [MCP server](mcp.md) | Let AI tools such as Claude Code explore and query your databases, read-only by default |
| [MCP threat model](mcp-security.md) | What an AI client can reach per MCP setting, how read-only is enforced, prompt injection, and what Qurry does not protect against |
| [Safety](safety.md) | Read-only data sources, transactions, confirmations, the production log and undo SQL, and how releases are signed |
| [Settings](settings.md) | Theme, editor and grid fonts, keymap, where the password key is kept |
| [GUI keyboard shortcuts](shortcuts-gui.md) | Every GUI shortcut (also in the app: `⇧⌘/`) |
| [TUI keyboard shortcuts](shortcuts-tui.md) | Every TUI key (also in the app: `F1`) |
| [TUI in your terminal](tui.md) | Colors (NO_COLOR, 256 and 16-color terminals), fonts, small windows, tmux and ⌥ keys |
| [Development](development.md) | Building from source, code layout, tests |
