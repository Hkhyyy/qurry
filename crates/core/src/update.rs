//! Update check: compare the current version with the latest GitHub release (published automatically on every merge to main).
//! Asks at most once an hour and keeps the result in update-check.json in the config folder. Doesn't ask if QURRY_NO_UPDATE_CHECK=1.
//! HTTP goes through curl (a default tool on macOS·Linux, no extra dependency)
use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const REPO: &str = "Hkhyyy/qurry";
const CACHE_FILE: &str = "update-check.json";
const CHECK_EVERY_SECS: u64 = 60 * 60;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Asset {
    pub name: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Release {
    /// "0.1.2" (the tag without the v)
    pub version: String,
    /// Release page
    pub url: String,
    /// Release notes (markdown)
    pub notes: String,
    pub assets: Vec<Asset>,
    /// Changes in recent releases (newest first, including this release)
    #[serde(default)]
    pub history: Vec<VersionNotes>,
}

/// What changed in one version ("[feat] title (#PR)")
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VersionNotes {
    pub version: String,
    pub changes: Vec<String>,
}

/// Only the changes from the release notes: lines under "## What's Changed" in GitHub's auto notes → "title (#PR)"
pub fn changes(notes: &str) -> Vec<String> {
    let mut out = vec![];
    let mut on = false;
    for line in notes.lines() {
        let t = line.trim();
        if t.starts_with("## ") {
            on = t.starts_with("## What's Changed");
            continue;
        }
        let Some(item) = t.strip_prefix("* ").or_else(|| t.strip_prefix("- ")).filter(|_| on) else { continue };
        // "title by @person in https://…/pull/17"
        let (title, pr) = match item.rsplit_once(" in https://") {
            Some((head, url)) => (head, url.rsplit_once("/pull/").map(|(_, n)| n.trim())),
            None => (item, None),
        };
        let title = title.rsplit_once(" by @").map(|(t, _)| t).unwrap_or(title).trim();
        // Printed to the terminal as is, so strip control characters (ESC etc.)
        let title: String = title.chars().filter(|c| !c.is_control()).collect();
        out.push(match pr {
            Some(n) => format!("{title} (#{n})"),
            None => title.to_string(),
        });
    }
    out
}

/// Changes per version as text (TUI window, --update)
pub fn changes_text(notes: &[VersionNotes]) -> String {
    let mut s = String::new();
    for v in notes {
        s.push_str(&v.version);
        s.push('\n');
        if v.changes.is_empty() {
            s.push_str("  • (no notes)\n");
        }
        for c in &v.changes {
            s.push_str(&format!("  • {c}\n"));
        }
    }
    s
}

impl Release {
    /// Changes in versions newer than the current one (newest first)
    pub fn changes_since(&self, current: &str) -> Vec<VersionNotes> {
        self.history.iter().filter(|v| is_newer(current, &v.version)).cloned().collect()
    }

    pub fn asset(&self, prefix: &str, suffix: &str) -> Option<&Asset> {
        self.assets.iter().find(|a| a.name.starts_with(prefix) && a.name.ends_with(suffix))
    }
}

#[derive(Serialize, Deserialize)]
struct Cache {
    checked_at: u64,
    latest: Option<Release>,
}

pub fn disabled() -> bool {
    // Also accept the old name (DBTOOL_NO_UPDATE_CHECK)
    ["QURRY_NO_UPDATE_CHECK", "DBTOOL_NO_UPDATE_CHECK"].iter().any(|k| std::env::var(k).is_ok_and(|v| !v.is_empty() && v != "0"))
}

/// Compare numerically, so "0.1.10" > "0.1.9". If unparsable, treat as not newer
pub fn is_newer(current: &str, latest: &str) -> bool {
    let parse = |s: &str| -> Option<Vec<u64>> { s.trim_start_matches('v').split('.').map(|p| p.parse().ok()).collect() };
    match (parse(current), parse(latest)) {
        (Some(c), Some(l)) => l > c,
        _ => false,
    }
}

fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Read recent releases via the GitHub API: the latest release (+ changes per version)
pub async fn latest_release() -> AppResult<Release> {
    let out = tokio::process::Command::new("curl")
        .args(["-fsSL", "-m", "10", "-H", "Accept: application/vnd.github+json", &format!("https://api.github.com/repos/{REPO}/releases?per_page=30")])
        .output()
        .await
        .map_err(|e| AppError::Msg(format!("curl: {e}")))?;
    if !out.status.success() {
        return Err(AppError::Msg(format!("Update check failed: {}", String::from_utf8_lossy(&out.stderr).trim())));
    }
    parse_releases(&out.stdout)
}

fn parse_release(v: &serde_json::Value) -> Option<Release> {
    let s = |k: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
    let assets = v
        .get("assets")
        .and_then(|a| a.as_array())
        .map(|a| a.iter().map(|x| Asset { name: x["name"].as_str().unwrap_or("").into(), url: x["browser_download_url"].as_str().unwrap_or("").into() }).collect())
        .unwrap_or_default();
    let version = s("tag_name").trim_start_matches('v').to_string();
    (!version.is_empty()).then(|| Release { version, url: s("html_url"), notes: s("body"), assets, history: vec![] })
}

/// Release list (draft·prerelease excluded) → highest version + changes per version
fn parse_releases(json: &[u8]) -> AppResult<Release> {
    let v: serde_json::Value = serde_json::from_slice(json)?;
    let mut all: Vec<Release> = v
        .as_array()
        .map(|a| a.iter().filter(|r| !r["draft"].as_bool().unwrap_or(false) && !r["prerelease"].as_bool().unwrap_or(false)).filter_map(parse_release).collect())
        .unwrap_or_default();
    all.sort_by(|a, b| if is_newer(&a.version, &b.version) { std::cmp::Ordering::Greater } else if is_newer(&b.version, &a.version) { std::cmp::Ordering::Less } else { std::cmp::Ordering::Equal });
    let history = all.iter().map(|r| VersionNotes { version: r.version.clone(), changes: changes(&r.notes) }).collect();
    let mut latest = all.into_iter().next().ok_or_else(|| AppError::Msg("Update check failed: no release found".into()))?;
    latest.history = history;
    Ok(latest)
}

/// Some if there's a new version. Unless force, asks the network at most once an hour and uses the saved result in between
pub async fn check(dir: &Path, current: &str, force: bool) -> AppResult<Option<Release>> {
    let path = dir.join(CACHE_FILE);
    let cached: Option<Cache> = std::fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str(&t).ok());
    let latest = match cached {
        Some(c) if !force && now().saturating_sub(c.checked_at) < CHECK_EVERY_SECS => c.latest,
        _ => {
            let r = latest_release().await?;
            let _ = std::fs::write(&path, serde_json::to_string(&Cache { checked_at: now(), latest: Some(r.clone()) })?);
            Some(r)
        }
    };
    Ok(latest.filter(|r| is_newer(current, &r.version)))
}

