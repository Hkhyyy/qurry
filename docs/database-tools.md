# Database tools

English | [한국어](ko/database-tools.md)

Admin reports, diagrams, schema and data comparison, migrations, backups, importing files, test data, copying related rows, charts, query history and explain plans.

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

Generated SQL opens in a new console; nothing changes until you run it. Stopping sessions asks first (showing the pid and query) and is blocked on read-only data sources.

## Diagram (ERD)

GUI — right-click a schema or database › Diagrams › Show Diagram: tables with keys and foreign keys, auto-laid out (parents above children); drag tables, zoom, double-click to open a table, **Copy as Mermaid**, **Save SVG**. TUI — `⌃k` › Show Diagram: each table's foreign keys in and out as text, plus the Mermaid source (`y` copies it all). Opened from a data source, it first asks which database to draw.

## Compare & migrate

GUI — right-click a data source › Tools › Compare & Migrate; TUI — `⌃k` › Compare Schemas… / Compare Table Data… / Run Migrations…
- *Schema compare*: the DDL that makes the target schema look like the source (tables, columns, keys, indexes, foreign keys, views); drops come last and are left unchecked. Open it in a console or save it as the next migration file.
- *Data compare*: rows matched by the primary key → INSERT / UPDATE / DELETE to sync the target.
- *Migrations*: a folder of `V1__create.sql`, `V2__add_index.sql` … applied in version order, each in a transaction; applied ones are recorded in `qurry_schema_history`, and a file changed after it was applied is refused.

## Back up / restore

Right-click a database › Tools › Back Up Database… / Restore Database… (TUI: `⌃k`). Runs `pg_dump` / `pg_restore` / `psql` or `mysqldump` / `mysql` from this computer (Homebrew `libpq`, `mysql-client`, Postgres.app and the usual Linux paths are found too; for PostgreSQL the client closest to the server version is used). Passwords go through the environment, SSH tunnels are used, progress shows in the status bar and can be cancelled.

## Import data (CSV, TSV, JSON, Excel)

Right-click a table › Import/Export › Import Data from File… (or a schema › Import Data into New Table…); TUI: `⌃k` › Import Data from File…. Reads `.csv`, `.tsv`, `.json` (an array of objects) and Excel `.xlsx` / `.xls` / `.ods` (the first sheet; its first row is the column names). Match file columns to table columns, then everything goes in one transaction — if a row fails, nothing is imported. Excel dates come in as `YYYY-MM-DD [HH:MM:SS]`, whole numbers without `.0`, and empty cells as NULL.

## Generate test data (PostgreSQL, MySQL)

Right-click a table › Import/Export › Generate Test Data…; TUI: select the table, `⌃k` › Generate Test Data…. Fills the table with realistic rows that pass its constraints:
- types, lengths and precision; `NOT NULL`; unique and primary key values that don't clash with rows already there;
- enum values and `CHECK (col IN (…))` lists;
- foreign keys pick rows that already exist in the referenced table (fill the parent first);
- column names hint at the content — `email`, `phone`, `name`, `address`, `city`, `url`, `status`, `price`, `created_at` …;
- auto-numbered, identity and generated columns are left to the database.

It shows how each column will be filled and a sample first, then inserts up to 100,000 rows in one transaction.

## Copy related rows into another data source (PostgreSQL, MySQL)

To debug with real data without copying the whole production database: right-click a table › Import/Export › Copy Related Rows To…; TUI: `⌃k` › Copy Related Rows To….
1. Pick the starting rows with a WHERE condition (e.g. `id = 42`).
2. Rows they point to (parents) always come along, so foreign keys hold. Child rows (rows pointing at them, e.g. a customer's orders, then the order items) come along up to the levels you choose.
3. Personal data is masked as it is read (on by default; untick it in the dialog, or pick "Copy as is" in the TUI): emails, phone numbers, people's names, addresses, ID and card numbers, passwords and tokens, birth dates and IP addresses (by column name). The mask is md5-based, so the same value always masks the same way. Primary and foreign key columns are never masked.
4. Preview shows each table, how many rows and which columns are masked. Copy inserts them parents-first into the target data source (same kind) in one transaction; rows already there are skipped.

The tables must already exist in the target ([Compare & migrate](#compare--migrate) makes the DDL). Up to 50,000 rows per copy.

## Charts

GUI: in a result, View as (the eye icon) › **Chart**: bar, line or pie. Pick X and one or more Y columns; rows with the same X are summed, and **count** counts rows instead. Uses the rows already loaded. TUI: `⌃k` › Chart Result draws horizontal bars — the sum of the first number column by the first text column (or the row count).

## Query history & snippets

Query history and snippets are kept in the config folder and shared by the GUI and the TUI (passwords in `PASSWORD '…'` / `IDENTIFIED BY '…'` are masked). GUI: `⌥⌘e` (History / Snippets tabs, Save as Snippet) · TUI: `⌃k` › Query History…, Snippets…, Save Query as Snippet….

## Explain plan (PostgreSQL)

Each node shows its own share of the time (Explain Analyze) or cost, and flags what is worth a look — a Seq Scan over a big table, row estimates off by 10× or more, filters that throw away most rows, sorts and hashes spilled to disk. TUI: `⌥e` Explain, `⌃k` › Explain Analyze.

Explain Analyze really runs the statement. For one that changes data (UPDATE, DELETE, INSERT …) Qurry runs it on PostgreSQL and MySQL inside a transaction and rolls it back — or back to a savepoint when you already have a transaction open — so nothing is changed.

---

[← User guide](README.md)
