//! Copy Diagnostics (GUI Help menu, TUI ⌃k): one text to paste into a bug report. Versions, OS, installed driver plugins, how many
//! data sources of each kind (never their names, hosts, users or databases) and the end of the log file; paths and log lines go
//! through `logfile::redact`, with the home folder as `~`
use crate::store::{ConnectionConfig, DbKind};
use std::path::Path;

/// The log's end at most this long, whatever `log_lines` asks for
const LOG_CAP: usize = 64 * 1024;

/// The text: `app` is "gui" or "tui", `log` the log file (`logfile::path()`; the TUI's own), `log_lines` how many of its last lines
pub fn text(app: &str, app_version: &str, sources: &[ConnectionConfig], log: &Path, log_lines: usize) -> String {
    sources.iter().for_each(crate::logfile::hide);
    let os = match os_version() {
        Some(v) => format!("{} {v}", std::env::consts::OS),
        None => std::env::consts::OS.to_string(),
    };
    let home = dirs::home_dir().map(|h| h.display().to_string()).filter(|h| h.len() > 1);
    // Paths and log lines: the home folder as ~, then redacted one line at a time (a quote left open is masked to the end of its
    // line, not of the text)
    let clean = |l: String| {
        let l = home.as_ref().map_or_else(|| l.clone(), |h| l.replace(h.as_str(), "~"));
        crate::logfile::redact(&l).into_owned()
    };
    let mut out = vec![format!("Qurry {app} {app_version} · core {}", crate::VERSION), format!("OS: {os} ({})", std::env::consts::ARCH), String::new()];
    out.push("Driver plugins:".into());
    for name in DbKind::ALL.iter().filter_map(|k| k.driver()) {
        // No version is recorded at install, and a plugin is not launched here: the file's date tells which download it is
        out.push(clean(match crate::plugin::driver_path(name) {
            Some(p) => {
                let date = std::fs::metadata(&p).and_then(|m| m.modified()).map(|t| chrono::DateTime::<chrono::Local>::from(t).format("%Y-%m-%d").to_string());
                format!("  {name}: installed, modified {} ({})", date.unwrap_or_else(|_| "?".into()), p.display())
            }
            None => format!("  {name}: not installed"),
        }));
    }
    out.push(String::new());
    // Fixed words and numbers only, so not redacted: a user or database called postgres would turn `postgres: 1` into `<user>: 1`
    out.extend(counts(sources));
    out.push(String::new());
    out.extend(log_tail(log, log_lines).into_iter().map(clean));
    out.join("\n") + "\n"
}

/// How many data sources of each kind, and how many are Production, Read-only, behind an SSH tunnel or SSM: numbers only
fn counts(sources: &[ConnectionConfig]) -> Vec<String> {
    if sources.is_empty() {
        return vec!["Data sources: none".into()];
    }
    let mut out = vec![format!("Data sources: {}", sources.len())];
    for k in DbKind::ALL {
        let n = sources.iter().filter(|c| c.kind == k).count();
        if n > 0 {
            out.push(format!("  {}: {n}", format!("{k:?}").to_lowercase()));
        }
    }
    let n = |f: fn(&ConnectionConfig) -> bool| sources.iter().filter(|c| f(c)).count();
    out.push(format!("  Production: {}", n(|c| c.options.confirm_changes)));
    out.push(format!("  Read-only: {}", n(|c| c.read_only)));
    out.push(format!("  SSH tunnel: {}", n(|c| c.ssh.as_ref().is_some_and(|s| s.enabled))));
    out.push(format!("  SSM: {}", n(|c| c.ssm.as_ref().is_some_and(|s| s.enabled))));
    out
}

/// The log file's last `n` lines, from its last LOG_CAP bytes
fn log_tail(log: &Path, n: usize) -> Vec<String> {
    let Ok(bytes) = std::fs::read(log) else {
        return vec![format!("Log: {} (no log file yet)", log.display())];
    };
    let cut = bytes.len() > LOG_CAP;
    let text = String::from_utf8_lossy(&bytes[bytes.len().saturating_sub(LOG_CAP)..]);
    // A cut starts mid-line: that part is dropped
    let mut lines: Vec<&str> = text.lines().skip(usize::from(cut)).collect();
    lines.drain(..lines.len().saturating_sub(n));
    let mut out = vec![format!("Log: {} (last {} lines)", log.display(), lines.len())];
    out.extend(lines.into_iter().map(String::from));
    out
}

