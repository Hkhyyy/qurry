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

/// What became of a console's open transaction when its connection was lost while a statement ran (core's recover, after the
/// error and how the connection went)
pub const TX_ROLLED_BACK: &str = "the open transaction was rolled back by the server";
/// What a console says when its session was closed with a transaction open before the statement was sent (a disconnect, a dead
/// tunnel: core's lost_tx)
pub const TX_LOST_NOTHING_SENT: &str = "The transaction is lost (the server rolls it back), and nothing was sent. Run it again in a new session.";

/// The server's setting that ends a session sitting idle in a transaction: PostgreSQL's idle_in_transaction_session_timeout,
/// MySQL's wait_timeout (any idle session)
pub fn idle_tx_setting(mysql: bool) -> &'static str {
    if mysql { "wait_timeout" } else { "idle_in_transaction_session_timeout" }
}

/// The server ended a console's session that sat idle with a transaction open (after the server's error)
pub fn idle_tx_ended(mysql: bool) -> String {
    format!("the server ended the session because it sat idle in a transaction longer than its {} allows, so {TX_ROLLED_BACK}. Run it again to reconnect.", idle_tx_setting(mysql))
}

/// A Production console whose transaction has stayed open and unused (GUI Output and status bar, TUI status line). changed = a
/// change ran in it; timeout = the server's limit for it, in seconds, when it has one (and keep-alive doesn't restart its clock)
pub fn tx_idle_reminder(source: &str, minutes: u64, changed: bool, timeout: Option<(bool, u64)>) -> String {
    let ends = match timeout {
        Some((mysql, s)) => format!(" The server ends it after {} idle ({}), and rolls it back.", duration(s), idle_tx_setting(mysql)),
        None => String::new(),
    };
    let what = match changed {
        true => "its changes aren't saved and the rows it changed stay locked. Commit or Roll back.",
        false => "nothing was changed in it, but it keeps the locks of what ran in it (an ALTER TABLE on those tables waits). Commit or Roll back to end it.",
    };
    format!("A transaction on Production \"{source}\" has been open for {minutes} min without a statement: {what}{ends}")
}

/// A read that ran outside a Production console's Manual transaction, in a read-only one of its own, tried to change data (a
/// function that writes): the server refused it (after the server's error)
pub const READ_FIRST_WROTE: &str = "Nothing was changed: on Production, a read run before the console's transaction is open runs in a read-only transaction of its own, so it can't hold locks or commit by itself, and this one tried to change data (a function that writes?). Run BEGIN first, then run it again: it then runs in the console's transaction and its changes stay pending until Commit or Roll back.";

/// The first time a console of a Production data source starts in Manual mode (GUI Output, TUI status line)
pub const PRODUCTION_MANUAL: &str = "Production: this console starts in Tx: Manual, so changes stay pending until you Commit or Roll back. To start them in Auto, turn off \"Production: start consoles in Manual transaction mode\" for this data source (GUI: Data Source Properties › Options · TUI: the data source form, \"Production Manual\").";

/// "90 s", "10 min", "2 h 5 min"
fn duration(s: u64) -> String {
    match s {
        _ if s < 120 => format!("{s} s"),
        _ if s < 7200 => format!("{} min", s / 60),
        _ if s % 3600 / 60 == 0 => format!("{} h", s / 3600),
        _ => format!("{} h {} min", s / 3600, s % 3600 / 60),
    }
}

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

/// The status while Qurry waits for or replaces a data source's tunnel after the Mac woke or the tunnel stopped answering (GUI and
/// TUI ask for it: AppState::reconnecting, and append "… <elapsed>"). ssm = through AWS SSM, else SSH
pub fn reconnecting(name: &str, ssm: bool) -> String {
    format!("Reconnecting {name} through {}", tunnel_way(ssm))
}

/// After this long reconnecting the apps add why it may take a while (reconnect_slow)
pub const RECONNECT_SLOW: std::time::Duration = std::time::Duration::from_secs(10);

/// What the apps add to reconnecting after RECONNECT_SLOW
pub fn reconnect_slow(ssm: bool) -> String {
    format!("Taking longer than usual: after the Mac sleeps, the {} session can take a while to come back.", tunnel_way(ssm))
}

/// The heading of a load that failed while its data source was reconnecting: the reason follows, then how to retry
pub fn reconnect_failed(name: &str, ssm: bool) -> String {
    format!("Couldn't reconnect {name} through {}", tunnel_way(ssm))
}

/// The error of a load whose data source couldn't be reconnected (the core: its tunnel didn't open again while the apps said it
/// reconnects): reconnect_failed, then the reason
pub fn reconnect_failed_error(name: &str, ssm: bool, reason: &str) -> String {
    format!("{}: {reason}", reconnect_failed(name, ssm))
}

