//! stdout is the protocol: with the log file on, it still carries nothing but JSON-RPC

#[cfg(unix)]
#[test]
fn stdout_is_only_json_rpc_while_the_log_file_is_written() {
    use std::io::Write;
    use std::process::{Command, Stdio};
    // HOME decides where the log folder is (logfile::dir): never the user's own
    let home = std::env::temp_dir().join(format!("qurry-mcp-stdout-{}", std::process::id()));
    std::fs::create_dir_all(&home).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_qurry-mcp"))
        .arg("--config-dir")
        .arg(home.join("config"))
        .env("HOME", &home)
        .env_remove("XDG_STATE_HOME")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let requests = [
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}"#,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"list_data_sources","arguments":{}}}"#,
    ];
    let pid = child.id();
    child.stdin.take().unwrap().write_all(format!("{}\n", requests.join("\n")).as_bytes()).unwrap();
    let out = child.wait_with_output().unwrap();
    let stdout = String::from_utf8(out.stdout).unwrap();
    let replies: Vec<serde_json::Value> = stdout.lines().map(|l| serde_json::from_str(l).unwrap_or_else(|e| panic!("not JSON-RPC: {l} ({e})"))).collect();
    assert_eq!(replies.len(), 2, "{stdout}");
    assert!(replies.iter().all(|r| r["jsonrpc"] == "2.0"));
    let dir = if cfg!(target_os = "macos") { home.join("Library/Logs/Qurry") } else { home.join(".local/state/qurry") };
    let log = std::fs::read_to_string(dir.join("qurry.log")).unwrap();
    std::fs::remove_dir_all(&home).unwrap();
    assert!(log.lines().any(|l| l.ends_with(&format!("[mcp {pid}] list_connections"))), "{log}");
}
