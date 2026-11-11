//! Database kind of a data source. Stored lowercase in connections.json, which every app version reads, so names never change.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DbKind {
    Postgres,
    Mysql,
    Redis,
    // Below are driver plugins (downloaded when needed): crate::plugin
    Sqlite,
    Mssql,
    Clickhouse,
    Duckdb,
    Mongodb,
    Oracle,
    // Vector DBs (REST)
    Qdrant,
    Chroma,
    Milvus,
}

impl DbKind {
    /// Driver plugin name (None for the built-in PG·MySQL·Redis)
    pub fn driver(self) -> Option<&'static str> {
        match self {
            DbKind::Postgres | DbKind::Mysql | DbKind::Redis => None,
            DbKind::Sqlite => Some("sqlite"),
            DbKind::Mssql => Some("mssql"),
            DbKind::Clickhouse => Some("clickhouse"),
            DbKind::Duckdb => Some("duckdb"),
            DbKind::Mongodb => Some("mongodb"),
            DbKind::Oracle => Some("oracle"),
            DbKind::Qdrant => Some("qdrant"),
            DbKind::Chroma => Some("chroma"),
            DbKind::Milvus => Some("milvus"),
        }
    }

    /// The DB is the schema (no schema level in the tree)
    pub fn flat(self) -> bool {
        matches!(self, DbKind::Mysql | DbKind::Redis | DbKind::Sqlite | DbKind::Clickhouse | DbKind::Mongodb | DbKind::Qdrant | DbKind::Chroma | DbKind::Milvus)
    }

    /// A single file is the DB (file path instead of host·port)
    pub fn is_file(self) -> bool {
        matches!(self, DbKind::Sqlite | DbKind::Duckdb)
    }

    /// Vector DB: the console is REST (METHOD /path + JSON), not SQL
    pub fn is_vector(self) -> bool {
        matches!(self, DbKind::Qdrant | DbKind::Chroma | DbKind::Milvus)
    }

    /// The server's usual port: a new data source's Port, and a URL without one (file DBs have none: 0)
    pub fn default_port(self) -> u16 {
        match self {
            DbKind::Postgres => 5432,
            DbKind::Mysql => 3306,
            DbKind::Redis => 6379,
            DbKind::Mssql => 1433,
            DbKind::Clickhouse => 8123,
            DbKind::Mongodb => 27017,
            DbKind::Oracle => 1521,
            DbKind::Qdrant => 6333,
            DbKind::Chroma => 8000,
            DbKind::Milvus => 19530,
            DbKind::Sqlite | DbKind::Duckdb => 0,
        }
    }
}

/// The database a target without "::db" connects to, from the data source's Database field: what the tree, the console and
/// the search take as the default (PostgreSQL: postgres when blank; MySQL: none when blank, all schemas; Redis: db<n>)
pub fn default_db(kind: DbKind, database: &str) -> String {
    let d = database.trim();
    match kind {
        DbKind::Postgres if d.is_empty() => "postgres".into(),
        DbKind::Redis => format!("db{}", redis_db_index(d)),
        // File DBs use main (Database is the file path)
        k if k.is_file() => "main".into(),
        // Vector DB: the server's default name if empty
        DbKind::Qdrant if d.is_empty() => "qdrant".into(),
        DbKind::Chroma if d.is_empty() => "default_database".into(),
        DbKind::Milvus if d.is_empty() => "default".into(),
        // Other drivers use the configured DB name as is
        _ => d.into(),
    }
}

/// A Redis database index from its name or the Database field: "db3" / "3" / "" → 3 / 3 / 0
pub fn redis_db_index(name: &str) -> i64 {
    name.trim().trim_start_matches("db").parse().unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [DbKind; 12] = [
        DbKind::Postgres, DbKind::Mysql, DbKind::Redis, DbKind::Sqlite, DbKind::Mssql, DbKind::Clickhouse,
        DbKind::Duckdb, DbKind::Mongodb, DbKind::Oracle, DbKind::Qdrant, DbKind::Chroma, DbKind::Milvus,
    ];
    /// (blank Database, Database " app ") → default db; a match, so a new kind doesn't compile without its row
    fn expected(k: DbKind) -> (&'static str, &'static str) {
        match k {
            DbKind::Postgres => ("postgres", "app"),
            DbKind::Mysql => ("", "app"),
            DbKind::Redis => ("db0", "db0"),
            DbKind::Sqlite | DbKind::Duckdb => ("main", "main"),
            DbKind::Mssql | DbKind::Clickhouse | DbKind::Mongodb | DbKind::Oracle => ("", "app"),
            DbKind::Qdrant => ("qdrant", "app"),
            DbKind::Chroma => ("default_database", "app"),
            DbKind::Milvus => ("default", "app"),
        }
    }

    #[test]
    fn default_db_per_kind() {
        for k in ALL {
            let (blank, set) = expected(k);
            assert_eq!(default_db(k, ""), blank, "{k:?} blank");
            assert_eq!(default_db(k, "  "), blank, "{k:?} spaces");
            assert_eq!(default_db(k, " app "), set, "{k:?} set");
        }
        assert_eq!(default_db(DbKind::Redis, "0"), "db0");
        assert_eq!(default_db(DbKind::Redis, "3"), "db3");
        assert_eq!(default_db(DbKind::Redis, "db11"), "db11");
        assert_eq!(default_db(DbKind::Sqlite, "/tmp/a.db"), "main");
    }

    #[test]
    fn default_port_per_kind() {
        let ports: Vec<(DbKind, u16)> = ALL.iter().map(|&k| (k, k.default_port())).collect();
        assert_eq!(
            ports,
            [
                (DbKind::Postgres, 5432), (DbKind::Mysql, 3306), (DbKind::Redis, 6379), (DbKind::Sqlite, 0), (DbKind::Mssql, 1433),
                (DbKind::Clickhouse, 8123), (DbKind::Duckdb, 0), (DbKind::Mongodb, 27017), (DbKind::Oracle, 1521),
                (DbKind::Qdrant, 6333), (DbKind::Chroma, 8000), (DbKind::Milvus, 19530),
            ]
        );
    }

    #[test]
    fn redis_db_index_reads_name_or_number() {
        assert_eq!(redis_db_index("db11"), 11);
        assert_eq!(redis_db_index(" 3 "), 3);
        assert_eq!(redis_db_index(""), 0);
        assert_eq!(redis_db_index("x"), 0);
    }
}