/// The reason in a load's error that says its data source couldn't be reconnected (reconnect_failed_error, `failed` its heading).
/// None for any other error: the load's own, also after a reconnect (the GUI: reconnect.ts reconnectReason)
pub fn reconnect_reason<'a>(failed: &str, e: &'a str) -> Option<&'a str> {
    e.strip_prefix(failed)?.strip_prefix(": ")
}

/// A load cancelled while its data source was reconnecting
pub fn reconnect_cancelled(name: &str, ssm: bool) -> String {
    format!("Cancelled while reconnecting {name} through {}.", tunnel_way(ssm))
}

fn tunnel_way(ssm: bool) -> &'static str {
    if ssm { "AWS SSM" } else { "SSH" }
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

pub fn edit_missing_pk(table: &str, pk: &[String]) -> String {
    format!("Read-only: the result doesn't include the primary key of {table} ({}).", pk.join(", "))
}

/// A join's outer side where no row of the table matched (its key is NULL in that row)
pub fn edit_no_row(table: &str) -> String {
    format!("Read-only: no {table} row matches this row (outer join).")
}

/// A table joined with itself, where Qurry can't tell which of the two a column is (a bare `*` with USING, a bare column name)
pub fn edit_self_join(table: &str) -> String {
    format!("Read-only: {table} is in the join more than once, and Qurry can't tell which one this column comes from. Select it as alias.column.")
}

/// MySQL: a bare `*` over a join with USING or NATURAL lists the merged columns once, so it can't be lined up with the tables
pub const EDIT_STAR_USING: &str = "Read-only: with USING or NATURAL, Qurry can't tell which table each column of * comes from. List the columns, or use alias.*.";
/// Add, delete and clone rows on a join result: its rows aren't rows of one table (GUI buttons, TUI status line)
pub const EDIT_JOIN_ROWS: &str = "Rows can be added or deleted only in a result from one table. In a join, edit the cells of a table whose primary key is in the result.";

/// A temporary table lives in the console's session only: edits saved on another connection would miss it (PostgreSQL) or change
/// the base table it shadows (MySQL)
pub fn edit_temporary(table: &str) -> String {
    format!("Read-only: {table} is a temporary table (edits are saved on another connection).")
}

/// what: "view", "materialized view", "foreign table" …
pub fn edit_not_table(table: &str, what: &str) -> String {
    format!("Read-only: {table} is a {what}, not a table.")
}

/// A server error with the spot in the statement it points at (PostgreSQL's 1-based character position), as the core words it:
/// the console finds it again with [`error_position`] to mark that spot
pub fn with_position(msg: &str, pos: usize) -> String {
    format!("{msg} (position {pos})")
}

/// `msg` without the position [`with_position`] added: for where the error is shown next to its mark (the mark is the spot)
pub fn without_position(msg: &str) -> &str {
    let t = msg.trim_end();
    t.strip_suffix(')').and_then(|r| r.rfind(" (position ").filter(|&i| r[i + 11..].parse::<usize>().is_ok()).map(|i| &t[..i])).unwrap_or(msg)
}

