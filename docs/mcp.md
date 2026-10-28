# MCP server (AI tools)

English | [한국어](ko/mcp.md)

```bash
claude mcp add qurry -- qurry-mcp                   # read-only (default)
claude mcp add qurry-rw -- qurry-mcp --allow-write  # adds the execute tool
```

| Tool | Description |
|---|---|
| `list_data_sources` | Configured connections (kind, host, default database, read-only, SSH) |
| `list_databases` / `list_schemas` / `list_objects` | Browse the tree (Redis: keys and their types) |
| `describe_table` | Columns, indexes, constraints |
| `get_ddl` | DDL (Redis: commands that recreate the key) |
| `search_objects` | Fuzzy search by object name (Go to Database Object), across the databases visible in each data source |
| `query` | Read-only query (100 rows by default, max 1000) |
| `explain` | Execution plan (optionally ANALYZE) |
| `full_text_search` | Find a value in every table |
| `redis_scan` / `redis_get` | Redis key list / key details |
| `export_csv` | Save a read-only query result as CSV in `~/Downloads/qurry-exports` (file name only, never overwrites) |
| `list_rag_indexes` / `semantic_search` | RAG indexes made in the app / the rows closest in meaning to a question, with a score |
| `execute` | Only with `--allow-write`. Runs statements in one transaction; refused for read-only data sources |

**How read-only is enforced**
- PostgreSQL / MySQL: exactly one statement, starting with SELECT / WITH / VALUES / TABLE / SHOW / EXPLAIN / DESCRIBE, executed inside a READ ONLY transaction with a 60 s time limit that is rolled back afterwards. Statements are split with the server's own comment rules (MySQL `--x` and `/*! … */`, MariaDB `/*M! … */`, backquoted names, PostgreSQL nested comments), and MySQL runs over the prepared-statement protocol, which accepts a single statement. `COMMIT; DELETE …`, `WITH d AS (DELETE …) SELECT …`, `EXPLAIN ANALYZE DELETE`, `set_config(…read_only…)`, `SELECT … INTO OUTFILE` and functions with side effects outside the transaction (`pg_terminate_backend`, `pg_read_file`, `dblink`, `load_file`, `sleep`, …) are rejected.
- Redis: only read commands, and not `CONFIG` (it can reveal passwords) or `KEYS` (use `redis_scan`).
- Still, give the MCP server a **least-privilege database user** (read-only role, no superuser/FILE privilege): the checks above are a second line of defence, not a replacement for database permissions.

Options: `--config-dir DIR` (use another config folder), `--verbose` (engine logs to stderr).

---

[← User guide](README.md)
