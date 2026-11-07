//! Error texts the GUI recognises (through WASM) to offer a way out, kept next to the check so the two can't drift apart

/// When the server dropped the connection while TLS was required (the cause isn't known for sure)
pub const DROPPED_TLS: &str = "The server closed the connection while setting up encryption — it may be an old MySQL without TLS 1.2 (then, on a network you trust, turn on \"Allow unencrypted connection\" for this data source: GUI: Options tab · TUI: data source form, \"Allow unencrypted\"), or check the host, port and any proxy in between.";

/// When not falling back to plaintext on a server without TLS
pub const NEEDS_TLS: &str = "This server didn't set up an encrypted (TLS) connection, so Qurry didn't send the password. If it's an old server without TLS 1.2 and the network is trusted, turn on \"Allow unencrypted connection\" for this data source (GUI: Options tab · TUI: data source form, \"Allow unencrypted\").";

/// NEEDS_TLS for PostgreSQL (TLS 1.2 is a MySQL 5.7 matter)
pub fn needs_tls_pg() -> String {
    NEEDS_TLS.replace(" without TLS 1.2", "")
}

/// The connect error is one where turning on "Allow unencrypted connection" for the source is the way forward. Only the core's own error, which
/// starts with the text: a server error that only quotes it (RAISE EXCEPTION, a cast error echoing a value) must not offer to turn off TLS
pub fn tls_opt_in(msg: &str) -> bool {
    let m = msg.trim_start();
    m.starts_with(NEEDS_TLS) || m.starts_with(DROPPED_TLS) || m.starts_with(&needs_tls_pg())
}

/// When the connection was lost during a COMMIT (after the server error)
pub const COMMIT_UNKNOWN_ERR: &str = "connection lost while committing, so it is unknown whether the changes were committed. Check the data before you run them again (the next run reconnects).";

/// Title of the notice for a failed COMMIT (GUI and TUI): it may not have failed when the connection was lost during it
pub fn commit_failed_title(msg: &str) -> &'static str {
    if msg.contains(COMMIT_UNKNOWN_ERR) { "Commit outcome unknown" } else { "Commit failed" }
}

/// The line the result grid keeps when loading more rows failed (GUI and TUI): the result isn't complete, so it says why it stopped
pub fn more_rows_failed(msg: &str) -> String {
    format!("Couldn't load more rows: {}", msg.trim())
}

/// Why the app disconnected a data source by itself (the status line of GUI and TUI: disconnected)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Disconnected {
    /// Auto-disconnect (Options)
    Idle,
    /// Its tunnel's process (aws CLI) or SSH session ended
    TunnelStopped,
    /// Its tunnel carried nothing any more when the keep-alive checked (sleep, a network change)
    TunnelSilent,
}

/// The status line when the app disconnected a data source by itself (GUI and TUI). tunnel = its kind ("AWS SSM tunnel", "SSH tunnel")
pub fn disconnected(name: &str, tunnel: &str, why: Disconnected) -> String {
    match why {
        Disconnected::Idle => format!("Disconnected (idle): {name}"),
        Disconnected::TunnelStopped => format!("Disconnected: the {tunnel} of {name} stopped — the next query opens a new one"),
        Disconnected::TunnelSilent => format!("Disconnected: the {tunnel} of {name} stopped answering — the next query opens a new one"),
    }
}

/// The DBA report's database picker couldn't list the databases (GUI and TUI): it lists through the report's database, or the
/// source's default one, so the way out is a default database the account can connect to
pub fn db_list_failed(msg: &str) -> String {
    format!("Couldn't list the databases: {}\nSet a default database you can connect to (GUI: Data Source Properties › General › Database · TUI: the data source form's Database field).", msg.trim())
}

/// With AWS SSM the instance connects to Host:Port, so they are the database as seen from it
const SSM_HOST_PORT: &str = "are the database as seen from the instance (e.g. an RDS endpoint and 5432), not a local port";

/// The connect error when the AWS SSM instance couldn't reach the database (the plugin said "Connection to destination port failed")
pub fn ssm_unreachable(host: &str, port: u16) -> String {
    format!("AWS SSM: the instance couldn't reach {host}:{port}. With AWS SSM, Host and Port on the General tab (TUI: the data source's host/port) {SSM_HOST_PORT}.")
}

/// The data source form's hint when AWS SSM is on and Host is this machine's name (GUI and TUI). Not an error: a database on the
/// instance itself is reached this way. Not for ::1, which the tunnel doesn't take (a host name or IPv4 address only)
pub fn ssm_local_host(host: &str) -> Option<String> {
    let h = host.trim();
    ["localhost", "127.0.0.1"]
        .iter()
        .any(|l| h.eq_ignore_ascii_case(l))
        .then(|| format!("With AWS SSM, {h} is the instance itself: Host and Port {SSM_HOST_PORT}. Keep it if the database runs on the instance."))
}

// Why a console result can't be edited (result_edit), on its greyed-out edit buttons (GUI) and status line (TUI)

/// Not a single plain SELECT … FROM one table (a CTE, a subquery or function in FROM, SELECT INTO, more than one statement)
pub const EDIT_NOT_PLAIN: &str = "Read-only: only the result of a plain SELECT from one table can be edited.";
/// SELECT without FROM
pub const EDIT_NO_TABLE: &str = "Read-only: the result doesn't come from a table.";
/// A join or FROM a, b
pub const EDIT_MANY_TABLES: &str = "Read-only: the columns come from more than one table.";
/// MySQL: the select list didn't line up with the table's columns
pub const EDIT_UNMATCHED: &str = "Read-only: Qurry couldn't match the result's columns to the table's.";
/// FOR UPDATE / FOR SHARE / LOCK IN SHARE MODE: the console's transaction holds the row locks the edits would wait on
pub const EDIT_LOCKS: &str = "Read-only: the statement locks its rows, and edits are saved on another connection.";
/// Driver plugins don't say where a result column comes from
pub const EDIT_DRIVER: &str = "Read-only: console results can be edited on PostgreSQL and MySQL only.";

