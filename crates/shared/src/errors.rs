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

#[cfg(test)]
mod tests {
    use super::*;

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