/// Update signing public key (same as plugins.updater.pubkey in the GUI tauri.conf.json — checked by a test)
pub const UPDATE_PUBKEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDA0MjA1NDlDN0EyODFEM0YKUldRL0hTaDZuRlFnQklTVks5ZFNkdVBNamNqK0JJWEY0akRmR3p3azVGQ2ZiTXUwM2xiL3VtUWwK";


/// Verify the SHA256SUMS signature (the release's SHA256SUMS.sig, tauri signer format = base64 of the minisign signature text)
pub fn verify_sums(sums: &[u8], sig_b64: &str) -> Result<(), String> {
    use base64::Engine;
    let b64 = |s: &str| -> Result<String, String> {
        let raw = base64::engine::general_purpose::STANDARD.decode(s.trim()).map_err(|e| format!("bad base64: {e}"))?;
        String::from_utf8(raw).map_err(|e| e.to_string())
    };
    let pk = minisign_verify::PublicKey::decode(&b64(UPDATE_PUBKEY)?).map_err(|e| format!("bad public key: {e}"))?;
    let sig = minisign_verify::Signature::decode(&b64(sig_b64)?).map_err(|e| format!("bad signature: {e}"))?;
    pk.verify(sums, &sig, false).map_err(|e| format!("signature check failed: {e}"))
}

/// Hash for a file name from SHA256SUMS
pub fn expected_sha(sums: &str, name: &str) -> Option<String> {
    sums.lines().find_map(|l| {
        let (hash, file) = l.split_once(char::is_whitespace)?;
        (file.trim().trim_start_matches('*') == name).then(|| hash.trim().to_lowercase())
    })
}

