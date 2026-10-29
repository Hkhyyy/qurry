# MCP server (AI tools)

English | [한국어](ko/mcp.md)

```bash
claude mcp add qurry -- qurry-mcp                   # read-only (default)
claude mcp add qurry-rw -- qurry-mcp --allow-write  # adds the execute tool
```

**Which data sources the AI sees** — each data source has an **MCP** setting (GUI: data source › Options › MCP · TUI: the `MCP` field in the data source form):
- **Off**: not listed, and the MCP server never connects to it or reads its password. New data sources start here.
- **Read**: browse and read-only queries.
- **Read & write**: also `execute`, when `qurry-mcp` runs with `--allow-write`. Read-only data sources are never written. On a **Production** data source this lets an AI change production data with no one confirming it (the app's row-count check needs a person) — only turn it on if you mean that, ideally with a database user that can change just what it should.

Data sources made before this setting existed keep working as before (read, and write with `--allow-write`), except **Production** ones, which stay hidden until you turn MCP on for them.

| Tool | Description |
|---|---|
| `list_data_sources` | Data sources shared with MCP (kind, host without any password in a URI, default database, read-only, production, MCP access, SSH) |
| `list_databases` / `list_schemas` / `list_objects` | Browse the tree (Redis: keys and their types) |
| `describe_table` | Columns, indexes, constraints |
| `get_ddl` | DDL (Redis: commands that recreate the key — up to 1,000 elements; use `redis_get` for bigger keys) |
| `search_objects` | Fuzzy search by object name (Go to Database Object), across the databases visible in each data source |
| `query` | Read-only query (100 rows by default, max 1000) |
| `explain` | Execution plan (optionally ANALYZE) |
| `full_text_search` | Find a value in every table |
| `redis_scan` / `redis_get` | Redis key list / key details |
| `export_csv` | Save a read-only query result as CSV in `~/Downloads/qurry-exports` (file name only, never overwrites; 60 s and 512 MB at most) |
| `list_rag_indexes` / `semantic_search` | RAG indexes made in the app / the rows closest in meaning to a question, with a score |
| `execute` | Only with `--allow-write`, for data sources set to **Read & write**. An allow-list: one `INSERT`, or `UPDATE` / `DELETE` with a `WHERE`, per item (PostgreSQL / MySQL run each item as a prepared statement, so the server refuses a second statement); MongoDB `insertOne` / `insertMany` and `updateOne` / `updateMany` / `replaceOne` / `deleteOne` / `deleteMany` with a non-empty filter; vector DBs: adding points and deleting points by id; Redis: reads and writes to single keys without expiry options. PostgreSQL / MySQL run all items in one transaction, other kinds one by one. Anything else (DDL, `MERGE`, `TRUNCATE`, scripts, collection drops …) is refused — ask the user to run it in Qurry |

**How read-only is enforced**
- PostgreSQL / MySQL: exactly one statement, starting with SELECT / WITH / VALUES / TABLE / SHOW / EXPLAIN / DESCRIBE, executed inside a READ ONLY transaction with a 60 s time limit that is rolled back afterwards. Statements are split with the server's own comment rules (MySQL `--x` and `/*! … */`, MariaDB `/*M! … */`, backquoted names, PostgreSQL nested comments), and MySQL runs over the prepared-statement protocol, which accepts a single statement. `COMMIT; DELETE …`, `WITH d AS (DELETE …) SELECT …`, `EXPLAIN ANALYZE DELETE`, `set_config(…read_only…)`, `SELECT … INTO OUTFILE` and functions with side effects outside the transaction (`pg_terminate_backend`, `pg_read_file`, `dblink`, `load_file`, `sleep`, …) are rejected.
- Redis: only read commands, and not `CONFIG` (it can reveal passwords) or `KEYS` (use `redis_scan`). Commands that read a whole key (`HGETALL`, `SMEMBERS`, `LRANGE 0 -1`, …) need a bound of at most 1,000 elements (`HSCAN`, `LRANGE 0 999`, `ZRANGEBYSCORE … LIMIT`, `XRANGE … COUNT`), or use `redis_get`.
- Driver data sources (SQL Server, DuckDB, ClickHouse, MongoDB, vector DBs) have no READ ONLY transaction, so the check is stricter: one statement and no second statement in the same SQL Server batch, no `SELECT … INTO`, no functions that read files or other servers (`OPENROWSET`, ClickHouse `url()` / `file()` / `remote()` …, DuckDB `read_text()` …), only search / query / list requests for vector DBs, and no `$out` / `$merge` for MongoDB. DuckDB is opened with external access switched off, and ClickHouse reads run with `readonly=2` (unless the user is already read-only). This applies to the MCP server; in the app, a read-only DuckDB data source can still read files on your own computer.
- Still, give the MCP server a **least-privilege database user** (read-only role, no superuser/FILE privilege): the checks above are a second line of defence, not a replacement for database permissions.

Options: `--config-dir DIR` (use another config folder), `--verbose` (engine logs to stderr).

---

[← User guide](README.md)
