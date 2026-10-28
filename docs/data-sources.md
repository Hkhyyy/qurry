# Data sources

The databases Qurry works with, how to connect to them (SSH, SSL, options) and how drivers for the less common ones are added.

## Supported databases

| Database | Features |
|---|---|
| PostgreSQL | Explorer tree (databases, schemas, tables, views, sequences, routines), query console (server-side cursor paging, cancel, manual transactions), data editing, explain plan, DDL, full-text search |
| MySQL / MariaDB | Same as above. A MySQL database is its own schema, so tables appear directly under the database |
| Redis | Medis-style browser (key list with type and TTL, per-type value editor), redis-cli style terminal, **Standalone / Sentinel / Cluster** |

Connection options:
- **SSH tunnel** with a key or password. Host keys are checked against `~/.ssh/known_hosts`: a changed key is refused, and an unknown server stops with its SHA256 fingerprint until you choose **Trust host key** (GUI: data source dialog; TUI: `⌃y` in the form or `⌃k › Trust SSH Host Key`).
- **SSL/TLS**: Require / Verify CA / Full Verification, with optional CA and client certificate. Works for PostgreSQL, MySQL and Redis.
- **Options tab**: read-only, transaction control and isolation level, time zone, keep-alive, auto-disconnect when idle, single database mode, startup script.
- **Production**: before a statement that changes data runs, Qurry counts the rows it will change (the same table and WHERE as a `SELECT count(*)`, nothing is changed) and asks. Statements it can't count (`UPDATE … FROM`, `DELETE … USING`, CTEs) still ask. GUI: Options tab · TUI: the `Production` toggle in the data source form.

> Redis Sentinel and Cluster nodes advertise their own addresses, so they can't be combined with an SSH tunnel.

## Drivers (added when you need them)

Nothing extra is installed up front. Pick one of these types for a data source and Qurry offers to **Install** its driver: a small program downloaded from the GitHub release of your Qurry version, checked against the signed `SHA256SUMS` (same key as app updates) and kept in the config folder under `drivers/`.

| Database | Notes |
|---|---|
| SQL Server | SQL Server 2017+ (older versions work, with a warning); `dbo` and other schemas |
| Oracle | Needs the free [Oracle Instant Client](https://www.oracle.com/database/technologies/instant-client.html) — set its folder in the data source (or `ORACLE_CLIENT_LIB_DIR`). Oracle 19c+ (11g works, with a warning); the service name goes in Database |
| SQLite · DuckDB | A file instead of host and port (DuckDB: empty = in memory) |
| ClickHouse | Over HTTP (port 8123) |
| MongoDB | Shell syntax in the console: `db.users.find({age: {$gt: 30}}).sort({age: -1}).limit(10)`, `aggregate`, `countDocuments`, `insertMany`, `updateMany` …, or a command as JSON. Fields are guessed from 50 documents |
| Qdrant · Chroma · Milvus | Collections are tables. The console takes REST requests — see below |

The data source test shows the server version and warns when it is older than the versions Qurry is tested with (PostgreSQL 12, MySQL 5.7, MariaDB 10.5, SQL Server 2017, MongoDB 5, ClickHouse 22, Oracle 19c — CI runs against these). Read-only data sources are enforced for drivers too: MongoDB runs only `find` / `aggregate` (without `$out` / `$merge`) / `count` / `distinct`, vector databases only `GET` and search requests.

---

[← User guide](README.md)