/// Where a failed statement's error points, as a byte offset into `sql` (the text that was sent): PostgreSQL's position
/// ([`with_position`]), MySQL's "near '…' at line N" (the quoted text is the rest of the statement from the spot, cut short after
/// 80 characters; `near ''` is its end). None when the error doesn't say (the console then marks the whole statement)
pub fn error_position(msg: &str, sql: &str) -> Option<usize> {
    let msg = msg.trim_end();
    if let Some(rest) = msg.strip_suffix(')')
        && let Some(i) = rest.rfind(" (position ")
        && let Ok(n) = rest[i + 11..].parse::<usize>()
    {
        return sql.char_indices().map(|(b, _)| b).chain([sql.len()]).nth(n.checked_sub(1)?);
    }
    // The first "near '" is MySQL's own (the quoted text may hold another); " at line N" ends the message
    let from = msg.find("to use near '")? + 13;
    let at = msg.rfind("' at line ")?;
    let line: usize = msg[at + 10..].parse().ok()?;
    let near = msg.get(from..at)?;
    let start = sql.split_inclusive('\n').take(line.checked_sub(1)?).map(str::len).sum::<usize>();
    if start > sql.len() {
        return None;
    }
    let body = sql.trim_end();
    if body.ends_with(near) && body.len() - near.len() >= start {
        return Some(body.len() - near.len());
    }
    // Cut short: where the whole quoted text starts, from that line on
    Some(sql[start..].find(near).map_or(start, |j| start + j))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn console_texts_match_the_gui() {
        let gui = include_str!("../../../src/components/ResultGrid.tsx");
        assert!(gui.contains(&format!("\"{CONSOLE_READ_ONLY}\"")), "ResultGrid.tsx lacks: {CONSOLE_READ_ONLY}");
    }

    /// The reconnect texts name the source and the way: AWS SSM or SSH
    #[test]
    fn reconnecting_names_the_tunnel() {
        assert_eq!(reconnecting("prod", true), "Reconnecting prod through AWS SSM");
        assert_eq!(reconnecting("stage", false), "Reconnecting stage through SSH");
        assert_eq!(reconnect_failed("prod", true), "Couldn't reconnect prod through AWS SSM");
        assert_eq!(reconnect_cancelled("stage", false), "Cancelled while reconnecting stage through SSH.");
        assert!(reconnect_slow(true).contains("the AWS SSM session") && reconnect_slow(false).contains("the SSH session"));
        let e = reconnect_failed_error("prod", true, "TargetNotConnected");
        assert_eq!(reconnect_reason(&reconnect_failed("prod", true), &e), Some("TargetNotConnected"));
        assert_eq!(reconnect_reason(&reconnect_failed("prod", true), "permission denied for schema x"), None);
        assert_eq!(reconnect_reason(&reconnect_failed("prod", false), &e), None);
    }

    #[test]
    fn error_position_finds_the_spot() {
        // PostgreSQL: characters, 1-based (a multi-byte one before it counts once)
        let sql = "select '한' fro t";
        assert_eq!(error_position(&with_position("syntax error at or near \"t\"", 16), sql), Some(sql.find(" t").unwrap() + 1));
        assert_eq!(error_position(&with_position("x", 1), sql), Some(0));
        assert_eq!(error_position(&with_position("x", 99), sql), None);
        // MySQL: the line, then the quoted rest of the statement on it
        let sql = "select a,\n  b fro t\nwhere x";
        let my = |near: &str, line| format!("1064 (42000): You have an error in your SQL syntax; check the manual that corresponds to your MySQL server version for the right syntax to use near '{near}' at line {line}");
        assert_eq!(error_position(&my("fro t\nwhere x", 2), sql), Some(sql.find("fro").unwrap()));
        // The same text earlier in the statement is not the spot
        assert_eq!(error_position(&my("x", 3), "select x\nfrom t\nwhere x"), Some("select x\nfrom t\nwhere ".len()));
        assert_eq!(error_position(&my("", 1), "select 1 +  "), Some("select 1 +".len()));
        // A short near text that also comes earlier on its line: the rest of the statement says where
        assert_eq!(error_position(&my("t", 1), "select 1 fro t"), Some("select 1 fro ".len()));
        let sql = "select 1 as a from dual where 1 = 1 and 2 = 1 1";
        assert_eq!(error_position(&my("1", 1), sql), Some(sql.len() - 1));
        // The quoted text holding " near '" itself
        let sql = "select 1 fro t where x = ' near ' and y = 2";
        assert_eq!(error_position(&my("t where x = ' near ' and y = 2", 1), sql), Some("select 1 fro ".len()));
        // Cut short after 80 characters: the whole quoted text is found from its line on
        let sql = format!("select a from\nt where b = 1 fro {} and c = 1", "x".repeat(90));
        let cut: String = sql[sql.find("fro x").unwrap()..].chars().take(80).collect();
        assert_eq!(error_position(&my(&cut, 2), &sql), sql.find("fro x"));
        assert_eq!(without_position(&with_position("ERROR: relation \"t\" does not exist", 15)), "ERROR: relation \"t\" does not exist");
        assert_eq!(without_position("f(x) (position y)"), "f(x) (position y)");
        // Errors that don't say
        assert_eq!(error_position("ERROR: relation \"t\" does not exist", "select * from t"), None);
        assert_eq!(error_position("Unknown column 'x' in 'field list'", "select x"), None);
    }

    #[test]
    fn transaction_texts_name_the_server_setting() {
        assert!(idle_tx_ended(false).contains("longer than its idle_in_transaction_session_timeout allows") && idle_tx_ended(false).contains(TX_ROLLED_BACK));
        assert!(idle_tx_ended(true).contains("wait_timeout"));
        assert!(tx_idle_reminder("prod", 5, true, None).ends_with("the rows it changed stay locked. Commit or Roll back."));
        // Only reads ran in it (a typed BEGIN): no changes or changed rows to speak of, but its locks
        let read = tx_idle_reminder("prod", 5, false, None);
        assert!(!read.contains("its changes") && !read.contains("rows it changed") && read.contains("nothing was changed") && read.contains("ALTER TABLE"), "{read}");
        for (s, says) in [(90, "90 s idle"), (600, "10 min idle"), (28800, "8 h idle"), (7500, "2 h 5 min idle")] {
            assert!(tx_idle_reminder("prod", 5, true, Some((true, s))).contains(&format!("after {says} (wait_timeout)")), "{s}");
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
