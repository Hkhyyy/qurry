//! Real servers for the ignored tests, read one way by every core test module:
//! QURRY_TEST_PG=host:port:user:db[:password] · QURRY_TEST_MY=host:port:user:password:db.
//! Unset: None, and the test returns without running.

use crate::store::{ConnectionConfig, DbKind};

/// A test server: the data source to save or connect with, and its password
pub struct Server {
    pub cfg: ConnectionConfig,
    pub password: Option<String>,
}

/// The test server of a kind: QURRY_TEST_PG for PostgreSQL, QURRY_TEST_MY for MySQL
pub fn server(kind: DbKind) -> Option<Server> {
    match kind {
        DbKind::Postgres => pg(),
        DbKind::Mysql => my(),
        k => panic!("no test server variable for {k:?}"),
    }
}

/// QURRY_TEST_PG=host:port:user:db[:password] (the password is the rest of the line, so it may hold ':')
pub fn pg() -> Option<Server> {
    std::env::var("QURRY_TEST_PG").ok().map(|e| parse_pg(&e))
}

/// QURRY_TEST_MY=host:port:user:password:db
pub fn my() -> Option<Server> {
    my_from("QURRY_TEST_MY")
}

/// Another MySQL in the QURRY_TEST_MY format (e.g. QURRY_TEST_MY_NOTLS)
pub fn my_from(var: &str) -> Option<Server> {
    std::env::var(var).ok().map(|e| parse_my(var, &e))
}

/// A QURRY_TEST_PG line (also a server that isn't there, for tests that fail before connecting)
pub fn parse_pg(e: &str) -> Server {
    let p: Vec<&str> = e.splitn(5, ':').collect();
    assert!(p.len() >= 4, "QURRY_TEST_PG must be host:port:user:db[:password], not {e:?}");
    Server { cfg: cfg(DbKind::Postgres, "QURRY_TEST_PG", p[0], p[1], p[2], p[3]), password: p.get(4).map(|s| s.to_string()) }
}

fn parse_my(var: &str, e: &str) -> Server {
    let p: Vec<&str> = e.split(':').collect();
    assert!(p.len() == 5, "{var} must be host:port:user:password:db, not {e:?}");
    Server { cfg: cfg(DbKind::Mysql, var, p[0], p[1], p[2], p[4]), password: Some(p[3].into()) }
}

fn cfg(kind: DbKind, var: &str, host: &str, port: &str, user: &str, database: &str) -> ConnectionConfig {
    ConnectionConfig {
        id: "test".into(),
        name: "test".into(),
        kind,
        host: host.into(),
        port: port.parse().unwrap_or_else(|_| panic!("{var}: the port {port:?} is not a number")),
        user: user.into(),
        database: database.into(),
        color: None,
        ssh: None,
        password_saved: false,
        ssh_secret_saved: false,
        scope: Default::default(),
        read_only: false,
        options: Default::default(),
        ssl: None,
        redis: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_documented_formats() {
        let s = parse_pg("127.0.0.1:55433:postgres:postgres");
        assert_eq!((s.cfg.kind, s.cfg.host.as_str(), s.cfg.port, s.cfg.user.as_str(), s.cfg.database.as_str(), s.password), (DbKind::Postgres, "127.0.0.1", 55433, "postgres", "postgres", None));
        assert_eq!(parse_pg("h:5432:u:d:p:w").password.as_deref(), Some("p:w"));
        let s = parse_my("QURRY_TEST_MY", "127.0.0.1:13306:root:dbtool:shop");
        assert_eq!((s.cfg.kind, s.cfg.port, s.cfg.user.as_str(), s.cfg.database.as_str(), s.password.as_deref()), (DbKind::Mysql, 13306, "root", "shop", Some("dbtool")));
    }

    #[test]
    #[should_panic(expected = "QURRY_TEST_PG: the port \"x\" is not a number")]
    fn a_bad_port_says_which_variable() {
        parse_pg("h:x:u:d");
    }

    #[test]
    #[should_panic(expected = "QURRY_TEST_MY must be host:port:user:password:db")]
    fn a_short_mysql_line_says_the_format() {
        parse_my("QURRY_TEST_MY", "h:3306:root:shop");
    }
}