/// SHA-256 of a file (hex)
pub fn sha256_of(path: &Path) -> std::io::Result<String> {
    use sha2::Digest;
    let mut h = sha2::Sha256::new();
    std::io::copy(&mut std::fs::File::open(path)?, &mut h)?;
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_compare() {
        assert!(is_newer("0.1.0", "0.1.1"));
        assert!(is_newer("0.1.9", "0.1.10"));
        assert!(is_newer("0.1.10", "0.2.0"));
        assert!(is_newer("0.1.0", "v1.0.0"));
        assert!(!is_newer("0.1.1", "0.1.1"));
        assert!(!is_newer("0.2.0", "0.1.9"));
        assert!(!is_newer("0.1.0", "garbage"));
    }

    #[test]
    fn parses_github_release() {
        let json = br#"{"tag_name":"v0.1.2","html_url":"https://github.com/Hkhyyy/dbtool/releases/tag/v0.1.2","body":"notes",
            "assets":[{"name":"dbtool_0.1.2_aarch64.dmg","browser_download_url":"https://x/dmg"},
                      {"name":"dbtool-cli_0.1.2_aarch64-apple-darwin.tar.gz","browser_download_url":"https://x/tgz"}]}"#;
        let r = parse_release(&serde_json::from_slice(json).unwrap()).unwrap();
        assert_eq!(r.version, "0.1.2");
        assert_eq!(r.asset("dbtool_", ".dmg").unwrap().url, "https://x/dmg");
        assert_eq!(r.asset("dbtool-cli_", "aarch64-apple-darwin.tar.gz").unwrap().url, "https://x/tgz");
    }

    /// Only the changes from the release notes (What's Changed in GitHub's auto notes: title (#PR))
    #[test]
    fn changes_from_release_notes() {
        let body = "## dbtool 0.1.12 (macOS)\n\n### Install\n* not this\n\n## What's Changed\n* [fix] ; 없는 문장 나누기 by @Hkhyyy in https://github.com/Hkhyyy/dbtool/pull/17\n* [feat] 다른 것 by @someone in https://github.com/Hkhyyy/dbtool/pull/16\n\n## New Contributors\n* @x made their first contribution\n\n**Full Changelog**: https://github.com/Hkhyyy/dbtool/compare/v0.1.11...v0.1.12";
        assert_eq!(changes(body), ["[fix] ; 없는 문장 나누기 (#17)", "[feat] 다른 것 (#16)"]);
        assert!(changes("no section").is_empty());
        // Terminal control characters in PR titles are removed (--update prints them as is)
        assert_eq!(changes("## What's Changed\n* [fix] a\x1b]0;x\x07b by @u in https://github.com/Hkhyyy/qurry/pull/9"), ["[fix] a]0;xb (#9)"]);
    }

    /// Release list → latest + changes per version, only those newer than the current version (newest first)
    #[test]
    fn changes_since_current_version() {
        let json = br###"[
            {"tag_name":"v0.1.12","html_url":"u12","body":"## What's Changed\n* [fix] c by @a in https://github.com/Hkhyyy/dbtool/pull/17","assets":[],"draft":false,"prerelease":false},
            {"tag_name":"v0.1.13-rc","html_url":"x","body":"","assets":[],"draft":false,"prerelease":true},
            {"tag_name":"v0.1.11","html_url":"u11","body":"## What's Changed\n* [feat] b by @a in https://github.com/Hkhyyy/dbtool/pull/16","assets":[],"draft":false,"prerelease":false},
            {"tag_name":"v0.1.10","html_url":"u10","body":"## What's Changed\n* [feat] a by @a in https://github.com/Hkhyyy/dbtool/pull/15","assets":[],"draft":false,"prerelease":false}
        ]"###;
        let r = parse_releases(json).unwrap();
        assert_eq!(r.version, "0.1.12");
        assert_eq!(r.url, "u12");
        let since = r.changes_since("0.1.10");
        assert_eq!(since, vec![
            VersionNotes { version: "0.1.12".into(), changes: vec!["[fix] c (#17)".into()] },
            VersionNotes { version: "0.1.11".into(), changes: vec!["[feat] b (#16)".into()] },
        ]);
    }

    /// Text printed in the TUI window·--update: changes per version
    #[test]
    fn changes_as_text() {
        let v = vec![
            VersionNotes { version: "0.1.12".into(), changes: vec!["[fix] c (#17)".into()] },
            VersionNotes { version: "0.1.11".into(), changes: vec![] },
        ];
        assert_eq!(changes_text(&v), "0.1.12\n  • [fix] c (#17)\n0.1.11\n  • (no notes)\n");
        assert_eq!(changes_text(&[]), "");
    }

    #[tokio::test]
    async fn cached_result_is_used_within_an_hour() {
        let dir = std::env::temp_dir().join(format!("dbtool-upd-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let rel = Release { version: "9.9.9".into(), url: "u".into(), notes: String::new(), assets: vec![], history: vec![] };
        std::fs::write(dir.join(CACHE_FILE), serde_json::to_string(&Cache { checked_at: now(), latest: Some(rel.clone()) }).unwrap()).unwrap();
        // Answers from the saved result without the network
        assert_eq!(check(&dir, "0.1.0", false).await.unwrap(), Some(rel));
        assert_eq!(check(&dir, "9.9.9", false).await.unwrap(), None);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