/// A console result the backend didn't check (src/components/ResultGrid.tsx CONSOLE_READ_ONLY has the same text)
pub const CONSOLE_READ_ONLY: &str = "Console results are read-only. Open the table to edit its rows.";
/// A console in Manual mode or with an open transaction: grid edits save in a transaction of their own (executeBatch), so they'd
/// commit outside it and wait on its row locks (ResultGrid.tsx CONSOLE_IN_TX)
pub const CONSOLE_IN_TX: &str = "Read-only in a transaction: edits are saved in a transaction of their own. Commit or roll back and switch to Tx: Auto to edit these rows.";

/// GROUP BY, HAVING, DISTINCT, UNION …: one result row isn't one table row
pub fn edit_combined(what: &str) -> String {
    format!("Read-only: the result uses {what}, so its rows aren't rows of the table.")
}

/// A result column that isn't read straight from the table (an expression, an aggregate, a cast)
pub fn edit_computed(column: &str) -> String {
    format!("Read-only: {column} is computed, not a column of the table.")
}

/// The same result name twice, or the same table column under two names
pub fn edit_duplicate(column: &str) -> String {
    format!("Read-only: the result has {column} more than once.")
}

pub fn edit_no_pk(table: &str) -> String {
    format!("Read-only: {table} has no primary key.")
}

pub fn edit_missing_pk(pk: &[String]) -> String {
    format!("Read-only: the result doesn't include the primary key ({}).", pk.join(", "))
}

/// A temporary table lives in the console's session only: edits saved on another connection would miss it (PostgreSQL) or change
/// the base table it shadows (MySQL)
pub fn edit_temporary(table: &str) -> String {
    format!("Read-only: {table} is a temporary table (edits are saved on another connection).")
}

/// what: "view", "materialized view", "foreign table" …
pub fn edit_not_table(table: &str, what: &str) -> String {
    format!("Read-only: {table} is a {what}, not a table.")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn console_texts_match_the_gui() {
        let gui = include_str!("../../../src/components/ResultGrid.tsx");
        for t in [CONSOLE_READ_ONLY, CONSOLE_IN_TX] {
            assert!(gui.contains(&format!("\"{t}\"")), "ResultGrid.tsx lacks: {t}");
        }
    }

    #[test]
    fn more_rows_failed_keeps_the_reason() {
        assert_eq!(more_rows_failed("Session closed\n"), "Couldn't load more rows: Session closed");
    }

    #[test]
    fn commit_failed_title_says_when_the_outcome_is_unknown() {
        assert_eq!(commit_failed_title(&format!("Connection reset by peer (os error 54) — {COMMIT_UNKNOWN_ERR}")), "Commit outcome unknown");
        assert_eq!(commit_failed_title("Session closed"), "Commit failed");
        assert_eq!(commit_failed_title("1180 (HY000): Got error 149 during COMMIT"), "Commit failed");
    }

    #[test]
    fn tls_opt_in_matches_the_connect_errors_only() {
        for m in [format!("{NEEDS_TLS}\n(error)"), format!("{DROPPED_TLS}\n(io)"), format!("{}\n(tls)", needs_tls_pg())] {
            assert!(tls_opt_in(&m), "{m}");
        }
        assert!(!needs_tls_pg().contains("TLS 1.2") && needs_tls_pg().contains("didn't send the password"));
        for m in ["", "error communicating with database: Connection refused", "password authentication failed for user \"u\"", "TLS handshake failed"] {
            assert!(!tls_opt_in(m), "{m}");
        }
        // A server error quoting the text (RAISE EXCEPTION, a cast error echoing a row value) isn't the core's connect error
        for m in [format!("error returned from database: {NEEDS_TLS}"), format!("ERROR: {}", needs_tls_pg()), format!("invalid input syntax for type integer: \"{DROPPED_TLS}\"")] {
            assert!(!tls_opt_in(&m), "{m}");
        }
    }

    #[test]
    fn disconnected_says_why_with_the_name() {
        assert_eq!(disconnected("prod", "SSH tunnel", Disconnected::Idle), "Disconnected (idle): prod");
        assert_eq!(disconnected("prod", "AWS SSM tunnel", Disconnected::TunnelSilent), "Disconnected: the AWS SSM tunnel of prod stopped answering — the next query opens a new one");
        assert!(disconnected("prod", "SSH tunnel", Disconnected::TunnelStopped).starts_with("Disconnected: the SSH tunnel of prod stopped —"));
    }

    #[test]
    fn ssm_texts() {
        let e = ssm_unreachable("127.0.0.1", 55433);
        assert!(e.contains("couldn't reach 127.0.0.1:55433") && e.contains("General tab") && e.contains("as seen from the instance"), "{e}");
        for h in ["localhost", " LocalHost ", "127.0.0.1"] {
            let hint = ssm_local_host(h).unwrap_or_else(|| panic!("{h}"));
            assert!(hint.contains(h.trim()) && hint.contains("as seen from the instance"), "{hint}");
        }
        // ::1 can't be passed to the instance (the host must be a name or IPv4): not hinted as something to keep
        for h in ["", "::1", "db.internal", "mydb.abc.ap-northeast-2.rds.amazonaws.com", "10.0.1.5", "127.0.0.2", "localhost.example.com"] {
            assert_eq!(ssm_local_host(h), None, "{h}");
        }
    }
}
