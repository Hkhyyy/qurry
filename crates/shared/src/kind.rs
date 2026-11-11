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

    /// Display name (one per kind; callers that know MariaDB from MySQL say so themselves)
    pub fn label(self) -> &'static str {
        match self {
            DbKind::Postgres => "PostgreSQL",
            DbKind::Mysql => "MySQL",
            DbKind::Redis => "Redis",
            DbKind::Sqlite => "SQLite",
            DbKind::Mssql => "SQL Server",
            DbKind::Clickhouse => "ClickHouse",
            DbKind::Duckdb => "DuckDB",
            DbKind::Mongodb => "MongoDB",
            DbKind::Oracle => "Oracle",
            DbKind::Qdrant => "Qdrant",
            DbKind::Chroma => "Chroma",
            DbKind::Milvus => "Milvus",
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
}
