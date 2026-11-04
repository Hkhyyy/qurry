use super::*;

/// Copy to the clipboard via OSC 52 (supported by most modern terminals)
pub fn copy(app: &mut App, text: &str) {
    use std::io::Write;
    // When running on this machine, write straight to the system clipboard (many terminals block OSC 52 or truncate it). Over SSH, OSC 52 only (the remote clipboard is meaningless)
    let native = std::env::var_os("SSH_CONNECTION").is_none() && std::env::var_os("SSH_TTY").is_none() && native_copy(text);
    if !native {
        let b64 = base64_encode(text.as_bytes());
        let _ = write!(std::io::stdout(), "\x1b]52;c;{b64}\x07");
        let _ = std::io::stdout().flush();
    }
    let lines = text.lines().count();
    app.status = if lines > 1 { format!("Copied {lines} lines") } else { format!("Copied: {}", text.chars().take(60).collect::<String>()) };
}

/// pbcopy (macOS) · wl-copy / xclip (Linux). true on success
fn native_copy(text: &str) -> bool {
    use std::io::Write;
    // Don't overwrite the clipboard of the person running the tests
    if cfg!(test) {
        return false;
    }
    use std::process::{Command, Stdio};
    let tools: &[(&str, &[&str])] = if cfg!(target_os = "macos") { &[("pbcopy", &[])] } else { &[("wl-copy", &[]), ("xclip", &["-selection", "clipboard"])] };
    for (cmd, args) in tools {
        let Ok(mut child) = Command::new(cmd).args(*args).stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null()).spawn() else { continue };
        let wrote = child.stdin.take().is_some_and(|mut w| w.write_all(text.as_bytes()).is_ok());
        if wrote && child.wait().is_ok_and(|s| s.success()) {
            return true;
        }
    }
    false
}
fn base64_encode(b: &[u8]) -> String {
    const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::new();
    for c in b.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            if i <= c.len() {
                s.push(T[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                s.push('=');
            }
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::Harness;

    /// ⌘C copies the cell value like y (when the terminal passes ⌘C through)
    #[tokio::test]
    async fn cmd_c_copies_cell() {
        let mut h = Harness::new().await;
        h.table_tab("fee_rule", &["idx", "fee_type"], &[&["7", "PERCENT"]], &["idx"]);
        h.key("Right");
        h.key("Cmd+c");
        assert_eq!(h.app.status, "Copied: PERCENT");
    }

    /// Y: copy the whole current column as an IN list (IPs from results straight into the next query's WHERE ip IN)
    #[tokio::test]
    async fn copy_column_as_in_list() {
        let mut h = Harness::new().await;
        h.table_tab("logs", &["ip", "n"], &[&["1.2.3.4", "1"], &["5.6.7.8", "2"], &["1.2.3.4", "3"]], &[]);
        h.key("Y");
        assert_eq!(h.app.status, "Copied 2 values as an IN list: ('1.2.3.4', '5.6.7.8')");
        // From ⌃K too
        assert!(crate::app::ACTIONS.iter().any(|a| a.action == Action::CopyInList));
    }
}
