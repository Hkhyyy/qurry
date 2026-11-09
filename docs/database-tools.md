# Database tools

English | [한국어](ko/database-tools.md)

Admin reports, table structure, diagrams, schema and data comparison, migrations, backups, importing files, test data, copying related rows, charts, query history and explain plans.

## Database admin (PostgreSQL, MySQL, MariaDB)

GUI: right-click a data source › **Tools** › Sessions & Locks, Slow Queries, … — TUI: `⌃k` › **Database Admin…**

The report runs on one database, shown next to the report select (TUI: in the grid's title). Opened from a database it's that one; opened from the data source it's the source's default database (`postgres` when none is set). Pick another there (TUI: `d`) to rerun the report on it; auto-refresh stays on that database. The list has every database of the server, the current one first, as the console's database picker (Manage Shown Schemas only filters the explorer); for MySQL it lists schemas, with *(all schemas)* when the source has no default one, and a source set to one database shows it without a choice. Sessions, Lock Waits and Users & Roles show the whole server whichever database is picked; if the server turns down the picked database (it doesn't exist, or the account has no CONNECT on it), the report says so — pick another one. The list is read through the report's database, else through the source's default one, so a database the account can't connect to doesn't take the list with it; if both fail (an account that can't connect to the source's default database, opened from the data source), it says so above the report (TUI: the status line): set a default database the account can connect to in Data Source Properties › General › Database (TUI: the data source form's Database field).

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

## Table structure (DDL tab)

**Go to DDL** (`⌘b`, TUI `⌃b`) on a table opens its SQL. The tab's **SQL | Structure** switch (`← →` once it has focus; TUI: `v`) shows the same table as read-only tables instead: columns (name, type, nullable, default, comment, the primary key marked), keys and constraints, indexes with their columns, foreign keys both ways (what the table references and which tables reference it, with ON UPDATE / ON DELETE), triggers, and table options with the comment (PostgreSQL: owner, tablespace, storage parameters, partition key, parent; MySQL: engine, charset, collation, row format). New DDL tabs open in the view you used last (SQL by default); each tab then keeps its own. The toolbar's **Reload** icon (TUI `r`) reads again what the tab shows, e.g. after Modify Table…: the DDL in the SQL view (of any object: views, functions, …), the structure in the Structure view. A DDL tab saved by an older version that doesn't know which object it shows can't reload: the icon is greyed out with "Reopen the DDL to reload" (TUI: `r` says so in the status line).
- PostgreSQL and MySQL fill every section. Other data sources (driver plugins) show columns, keys and indexes, and say "Not available for this data source" for the rest.
- MySQL lists only the referencing tables and triggers your account has privileges on, so on MySQL an empty Referenced by or Triggers says "None visible to this account" rather than "None".
- If loading fails, the view says why: **Retry** (TUI `r`), or **Show SQL** (TUI `v`) — the SQL view still has the full DDL.
- Editing is done in **Modify Table…** (below), not in this view.

## Create and modify objects (PostgreSQL, MySQL)

Like DataGrip: right-click › **New ▸** Table… / View… / Materialized View… / Sequence… / Schema… / Database…, a table › **Modify Table…** (`⌘F6`), and **Rename…** (`⇧F6`). TUI: `⌃k` › New Table… / Modify Table… (`⌃F6`) / New View… / New Schema… / New Database… / New Sequence… / Rename… (`⇧F6`).
- The table designer edits columns (name, type, NOT NULL, default, auto-increment / identity, primary key, comment), indexes and foreign keys (ON DELETE / ON UPDATE). The SQL it will run is shown as you edit: `CREATE TABLE` for a new table, the `ALTER` statements for a change. A renamed column keeps its keys, indexes and foreign keys. The column type field suggests the data source's types in a list right under it: typing filters it (exact match first, then names that start with, then contain what you typed), `↑` `↓` / Page Up / Page Down move, `↩` or `⇥` picks, `⎋` closes it, and any other type can be typed in as is.
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

## Console completion (PostgreSQL, MySQL)

The GUI and the TUI complete from one engine, so both list the same candidates in the same order (the GUI then filters them as you type, fuzzily). What comes depends on where the caret is, and names come only from the statement the caret is in:

- **At a statement start** only statement keywords (`select`, `set`, `insert` …) and the templates come: typing `se` on a new line lists `select` and `set`, not columns of the statement above. A word that can also go on the statement keeps the clause's keywords first (`an` lists `and`, `de` lists `desc`). Columns and tables come where they fit: after `select`, `where`, `on`, `set`, `group by`, `order by`, a `,` …
- **Fixed next words**: after a keyword only a few words can follow, only those come: `left` / `right` / `full` → `join`, `outer join`; `inner` / `cross` / `outer` → `join`; `order` / `group` → `by`, and `partition` → `by` at the start of `over (…)` / `window w as (…)` or after a `)` (a column named `partition` completes as usual); `is` → `null`, `not null`, `not`, `true`, `false`, `distinct from`; `nulls` → `first`, `last`; at a statement start (after a CTE too) `insert` → `into` and `delete` → `from` (MySQL also `ignore`, `low_priority` …; `on delete`, `after insert`, `grant insert` complete as usual); `union` / `intersect` / `except` → `select`, `all`, `distinct`, `values` (PostgreSQL also `table`); `create` / `alter` / `drop` at a statement start → the object kinds (`table`, `view`, `index`, `function`, `role`, `user` …; PostgreSQL also `materialized view`, `type`, `extension`, `policy`, `domain`, MySQL `event`). No columns or functions there (`left j` lists `join`, not `json_agg`).
- **Scope**: the columns are those of the tables, subqueries and CTEs in scope at the caret: a CTE's name after `from` / `join` and its columns after `cte.`, a subquery's alias and the names of its select list (`(select status as st from orders) s` → `s.st`), the outer query's aliases inside a nested `select` (in `where` or the select list), and `using (…)` / `natural` joins. A CTE body or a subquery in `from` sees only its own tables (a `lateral` one also those before it). `from` inside a call (`extract(year from …)`, `substring(… from …)`) and `is distinct from` are part of the expression, so columns come after them.
- **Joins**: after `join` the tables with a foreign key to (or from) the statement's tables come first; after `join users u` the whole condition from the foreign keys, both ways (`on o.user_id = u.id`; a table joined to itself gets both directions); right after `on`, the condition without `on`.
- **Clauses**: `group by` lists the select list's non-aggregated expressions first, `order by` its aliases first, and inside `insert into t (…)` the columns not listed yet. After `insert into t` comes its column list with a `values (…)` to fill in.
- **Templates**: `sel` writes `select * from `, `ins` `insert into ` (then the table's columns as above), `upd` `update … set … = … where …` and `del` `delete from … where …` with their placeholders selected one by one (GUI `⇥`; TUI: the first is selected, typing replaces it). Keywords are written in lower case.
- **`*` asked for**: `⌃Space` right after `*` (or `t.*`) in a select list offers the column list it stands for (qualified when several tables are joined), in the order the database returns them: with `using (…)` / `natural` the join columns once, first. Nothing is offered when a source's columns can't be read (a function in `from`, a table not loaded), since a shorter list would return other columns.
- **Functions**: built-in functions of the dialect and the database's own functions come with their signatures; inside a call's parentheses the GUI's signature help (TUI: the hint line above the caret) shows the function and highlights the parameter at the caret, every overload of a user function included.
- **Unknown names**: a table or column the statement's scope doesn't have is underlined (the same scope as completion, each statement on its own). On it, `⌥↩` lists the closest names and picking one replaces it (TUI: `⌥↩` elsewhere still runs, as before). Nothing is marked when Qurry can't tell: CTEs, aliases, functions, catalogs (`pg_…`), another database or a schema it hasn't loaded, a source whose columns it can't read (a function in `from`), tables the console creates, SQL words that aren't names (`fetch next`, `window w`, `collate "C"`, `grouping sets`, MySQL's `with rollup`, `lock in share mode`, `straight_join` and the `sql_…` modifiers), index hints, a `select … into` target, PostgreSQL's system columns (`ctid`, `xmin` …), DDL, or before the metadata has loaded. The names come from when the data source's metadata loaded: after creating a table or column elsewhere, refresh the data source to stop the mark.

With 2,000 tables loaded a request still answers in under 2 ms (p95; `cargo test --release -p qurry-shared --test complete_latency -- --ignored --nocapture`).

## Value suggestions in the console (PostgreSQL, MySQL)

Completion also suggests what is actually in the table: after `status = '`, `status in ('paid', '` or `name like '` it lists the column's values, and after `data->>'` / `data->'meta'->>'` (MySQL `data->>'$.meta.`) the keys inside the json, most common first with their row counts. Typing the quote opens the list (`⌃Space` without a quote — numbers go in bare, text gets quoted). Only the first 5,000 rows (2,000 for json keys) are read, in a read-only transaction with a 3-second limit, and the answer is reused for 2 minutes. Binary columns and whole json values are not suggested, nor is anything in a JOIN's `ON` condition (the other side is usually a column, so columns complete there; a `WHERE` after it suggests values again).

## Completion in the table filter

A table tab's **WHERE** and **ORDER BY** fields (TUI: `w` or `/` · `o`) complete like the console: the field reads as `SELECT * FROM schema.table WHERE …` (or `ORDER BY …`), so it lists the table's columns, then keywords and functions (`ASC` / `DESC` after a column in ORDER BY) — never other tables or schemas. The list opens while you type a word or after a `.`, not inside a string or a number, and closes once the word is a whole keyword or column, so `↩` after `is null` applies the filter. With the list open: `↑` `↓` `PgUp` `PgDn` move, `↩` / `⇥` insert, `⎋` closes the list. With it closed, `⌃Space` (TUI also `⇥`) asks for the list at the caret, even after a space or a whole word: at an empty spot the table's columns come first, then keywords. `↩` applies the filter and `⎋` puts the field back as it was. Column values are not suggested here.

## Quotes and brackets

Typing `'`, `"`, `(` or `[` adds the closing one after the caret, in the GUI console, a table tab's **WHERE** and **ORDER BY** fields, and the TUI console and its `w` / `o` inputs. It does so before a space, the end of the line or `;:.,=}])>`, and a quote not right after a letter or digit (`it's` stays as typed). Typing the closing character when it is already next to the caret steps over it, `⌫` between an empty pair removes both, and typing an opening one over a selection wraps the selection (it stays selected). Outside the GUI console nothing closes itself inside a string or a comment; the GUI console follows Monaco there and also pairs `{`.

A quote, parenthesis or bracket left open — or a closing one without an opener — is marked in the console as you type: in the GUI with a red underline from it to the end of its line (hover it for what is open, e.g. "Unclosed '('"), in the TUI underlined in red with its line number in red, and while the caret is on that line the editor's bottom border says what is open. Strings, comments and PostgreSQL dollar quotes (`$$ … $$`, `$tag$ … $tag$`) are read the way Qurry splits statements, so a `(` inside them counts for nothing, and brackets pair up within one statement.

## Query history & snippets

Query history and snippets are kept in the config folder and shared by the GUI and the TUI (passwords in `PASSWORD '…'` / `IDENTIFIED BY '…'` are masked). GUI: `⌥⌘e` (History / Snippets tabs, Save as Snippet) · TUI: `⌃k` › Query History…, Snippets…, Save Query as Snippet….

## Explain plan (PostgreSQL)

Each node shows its own share of the time (Explain Analyze) or cost, and flags what is worth a look — a Seq Scan over a big table, row estimates off by 10× or more, filters that throw away most rows, sorts and hashes spilled to disk. TUI: `⌥e` Explain, `⌃k` › Explain Analyze.

Explain Analyze really runs the statement. On PostgreSQL and MySQL Qurry runs it inside a transaction and rolls it back — or back to a savepoint when you already have a transaction open — so its table changes are undone (on MySQL only in transactional tables such as InnoDB, not MyISAM or MEMORY). Qurry asks first for what a rollback can't undo (sequence values, `dblink_exec`, `SELECT … INTO OUTFILE`) and for an `UPDATE` or `DELETE` without `WHERE` ([Safety](safety.md)).

---

[← User guide](README.md)
