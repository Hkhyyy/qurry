//! Update check: compare the current version with the newest GitHub release of the update channel.
//! Stable = releases from a version tag (vX.Y.Z); beta = also the pre-release published on every merge to main (vX.Y.Z-beta.N).
//! Asks at most once an hour and keeps the result in update-releases.json in the config folder. Doesn't ask if QURRY_NO_UPDATE_CHECK=1.
//! HTTP goes through curl (a default tool on macOS·Linux, no extra dependency)
use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const REPO: &str = "Hkhyyy/qurry";
// Not update-check.json: apps from before the channels read that one, and must keep seeing only stable releases there
const CACHE_FILE: &str = "update-releases.json";
const CHECK_EVERY_SECS: u64 = 60 * 60;

/// Which releases to offer (GUI Settings › Appearance › Updates, TUI ⌃k › Update Channel…). Saved in the config folder, shared by the GUI and the TUI
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Channel {
    /// Only releases from a version tag
    #[default]
    Stable,
    /// Also the pre-release built from every merge to main
    Beta,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Asset {
    pub name: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Release {
    /// "0.1.2" or "0.3.1-beta.2" (the tag without the v)
    pub version: String,
    /// Release page
    pub url: String,
    /// Release notes (markdown)
    pub notes: String,
    pub assets: Vec<Asset>,
    /// Changes in recent releases (newest first, including this release)
    #[serde(default)]
    pub history: Vec<VersionNotes>,
    /// A beta (GitHub pre-release)
    #[serde(default)]
    pub prerelease: bool,
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

/// Newest release of each channel
#[derive(Serialize, Deserialize)]
struct Latest {
    stable: Option<Release>,
    beta: Option<Release>,
}

impl Latest {
    fn of(self, channel: Channel) -> Option<Release> {
        match channel {
            Channel::Stable => self.stable,
            Channel::Beta => self.beta,
        }
    }
}

#[derive(Serialize, Deserialize)]
struct Cache {
    checked_at: u64,
    latest: Latest,
}

pub fn disabled() -> bool {
    // Also accept the old name (DBTOOL_NO_UPDATE_CHECK)
    ["QURRY_NO_UPDATE_CHECK", "DBTOOL_NO_UPDATE_CHECK"].iter().any(|k| std::env::var(k).is_ok_and(|v| !v.is_empty() && v != "0"))
}

/// Semver order, so "0.1.10" > "0.1.9" and "0.3.1-beta.2" < "0.3.1". If unparsable, treat as not newer
pub fn is_newer(current: &str, latest: &str) -> bool {
    let parse = |s: &str| semver::Version::parse(s.trim_start_matches('v')).ok();
    match (parse(current), parse(latest)) {
        (Some(c), Some(l)) => l > c,
        _ => false,
    }
}

fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// GET a GitHub API path of this repo ("releases?per_page=100")
async fn github(path: &str) -> AppResult<Vec<u8>> {
    let out = tokio::process::Command::new("curl")
        // --compressed: GitHub gzips the release list (100 releases: about 4 MB → 300 KB), so it fits the 10 s on slow links
        .args(["-fsSL", "--compressed", "-m", "10", "-H", "Accept: application/vnd.github+json", &format!("https://api.github.com/repos/{REPO}/{path}")])
        .output()
        .await
        .map_err(|e| AppError::Msg(format!("curl: {e}")))?;
    if !out.status.success() {
        return Err(AppError::Msg(format!("Update check failed: {}", String::from_utf8_lossy(&out.stderr).trim())));
    }
    Ok(out.stdout)
}

/// Read recent releases via the GitHub API: the newest release of each channel (+ changes per version)
async fn latest_releases() -> AppResult<Latest> {
    let all = parse_releases(&github("releases?per_page=100").await?)?;
    let mut stable = pick(&all, Channel::Stable);
    if stable.is_none() {
        // Only betas among the newest 100: GitHub's "latest" is the newest stable release
        stable = github("releases/latest").await.ok().and_then(|b| serde_json::from_slice(&b).ok()).and_then(|v| parse_release(&v)).map(with_own_notes);
    }
    Ok(Latest { stable, beta: pick(&all, Channel::Beta) })
}

/// The newest release of the channel (+ changes per version)
pub async fn latest_release(channel: Channel) -> AppResult<Release> {
    latest_releases().await?.of(channel).ok_or_else(|| AppError::Msg("Update check failed: no release found".into()))
}

fn parse_release(v: &serde_json::Value) -> Option<Release> {
    let s = |k: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
    let assets = v
        .get("assets")
        .and_then(|a| a.as_array())
        .map(|a| a.iter().map(|x| Asset { name: x["name"].as_str().unwrap_or("").into(), url: x["browser_download_url"].as_str().unwrap_or("").into() }).collect())
        .unwrap_or_default();
    let version = s("tag_name").trim_start_matches('v').to_string();
    let prerelease = v["prerelease"].as_bool().unwrap_or(false);
    (!version.is_empty()).then(|| Release { version, url: s("html_url"), notes: s("body"), assets, history: vec![], prerelease })
}

/// A release found on its own (not in the list): its history is just its own changes
fn with_own_notes(mut r: Release) -> Release {
    r.history = vec![VersionNotes { version: r.version.clone(), changes: changes(&r.notes) }];
    r
}

/// Release list → published releases (drafts excluded), in the API's order
fn parse_releases(json: &[u8]) -> AppResult<Vec<Release>> {
    let v: serde_json::Value = serde_json::from_slice(json)?;
    Ok(v.as_array().map(|a| a.iter().filter(|r| !r["draft"].as_bool().unwrap_or(false)).filter_map(parse_release).collect()).unwrap_or_default())
}

/// Newest release the channel offers (stable: no pre-releases; beta: both) + changes per version of that channel, newest first
fn pick(all: &[Release], channel: Channel) -> Option<Release> {
    let mut on: Vec<&Release> = all.iter().filter(|r| channel == Channel::Beta || !r.prerelease).collect();
    on.sort_by(|a, b| if is_newer(&a.version, &b.version) { std::cmp::Ordering::Greater } else if is_newer(&b.version, &a.version) { std::cmp::Ordering::Less } else { std::cmp::Ordering::Equal });
    let history = on.iter().map(|r| VersionNotes { version: r.version.clone(), changes: changes(&r.notes) }).collect();
    let mut latest = (*on.first()?).clone();
    latest.history = history;
    Some(latest)
}

/// Some if the channel has a new version. Unless force, asks the network at most once an hour and uses the saved result in between
/// (it holds both channels, so switching the channel needs no new request)
pub async fn check(dir: &Path, current: &str, channel: Channel, force: bool) -> AppResult<Option<Release>> {
    let path = dir.join(CACHE_FILE);
    let latest = match read_cache(dir) {
        Some(c) if !force && now().saturating_sub(c.checked_at) < CHECK_EVERY_SECS => c.latest,
        _ => {
            let cache = Cache { checked_at: now(), latest: latest_releases().await? };
            let _ = std::fs::write(&path, serde_json::to_string(&cache)?);
            cache.latest
        }
    };
    Ok(latest.of(channel).filter(|r| is_newer(current, &r.version)))
}

fn read_cache(dir: &Path) -> Option<Cache> {
    std::fs::read_to_string(dir.join(CACHE_FILE)).ok().and_then(|t| serde_json::from_str(&t).ok())
}

/// Like check, from the saved result only (any age, never the network): None if nothing was checked yet. For a channel switch
pub fn cached(dir: &Path, current: &str, channel: Channel) -> Option<Option<Release>> {
    read_cache(dir).map(|c| c.latest.of(channel).filter(|r| is_newer(current, &r.version)))
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
        let r = pick(&parse_releases(json).unwrap(), Channel::Stable).unwrap();
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

    /// Pre-releases compare below their release and by number (beta.10 after beta.9), as semver orders them
    #[test]
    fn pre_release_versions_compare() {
        assert!(is_newer("0.3.0", "0.3.1-beta.1"));
        assert!(is_newer("0.3.1-beta.1", "0.3.1-beta.2"));
        assert!(is_newer("0.3.1-beta.9", "0.3.1-beta.10"));
        assert!(is_newer("0.3.1-beta.10", "0.3.1"));
        assert!(!is_newer("0.3.1", "0.3.1-beta.2"));
        assert!(!is_newer("v0.3.1-beta.2", "0.3.1-beta.2"));
    }

    /// Fixture: what GitHub's release list looks like after a few merges (betas) and two version tags (stable), newest first
    const FIXTURE: &[u8] = br###"[
        {"tag_name":"v0.3.1-beta.2","html_url":"b2","body":"## What's Changed\n* [fix] two by @a in https://github.com/Hkhyyy/qurry/pull/82","assets":[],"draft":false,"prerelease":true},
        {"tag_name":"v0.3.2-beta.1","html_url":"draft","body":"","assets":[],"draft":true,"prerelease":true},
        {"tag_name":"v0.3.1-beta.1","html_url":"b1","body":"## What's Changed\n* [feat] one by @a in https://github.com/Hkhyyy/qurry/pull/81","assets":[],"draft":false,"prerelease":true},
        {"tag_name":"v0.3.0","html_url":"s30","body":"## What's Changed\n* [feat] stable by @a in https://github.com/Hkhyyy/qurry/pull/80","assets":[],"draft":false,"prerelease":false},
        {"tag_name":"v0.3.0-beta.1","html_url":"b0","body":"","assets":[],"draft":false,"prerelease":true},
        {"tag_name":"v0.2.9","html_url":"s29","body":"","assets":[],"draft":false,"prerelease":false}
    ]"###;

    /// Stable never offers a pre-release; beta offers the newest of both. Drafts are never offered
    #[test]
    fn channel_picks_from_the_release_list() {
        let all = parse_releases(FIXTURE).unwrap();
        let stable = pick(&all, Channel::Stable).unwrap();
        assert_eq!(stable.version, "0.3.0");
        assert_eq!(stable.history.iter().map(|v| v.version.as_str()).collect::<Vec<_>>(), ["0.3.0", "0.2.9"]);
        let beta = pick(&all, Channel::Beta).unwrap();
        assert_eq!((beta.version.as_str(), beta.url.as_str()), ("0.3.1-beta.2", "b2"));
        assert_eq!(beta.changes_since("0.3.0").iter().map(|v| v.version.as_str()).collect::<Vec<_>>(), ["0.3.1-beta.2", "0.3.1-beta.1"]);
        // Once the version is tagged, beta offers the stable release (it is newer than its betas)
        let tagged = br#"[{"tag_name":"v0.3.1","html_url":"s31","body":"","assets":[],"draft":false,"prerelease":false}]"#;
        let mut all = all;
        all.extend(parse_releases(tagged).unwrap());
        assert_eq!(pick(&all, Channel::Beta).unwrap().version, "0.3.1");
        // Only pre-releases in the list: nothing for stable
        let betas = parse_releases(br#"[{"tag_name":"v0.3.1-beta.1","html_url":"b","body":"","assets":[],"draft":false,"prerelease":true}]"#).unwrap();
        assert!(pick(&betas, Channel::Stable).is_none());
        assert_eq!(pick(&betas, Channel::Beta).unwrap().version, "0.3.1-beta.1");
    }

    #[test]
    fn channel_is_saved_in_lowercase_and_defaults_to_stable() {
        assert_eq!(Channel::default(), Channel::Stable);
        assert_eq!(serde_json::to_value(Channel::Beta).unwrap(), "beta");
        assert_eq!(serde_json::from_value::<Channel>("stable".into()).unwrap(), Channel::Stable);
    }

    #[tokio::test]
    async fn cached_result_is_used_within_an_hour() {
        let dir = std::env::temp_dir().join(format!("dbtool-upd-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let rel = |v: &str| Release { version: v.into(), url: "u".into(), notes: String::new(), assets: vec![], history: vec![], prerelease: v.contains('-') };
        let latest = Latest { stable: Some(rel("9.9.9")), beta: Some(rel("10.0.0-beta.1")) };
        std::fs::write(dir.join(CACHE_FILE), serde_json::to_string(&Cache { checked_at: now(), latest }).unwrap()).unwrap();
        // Answers from the saved result without the network, for the channel asked
        assert_eq!(check(&dir, "0.1.0", Channel::Stable, false).await.unwrap(), Some(rel("9.9.9")));
        assert_eq!(check(&dir, "9.9.9", Channel::Stable, false).await.unwrap(), None);
        assert_eq!(check(&dir, "9.9.9", Channel::Beta, false).await.unwrap(), Some(rel("10.0.0-beta.1")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A channel switch reads the saved result whatever its age, and says when nothing was checked yet (no request either way)
    #[test]
    fn cached_answers_without_the_network() {
        let dir = std::env::temp_dir().join(format!("dbtool-upd-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(cached(&dir, "0.1.0", Channel::Beta), None);
        let rel = |v: &str| Release { version: v.into(), url: "u".into(), notes: String::new(), assets: vec![], history: vec![], prerelease: v.contains('-') };
        let latest = Latest { stable: Some(rel("9.9.9")), beta: Some(rel("10.0.0-beta.1")) };
        std::fs::write(dir.join(CACHE_FILE), serde_json::to_string(&Cache { checked_at: 0, latest }).unwrap()).unwrap();
        assert_eq!(cached(&dir, "9.9.9", Channel::Stable), Some(None));
        assert_eq!(cached(&dir, "9.9.9", Channel::Beta), Some(Some(rel("10.0.0-beta.1"))));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The stable release from releases/latest (only betas in the list) still lists its own changes
    #[test]
    fn fallback_release_has_its_own_changes() {
        let v = serde_json::json!({"tag_name":"v0.3.0","html_url":"s","body":"## What's Changed\n* [feat] x by @a in https://github.com/Hkhyyy/qurry/pull/5","assets":[]});
        let r = with_own_notes(parse_release(&v).unwrap());
        assert_eq!(r.changes_since("0.2.9"), vec![VersionNotes { version: "0.3.0".into(), changes: vec!["[feat] x (#5)".into()] }]);
    }
}
