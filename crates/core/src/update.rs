//! 새 버전 확인: GitHub 의 최신 릴리스(main 에 머지될 때마다 자동으로 올라간다)와 지금 버전을 비교한다.
//! 한 시간에 한 번만 묻고 결과는 설정 폴더의 update-check.json 에 둔다. QURRY_NO_UPDATE_CHECK=1 이면 묻지 않는다.
//! HTTP 는 curl 로 한다 (macOS·Linux 기본 도구, 의존성 추가 없이)
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
    /// "0.1.2" (태그의 v 를 뗀 것)
    pub version: String,
    /// 릴리스 페이지
    pub url: String,
    /// 릴리스 노트 (마크다운)
    pub notes: String,
    pub assets: Vec<Asset>,
    /// 최근 릴리스들의 바뀐 것 (최신부터, 이 릴리스 포함)
    #[serde(default)]
    pub history: Vec<VersionNotes>,
}

/// 한 버전에서 바뀐 것 ("[feat] 제목 (#PR)")
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VersionNotes {
    pub version: String,
    pub changes: Vec<String>,
}

/// 릴리스 노트에서 바뀐 것만: GitHub 자동 노트의 "## What's Changed" 줄 → "제목 (#PR)"
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
        // "제목 by @사람 in https://…/pull/17"
        let (title, pr) = match item.rsplit_once(" in https://") {
            Some((head, url)) => (head, url.rsplit_once("/pull/").map(|(_, n)| n.trim())),
            None => (item, None),
        };
        let title = title.rsplit_once(" by @").map(|(t, _)| t).unwrap_or(title).trim();
        // 터미널에 그대로 찍히므로 제어문자(ESC 등)는 뺀다
        let title: String = title.chars().filter(|c| !c.is_control()).collect();
        out.push(match pr {
            Some(n) => format!("{title} (#{n})"),
            None => title.to_string(),
        });
    }
    out
}

/// 버전마다 바뀐 것을 글로 (TUI 창, --update)
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
    /// 지금 버전보다 새 버전들의 바뀐 것 (최신부터)
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
    // 예전 이름(DBTOOL_NO_UPDATE_CHECK)도 받는다
    ["QURRY_NO_UPDATE_CHECK", "DBTOOL_NO_UPDATE_CHECK"].iter().any(|k| std::env::var(k).is_ok_and(|v| !v.is_empty() && v != "0"))
}

/// "0.1.10" > "0.1.9" 처럼 숫자로 비교한다. 못 읽으면 새 버전이 아닌 것으로
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

/// GitHub API 로 최근 릴리스들을 읽어 최신 릴리스 (+ 버전마다 바뀐 것)
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

/// 릴리스 목록 (draft·prerelease 는 뺀다) → 가장 높은 버전 + 버전마다 바뀐 것
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

/// 새 버전이 있으면 Some. force 가 아니면 한 시간에 한 번만 네트워크로 묻고 그 사이엔 저장해 둔 결과를 쓴다
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

/// 업데이트 서명 공개키 (GUI tauri.conf.json 의 plugins.updater.pubkey 와 같다 — 테스트가 확인)
pub const UPDATE_PUBKEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDA0MjA1NDlDN0EyODFEM0YKUldRL0hTaDZuRlFnQklTVks5ZFNkdVBNamNqK0JJWEY0akRmR3p3azVGQ2ZiTXUwM2xiL3VtUWwK";


/// SHA256SUMS 의 서명(릴리스의 SHA256SUMS.sig, tauri signer 형식 = minisign 서명 글의 base64) 확인
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

/// SHA256SUMS 에서 파일 이름의 해시
pub fn expected_sha(sums: &str, name: &str) -> Option<String> {
    sums.lines().find_map(|l| {
        let (hash, file) = l.split_once(char::is_whitespace)?;
        (file.trim().trim_start_matches('*') == name).then(|| hash.trim().to_lowercase())
    })
}

/// 파일의 SHA-256 (16진)
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

    /// 릴리스 노트에서 바뀐 것만 (GitHub 자동 노트의 What's Changed: 제목 (#PR))
    #[test]
    fn changes_from_release_notes() {
        let body = "## dbtool 0.1.12 (macOS)\n\n### Install\n* not this\n\n## What's Changed\n* [fix] ; 없는 문장 나누기 by @Hkhyyy in https://github.com/Hkhyyy/dbtool/pull/17\n* [feat] 다른 것 by @someone in https://github.com/Hkhyyy/dbtool/pull/16\n\n## New Contributors\n* @x made their first contribution\n\n**Full Changelog**: https://github.com/Hkhyyy/dbtool/compare/v0.1.11...v0.1.12";
        assert_eq!(changes(body), ["[fix] ; 없는 문장 나누기 (#17)", "[feat] 다른 것 (#16)"]);
        assert!(changes("no section").is_empty());
        // PR 제목의 터미널 제어문자는 지운다 (--update 가 그대로 찍는다)
        assert_eq!(changes("## What's Changed\n* [fix] a\x1b]0;x\x07b by @u in https://github.com/Hkhyyy/qurry/pull/9"), ["[fix] a]0;xb (#9)"]);
    }

    /// 릴리스 목록 → 최신 + 버전마다 바뀐 것, 지금 버전보다 새 것만 (최신부터)
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

    /// TUI 창·--update 에 찍는 글: 버전마다 바뀐 것
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
        // 네트워크 없이 저장된 결과로 답한다
        assert_eq!(check(&dir, "0.1.0", false).await.unwrap(), Some(rel));
        assert_eq!(check(&dir, "9.9.9", false).await.unwrap(), None);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

