//! Logic shared by the GUI (compiled to WASM by `qurry-shared-wasm`), the TUI and the backend.
//! Pure Rust only: no I/O, no async, no platform APIs, so it builds for `wasm32-unknown-unknown`.
//! qurry-core re-exports these modules (qurry_core::guard, qurry_core::sql, qurry_core::store::DbKind), so there is one copy.

pub mod automap;
pub mod errors;
pub mod extract;
pub mod format;
pub mod fuzzy;
pub mod guard;
pub mod keytree;
pub mod kind;
pub mod labels;
pub mod redis_cmd;
pub mod sql;

pub use kind::DbKind;

use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Deserialize)]
pub struct PingRequest {
    pub text: String,
}

#[derive(Debug, PartialEq, Serialize)]
pub struct PingResponse {
    pub text: String,
    /// Length in characters (not bytes), so callers can see the string crossed the boundary intact
    pub chars: usize,
}

/// Trivial round trip that checks the WASM pipeline end to end.
pub fn ping(req: PingRequest) -> PingResponse {
    PingResponse { chars: req.text.chars().count(), text: req.text }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ping_counts_chars_not_bytes() {
        let r = ping(PingRequest { text: "héllo 한글".into() });
        assert_eq!(r, PingResponse { text: "héllo 한글".into(), chars: 8 });
    }
}
