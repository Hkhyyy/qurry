# Database tools

English | [한국어](ko/database-tools.md)

Admin reports, diagrams, schema and data comparison, migrations, backups, importing files, test data, copying related rows, charts, query history and explain plans.

## Database admin (PostgreSQL, MySQL, MariaDB)

GUI: right-click a data source › **Tools** › Sessions & Locks, Slow Queries, … — TUI: `⌃k` › **Database Admin…**

| Group | Reports |
|---|---|
| Activity | **Sessions** (who blocks whom, query / transaction age; refreshes every 3 s) · **Lock Waits** — right-click / `c` cancel the query, `K` terminate the session |
| Performance | **Slow Queries** — top 100 by total time (`pg_stat_statements` / `performance_schema`), open or EXPLAIN in a console (their text has `$1` / `?` / `(...)` in place of values: PostgreSQL 16+ gets `EXPLAIN (GENERIC_PLAN)`, elsewhere fill in the values first). Without `pg_stat_statements` it says what's missing: on a server that already preloads the library (usual on RDS / Aurora) just `CREATE EXTENSION pg_stat_statements;` (superuser; `rds_superuser` on RDS / Aurora, `cloudsqlsuperuser` on Cloud SQL); otherwise add it to `shared_preload_libraries` first and restart (on RDS / Aurora / Cloud SQL: parameter group / database flags, then reboot), then create the extension if it isn't there yet |
| Storage | **Table Sizes** · **Table Bloat** · **Index Bloat** (space held by deleted rows, estimated from statistics — PostgreSQL) · **Table Fragmentation** (MySQL) · **Unused Indexes** (with the time the scans were counted since) · **Duplicate Indexes** · **Foreign Keys Without an Index** — generate DROP / CREATE INDEX, VACUUM FULL, REINDEX, OPTIMIZE TABLE |
| Maintenance | **Vacuum & Dead Rows** · **Vacuum Progress** (VACUUM, VACUUM FULL / autovacuum running now: phase, % scanned — cancel it) · **Transaction ID Age** (wraparound) — generate VACUUM ANALYZE (PostgreSQL) |
| Server | **Server Overview** (uptime, connections, cache hit ratio) · **Replication** lag · **Settings** |
| Security | **Users & Roles** · **Table Privileges** — generate GRANT / REVOKE |

Generated SQL opens in a new console; nothing changes until you run it. Stopping sessions asks first (showing the pid and query) and is blocked on read-only data sources. On Amazon RDS / Aurora MySQL, where KILL of another user's session is denied, Qurry uses `mysql.rds_kill` / `mysql.rds_kill_query`.

Each report query stops after 30 seconds (`statement_timeout` / `max_execution_time` for that query only, not the connection); a stricter limit set on the server, role or connection stays. A report that hits the limit says so. Through pgbouncer in statement pooling mode, which allows no transaction, PostgreSQL reports run without the limit. Sessions and Lock Waits skip a refresh while the last one is still running.

## Diagram (ERD)

GUI — right-click a schema or database › Diagrams › Show Diagram: tables with keys and foreign keys, auto-laid out (parents above children); drag tables, zoom, double-click to open a table, **Copy as Mermaid**, **Save SVG**. TUI — `⌃k` › Show Diagram: each table's foreign keys in and out as text, plus the Mermaid source (`y` copies it all). Opened from a data source, it first asks which database to draw.

## Compare & migrate

GUI — right-click a data source › Tools › Compare & Migrate; TUI — `⌃k` › Compare Schemas… / Compare Table Data… / Run Migrations…
- *Schema compare*: the DDL that makes the target schema look like the source (tables, columns, keys, indexes, foreign keys, views); drops come last and are left unchecked. Open it in a console or save it as the next migration file.
- *Data compare*: rows matched by the primary key → INSERT / UPDATE / DELETE to sync the target.
- *Migrations*: a folder of `V1__create.sql`, `V2__add_index.sql` … applied in version order, each in a transaction; applied ones are recorded in `qurry_schema_history`, and a file changed after it was applied is refused.

## Back up / restore

