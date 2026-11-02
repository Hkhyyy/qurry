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

#[cfg(test)]
mod tests {
    use super::*;

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
}
