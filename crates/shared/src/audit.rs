//! Production log words the GUI and the TUI read back (the GUI through WASM), next to the rule that reads them

/// Log entry for a MySQL console transaction with logged changes that a statement ended either way: a CALL (or EXECUTE) whose
/// procedure committed or rolled it back, a deadlock or lock wait timeout (after a DDL's commit, or rolling it back)
pub const ENDED_UNKNOWN: &str = "ENDED (committed or rolled back)";
/// Log entry for a COMMIT cut off by a lost connection: the server may have committed before the connection went
pub const COMMIT_UNKNOWN: &str = "COMMIT outcome unknown (connection lost)";

/// Whether the log entry is one whose outcome is unknown (the changes may or may not be saved): its note says what to do
pub fn outcome_unknown(sql: &str) -> bool {
    sql == COMMIT_UNKNOWN || sql == ENDED_UNKNOWN
}

/// Result word of a Production log entry (GUI and TUI), or None when its rows say it. started = logged before it ran with no result
/// (still running, or Qurry stopped: the outcome, even of a COMMIT, is unknown); failed = it has an error. A transaction end
/// ("COMMIT", "COMMIT (implicit)", "ROLLBACK (by the server)" …) says how it ended
pub fn status(started: bool, sql: &str, failed: bool) -> Option<&'static str> {
    if started {
        Some("no result recorded")
    } else if outcome_unknown(sql) {
        Some("outcome unknown")
    } else if failed {
        Some("failed")
    } else if sql.starts_with("COMMIT") {
        Some("committed")
    } else if sql.starts_with("ROLLBACK") {
        Some("rolled back")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_of_log_entries() {
        assert_eq!(status(true, "COMMIT", false), Some("no result recorded"));
        assert_eq!(status(false, COMMIT_UNKNOWN, true), Some("outcome unknown"));
        assert_eq!(status(false, ENDED_UNKNOWN, false), Some("outcome unknown"));
        assert_eq!(status(false, "delete from t where id = 1", true), Some("failed"));
        for (sql, word) in [("COMMIT", "committed"), ("COMMIT (implicit)", "committed"), ("ROLLBACK (by the server)", "rolled back"), ("ROLLBACK (disconnected)", "rolled back")] {
            assert_eq!(status(false, sql, false), Some(word), "{sql}");
        }
        assert_eq!(status(false, "delete from t where id = 1", false), None);
    }
}