Right-click a database › Tools › Back Up Database… / Restore Database… (TUI: `⌃k`). Runs `pg_dump` / `pg_restore` / `psql` or `mysqldump` / `mysql` from this computer (Homebrew `libpq`, `mysql-client`, `mariadb`, Postgres.app and the usual Linux paths are found too, and MariaDB's own `mariadb-dump` / `mariadb` names; the dump client closest to the server version is used — for MySQL / MariaDB one of the same kind, and if only an older or other-kind client is found and it fails, the message says what to install. A MySQL restore uses the newest `mysql` of the server's kind that can switch its shell commands off: `--system-command=OFF`, MariaDB `--sandbox`). SSH tunnels are used, progress shows in the status bar and can be cancelled. Backup files and temporary dumps are readable only by you, and passwords go to the tools in a private file for that one server. Restoring a `.sql` file runs only SQL: psql and mysql client commands in the file (running programs, writing files, connecting elsewhere) are switched off — this needs psql 17.6 / 16.10 / 15.14 / 14.19 / 13.22 or newer, so an older psql doesn't restore `.sql` files (custom-format backups go through `pg_restore` and aren't affected — except when the closest `pg_restore` is 17 or newer and the server is older: it would send settings that server doesn't know, so the backup is replayed through psql instead). Dumps that switch databases with `\connect` (`pg_dump -C`, `pg_dumpall`) stop with a message — restore those with psql yourself.

MySQL backups leave out `SET @@GLOBAL.GTID_PURGED` (`--set-gtid-purged=OFF`, mysqldump 5.6.9 and newer), so a dump of a GTID server (RDS and others) restores into another server, and an 8.0+ `mysqldump` skips column statistics for a 5.7 or MariaDB server (`--column-statistics=0`). **Leave out DEFINER= clauses** in the backup dialog (TUI: y at the question after the file path) writes routines, views, triggers and events without their `DEFINER=` user, so restoring them on a managed service without SUPER works (they then belong to the user who restores). It applies to **Data only** too: a data-only dump still has the routines, triggers and events (not the views). A MySQL restore isn't atomic: statements run one by one, so if one fails, what ran before it stays (PostgreSQL restores run in one transaction).

## Create and modify objects (PostgreSQL, MySQL)

Like DataGrip: right-click › **New ▸** Table… / View… / Materialized View… / Sequence… / Schema… / Database…, a table › **Modify Table…** (`⌘F6`), and **Rename…** (`⇧F6`). TUI: `⌃k` › New Table… / Modify Table… (`⌃F6`) / New View… / New Schema… / New Database… / New Sequence… / Rename… (`⇧F6`).
- The table designer edits columns (name, type, NOT NULL, default, auto-increment / identity, primary key, comment), indexes and foreign keys (ON DELETE / ON UPDATE). The SQL it will run is shown as you edit: `CREATE TABLE` for a new table, the `ALTER` statements for a change. A renamed column keeps its keys, indexes and foreign keys.
- In the TUI each line is written like SQL: column `email varchar(100) not null default 'x' auto pk comment 'login'`, index `users_email_idx (email) unique`, foreign key `orders_user_fk (user_id) -> users (id) on delete cascade`. Keys: `a` column · `i` index · `f` foreign key · `↩` edit · `d` remove · `p` primary key · `n` name · `c` comment · `F5` run · `o` open in a console.
- PostgreSQL runs it in one transaction (if a statement fails, nothing changes); MySQL applies each statement right away. Production data sources ask first, and the change is recorded in the production log. **Open in Console** puts the SQL in a console instead.

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

## Value suggestions in the console (PostgreSQL, MySQL)

Completion also suggests what is actually in the table: after `status = '`, `status in ('paid', '` or `name like '` it lists the column's values, and after `data->>'` / `data->'meta'->>'` (MySQL `data->>'$.meta.`) the keys inside the json, most common first with their row counts. Typing the quote opens the list (`⌃Space` without a quote — numbers go in bare, text gets quoted). Only the first 5,000 rows (2,000 for json keys) are read, in a read-only transaction with a 3-second limit, and the answer is reused for 2 minutes. Binary columns and whole json values are not suggested.

## Query history & snippets

Query history and snippets are kept in the config folder and shared by the GUI and the TUI (passwords in `PASSWORD '…'` / `IDENTIFIED BY '…'` are masked). GUI: `⌥⌘e` (History / Snippets tabs, Save as Snippet) · TUI: `⌃k` › Query History…, Snippets…, Save Query as Snippet….

## Explain plan (PostgreSQL)

Each node shows its own share of the time (Explain Analyze) or cost, and flags what is worth a look — a Seq Scan over a big table, row estimates off by 10× or more, filters that throw away most rows, sorts and hashes spilled to disk. TUI: `⌥e` Explain, `⌃k` › Explain Analyze.

Explain Analyze really runs the statement. On PostgreSQL and MySQL Qurry runs it inside a transaction and rolls it back — or back to a savepoint when you already have a transaction open — so its table changes are undone (on MySQL only in transactional tables such as InnoDB, not MyISAM or MEMORY). Qurry asks first for what a rollback can't undo (sequence values, `dblink_exec`, `SELECT … INTO OUTFILE`) and for an `UPDATE` or `DELETE` without `WHERE` ([Safety](safety.md)).

---

[← User guide](README.md)
