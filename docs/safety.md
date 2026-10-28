# Safety

- **Read-only data sources** are enforced by the engine, not only the UI: in every console, the table editor, Redis and exports, only reading statements run (plus transaction control and `SET`, but not switching read-only off).
- **Transactions**: a transaction you open (Manual mode, or `BEGIN` in Auto mode) is never committed or rolled back behind your back by result paging. Switching Manual → Auto with uncommitted changes asks first, and closing a tab, disconnecting, switching database or quitting with uncommitted changes asks before rolling them back. Data sources with open transactions are not auto-disconnected.
- **Confirmations** before `UPDATE`/`DELETE` without `WHERE`, `DROP`, `TRUNCATE` and Redis `FLUSHALL`/`FLUSHDB`/`DEL` (GUI and TUI).
- **Scripts in Auto mode** run statement by statement: if one fails, the output says how many earlier statements were already committed.
- **Table editor** saves all edits in one transaction, and each `UPDATE`/`DELETE` must change exactly one row, otherwise nothing is saved.
- **Import** of data source files shows what changes first (e.g. "turns Read-only OFF", new startup script).
- **Query history** (GUI) hides passwords in `CREATE/ALTER USER … PASSWORD '…'`, `IDENTIFIED BY '…'` and Redis `AUTH`.
- **Releases** are built only from `main` by GitHub Actions pinned to commit SHAs, with `SHA256SUMS` and build provenance attestations. The update signing key is used only in a separate signing step (never during the build). The GUI checks the update bundle's signature, and `qurry-tui --update` checks the signature of `SHA256SUMS` (same key) and then the checksum before installing.

---

[← User guide](README.md)