/// The OS release, read from a file only (no command is run): macOS ProductVersion, Linux PRETTY_NAME
fn os_version() -> Option<String> {
    if cfg!(target_os = "macos") {
        let plist = std::fs::read_to_string("/System/Library/CoreServices/SystemVersion.plist").ok()?;
        let after = &plist[plist.find("<key>ProductVersion</key>")?..];
        let v = &after[after.find("<string>")? + 8..];
        Some(v[..v.find("</string>")?].trim().to_string())
    } else if cfg!(target_os = "linux") {
        let os = std::fs::read_to_string("/etc/os-release").ok()?;
        os.lines().find_map(|l| l.strip_prefix("PRETTY_NAME=")).map(|v| v.trim_matches('"').to_string())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(v: serde_json::Value) -> ConnectionConfig {
        serde_json::from_value(v).unwrap()
    }

    fn sources() -> Vec<ConnectionConfig> {
        vec![
            cfg(serde_json::json!({ "id": "1", "name": "acme prod", "kind": "postgres", "host": "db.acme.test", "port": 5432, "user": "alice",
                "database": "shop", "readOnly": true, "options": { "confirmChanges": true },
                "ssh": { "enabled": true, "host": "bastion.acme.test", "user": "tunneler", "auth": "key", "keyPath": "~/.ssh/id_ed25519" } })),
            cfg(serde_json::json!({ "id": "2", "name": "acme cache", "kind": "redis", "host": "cache.acme.test", "port": 6379, "user": "",
                "ssm": { "enabled": true, "target": "i-0abc12345def67890", "profile": "acme-ops" } })),
            // Named like kinds: the counts still say postgres and mysql
            cfg(serde_json::json!({ "id": "3", "name": "acme ledger", "kind": "mysql", "host": "ledger.acme.test", "port": 3306, "user": "postgres", "database": "mysql" })),
        ]
    }

    /// Secrets, data source details and the home folder never reach the text, from the log or the sources; counts and versions do
    #[test]
    fn has_no_secrets_or_data_source_details() {
        let t = tempfile::tempdir().unwrap();
        let log = t.path().join("qurry.log");
        let home = dirs::home_dir().unwrap().display().to_string();
        let lines = [
            "2026-11-18T10:00:00.000+09:00 [gui 1] first line".to_string(),
            "2026-11-18T10:00:01.000+09:00 [gui 1] connect with password=hunter2".to_string(),
            "2026-11-18T10:00:02.000+09:00 [gui 1] dsn postgres://alice:s3cret@db.acme.test:5432/shop failed".to_string(),
            "2026-11-18T10:00:03.000+09:00 [gui 1] header Authorization: Bearer eyJhbGciOi.tok3n x".to_string(),
            "2026-11-18T10:00:04.000+09:00 [gui 1] key -----BEGIN OPENSSH PRIVATE KEY-----\\nb3BlbnNzaC1rZXk\\n-----END OPENSSH PRIVATE KEY----- done".to_string(),
            "2026-11-18T10:00:05.000+09:00 [gui 1] no such file /Users/someone/.ssh/id_ed25519".to_string(),
            format!("2026-11-18T10:00:06.000+09:00 [gui 1] config in {home}/Library/Application Support/x and acme prod via bastion.acme.test ssm i-0abc12345def67890"),
            "2026-11-18T10:00:07.000+09:00 [gui 1] last line".to_string(),
        ];
        std::fs::write(&log, lines.join("\n") + "\n").unwrap();
        let text = text("gui", "9.8.7", &sources(), &log, 200);
        println!("{text}");
        for w in ["hunter2", "s3cret", "tok3n", "b3BlbnNzaC1rZXk", "db.acme.test", "bastion.acme.test", "alice", "shop", "acme prod", "acme cache", "id_ed25519", "i-0abc12345def67890", "acme-ops", "tunneler"] {
            assert!(!text.contains(w), "{w} in\n{text}");
        }
        assert!(!text.contains(&home), "the home folder in\n{text}");
        assert!(text.contains(&format!("Qurry gui 9.8.7 · core {}", crate::VERSION)), "{text}");
        for w in ["Data sources: 3", "postgres: 1", "mysql: 1", "redis: 1", "Production: 1", "Read-only: 1", "SSH tunnel: 1", "SSM: 1", "first line", "last line", "~/Library/Application Support/x"] {
            assert!(text.contains(w), "{w} not in\n{text}");
        }
        assert!(text.contains(std::env::consts::OS) && text.contains(std::env::consts::ARCH), "{text}");
    }

    /// No log file yet: the log section says so. A longer log: only its last `log_lines` lines
    #[test]
    fn log_section_without_a_file_and_with_a_long_one() {
        let t = tempfile::tempdir().unwrap();
        let log = t.path().join("qurry.log");
        let none = text("tui", "1.0.0", &[], &log, 200);
        assert!(none.contains("(no log file yet)"), "{none}");
        assert!(none.contains("Data sources: none"), "{none}");
        let lines: Vec<String> = (1..=250).map(|i| format!("line {i} end")).collect();
        std::fs::write(&log, lines.join("\n") + "\n").unwrap();
        let long = text("tui", "1.0.0", &[], &log, 200);
        assert!(!long.contains("line 50 end"), "{long}");
        assert!(long.contains("line 51 end") && long.contains("line 250 end"), "{long}");
        assert_eq!(long.lines().filter(|l| l.starts_with("line ")).count(), 200);
    }
}
