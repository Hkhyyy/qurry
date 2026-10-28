# Database tools

Admin reports, diagrams, schema and data comparison, migrations, backups, query history and explain plans.

## Database admin (PostgreSQL, MySQL)

GUI: right-click a data source › **Tools** › Sessions & Locks, Slow Queries, … — TUI: `⌃k` › **Database Admin…**

| Group | Reports |
|---|---|
| Activity | **Sessions** (who blocks whom, query / transaction age; refreshes every 3 s) · **Lock Waits** — right-click / `c` cancel the query, `K` terminate the session |
| Performance | **Slow Queries** — top 100 by total time (`pg_stat_statements` / `performance_schema`), open or EXPLAIN in a console |
| Storage | **Table Sizes** · **Unused Indexes** · **Duplicate Indexes** · **Foreign Keys Without an Index** — generate DROP / CREATE INDEX |
| Maintenance | **Vacuum & Dead Rows** · **Transaction ID Age** (wraparound) — generate VACUUM ANALYZE (PostgreSQL) |
| Server | **Server Overview** (uptime, connections, cache hit ratio) · **Replication** lag · **Settings** |
| Security | **Users & Roles** · **Table Privileges** — generate GRANT / REVOKE |

Generated SQL opens in a new console; nothing changes until you run it. Stopping sessions asks first and is blocked on read-only data sources.

## Diagram (ERD)

GUI — right-click a schema or database › Diagrams › Show Diagram: tables with keys and foreign keys, auto-laid out (parents above children); drag tables, zoom, double-click to open a table, **Copy as Mermaid**, **Save SVG**. TUI — `⌃k` › Show Diagram: each table's foreign keys in and out as text, plus the Mermaid source (`y` copies).

## Compare & migrate

GUI — right-click a data source › Tools › Compare & Migrate; TUI — `⌃k` › Compare Schemas… / Compare Table Data… / Run Migrations…
- *Schema compare*: the DDL that makes the target schema look like the source (tables, columns, keys, indexes, foreign keys, views); drops come last and are left unchecked. Open it in a console or save it as the next migration file.
- *Data compare*: rows matched by the primary key → INSERT / UPDATE / DELETE to sync the target.
- *Migrations*: a folder of `V1__create.sql`, `V2__add_index.sql` … applied in version order, each in a transaction; applied ones are recorded in `qurry_schema_history`, and a file changed after it was applied is refused.

## Back up / restore

Right-click a database › Tools › Back Up Database… / Restore Database… (TUI: `⌃k`). Runs `pg_dump` / `pg_restore` / `psql` or `mysqldump` / `mysql` from this computer (Homebrew `libpq`, `mysql-client`, Postgres.app and the usual Linux paths are found too; for PostgreSQL the client closest to the server version is used). Passwords go through the environment, SSH tunnels are used, progress shows in the status bar and can be cancelled.

## Query history & snippets

Query history and snippets are kept in the config folder and shared by the GUI and the TUI (passwords in `PASSWORD '…'` / `IDENTIFIED BY '…'` are masked). GUI: `⌥⌘e` (History / Snippets tabs, Save as Snippet) · TUI: `⌃k` › Query History…, Snippets…, Save Query as Snippet….

## Explain plan (PostgreSQL)

Each node shows its own share of the time (Explain Analyze) or cost, and flags what is worth a look — a Seq Scan over a big table, row estimates off by 10× or more, filters that throw away most rows, sorts and hashes spilled to disk. TUI: `⌥e` (or `⌃k` › Explain Analyze).

Explain Analyze really runs the statement. For one that changes data (UPDATE, DELETE, INSERT …) the GUI runs it inside a transaction and rolls it back — or back to a savepoint when you already have a transaction open — so nothing is changed.

---

[← User guide](README.md)
