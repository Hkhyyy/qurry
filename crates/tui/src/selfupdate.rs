//! `qurry-tui --update`: 최신 릴리스의 CLI 묶음(qurry-tui + qurry-mcp)을 받아 지금 실행 파일 자리에 바꿔 넣는다.
//! 소스에서 빌드한 실행 파일(…/target/release 등)은 건드리지 않고 다시 빌드하라고 안내한다.
//! 받는 파일은 이 레포의 릴리스 주소인지 확인하고, HTTPS 로만 받고, 릴리스의 SHA256SUMS 와 대조한 뒤에야 바꿔 넣는다
use qurry_core::update;
use qurry_core::update::{expected_sha, sha256_of, verify_sums};
#[cfg(test)]
use qurry_core::update::UPDATE_PUBKEY;
use std::path::{Path, PathBuf};

/// 이 플랫폼이 받을 릴리스 파일 이름의 끝 (릴리스는 Apple Silicon · Intel macOS, x86_64 Linux 를 빌드한다)
fn asset_suffix(os: &str, arch: &str) -> Option<&'static str> {
    match (os, arch) {
        ("macos", "aarch64") => Some("aarch64-apple-darwin.tar.gz"),
        ("macos", "x86_64") => Some("x86_64-apple-darwin.tar.gz"),
        ("linux", "x86_64") => Some("x86_64-unknown-linux-gnu.tar.gz"),
        _ => None,
    }
}

/// cargo 로 빌드한 실행 파일인지 (…/target/release/qurry-tui, …/target/debug/…)
pub fn from_source(exe: &Path) -> bool {
    let parts: Vec<String> = exe.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect();
    parts.windows(2).any(|w| w[0] == "target" && (w[1] == "release" || w[1] == "debug"))
}

pub fn current_exe() -> Option<PathBuf> {
    std::env::current_exe().ok().and_then(|p| p.canonicalize().ok())
}

fn err(e: impl std::fmt::Display) -> std::io::Error {
    std::io::Error::other(e.to_string())
}

/// 이 레포 릴리스에서 받는 파일만 (update-check.json 이나 API 응답이 바뀌어도 다른 곳에서 받지 않게)
fn trusted_url(url: &str) -> bool {
    // 지금 저장소 + 이름을 바꾸기 전 주소 (GitHub 이 새 주소로 돌려준다)
    [update::REPO, "Hkhyyy/dbtool"].iter().any(|r| url.starts_with(&format!("https://github.com/{r}/releases/download/")))
}

/// curl: HTTPS 만 (리다이렉트 포함). quiet = TUI 화면 안에서 (진행 막대를 찍지 않는다)
fn download(url: &str, to: &Path, quiet: bool) -> std::io::Result<()> {
    let mut cmd = std::process::Command::new("curl");
    cmd.args(["-fL", "--proto", "=https", "--proto-redir", "=https", "--tlsv1.2", if quiet { "-sS" } else { "--progress-bar" }, "-o"]).arg(to).arg(url);
    if quiet { run_quiet(&mut cmd) } else { run(&mut cmd) }
}



fn run(cmd: &mut std::process::Command) -> std::io::Result<()> {
    let st = cmd.status()?;
    if st.success() { Ok(()) } else { Err(err(format!("{cmd:?} failed ({st})"))) }
}

/// 출력을 화면에 내지 않고 실행한다 (실패하면 stderr 를 오류로)
fn run_quiet(cmd: &mut std::process::Command) -> std::io::Result<()> {
    let out = cmd.stdin(std::process::Stdio::null()).output()?;
    if out.status.success() { Ok(()) } else { Err(err(format!("{} failed: {}", cmd.get_program().to_string_lossy(), String::from_utf8_lossy(&out.stderr).trim()))) }
}

/// 업데이트 결과
pub enum Outcome {
    UpToDate,
    /// 바꿔 넣지 않은 이유 (소스 빌드, 이 플랫폼용 파일 없음 …)
    Skipped(String),
    Installed { version: String, paths: Vec<PathBuf> },
}

/// 소스에서 빌드했거나 Homebrew 로 깔았으면 그 안내 (바꿔 넣지 않는다)
fn source_build_note(exe: &Path) -> Option<String> {
    // Homebrew (…/Cellar/qurry/<v>/bin) 는 brew 가 관리한다
    if exe.components().any(|c| c.as_os_str() == "Cellar") {
        return Some("This qurry-tui was installed with Homebrew. Update it with: brew upgrade qurry".into());
    }
    from_source(exe).then(|| format!("This qurry-tui was built from source ({}). Update it with: git pull && cargo build --release -p qurry-tui -p qurry-mcp", exe.display()))
}

/// 최신 릴리스를 받아 지금 실행 파일 자리에 바꿔 넣는다. quiet = TUI 화면 안에서 (아무것도 찍지 않는다)
pub async fn install(quiet: bool) -> std::io::Result<Outcome> {
    let cur = env!("CARGO_PKG_VERSION");
    let exe = current_exe().ok_or_else(|| err("cannot find this executable"))?;
    if let Some(note) = source_build_note(&exe) {
        return Ok(Outcome::Skipped(note));
    }
    let rel = update::latest_release().await.map_err(err)?;
    if !update::is_newer(cur, &rel.version) {
        return Ok(Outcome::UpToDate);
    }
    // 묶음 이름: qurry-cli_… (이름을 바꾸기 전 릴리스는 dbtool-cli_…)
    let Some(asset) = asset_suffix(std::env::consts::OS, std::env::consts::ARCH).and_then(|s| rel.asset("qurry-cli_", s).or_else(|| rel.asset("dbtool-cli_", s))).cloned() else {
        return Ok(Outcome::Skipped(format!("Qurry {} is available, but there is no prebuilt binary for this platform. See {}", rel.version, rel.url)));
    };
    let Some(sums) = rel.assets.iter().find(|a| a.name == "SHA256SUMS").cloned() else {
        return Ok(Outcome::Skipped(format!("Qurry {} has no SHA256SUMS file, so it can't be verified. Download it yourself from {}", rel.version, rel.url)));
    };
    let Some(sums_sig) = rel.assets.iter().find(|a| a.name == "SHA256SUMS.sig").cloned() else {
        return Ok(Outcome::Skipped(format!("Qurry {} has no signed SHA256SUMS, so it can't be verified. Download it yourself from {}", rel.version, rel.url)));
    };
    if !trusted_url(&asset.url) || !trusted_url(&sums.url) || !trusted_url(&sums_sig.url) {
        return Err(err(format!("refusing to download from an unexpected address: {}", asset.url)));
    }
    if !quiet {
        print!("{}", update::changes_text(&rel.changes_since(cur)).chars().filter(|c| *c == '\n' || !crate::ui::is_unsafe_char(*c)).collect::<String>());
        println!("Updating qurry-tui {cur} → {} …", rel.version);
    }
    let paths = tokio::task::spawn_blocking(move || replace(&exe, &asset, &sums, &sums_sig, quiet)).await.map_err(err)??;
    Ok(Outcome::Installed { version: rel.version, paths })
}



/// 설치할 이름: 새 이름은 늘, 예전 이름(dbtool-*)은 이미 있을 때만
fn install_names(exists: impl Fn(&str) -> bool) -> Vec<&'static str> {
    let mut v = vec!["qurry-tui", "qurry-mcp"];
    v.extend(["dbtool-tui", "dbtool-mcp"].into_iter().filter(|n| exists(n)));
    v
}

/// 받아서 검증하고 exe 와 같은 폴더의 qurry-tui·qurry-mcp 를 바꿔 넣는다
fn replace(exe: &Path, asset: &update::Asset, sums: &update::Asset, sums_sig: &update::Asset, quiet: bool) -> std::io::Result<Vec<PathBuf>> {
    // 임시 폴더는 새로 만든다 (이미 있으면 남이 만든 것일 수 있어 쓰지 않는다)
    let tmp = std::env::temp_dir().join(format!("dbtool-update-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0)));
    std::fs::create_dir(&tmp)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o700))?;
    }
    let tgz = tmp.join("cli.tar.gz");
    let result = (|| -> std::io::Result<Vec<PathBuf>> {
        let sums_file = tmp.join("SHA256SUMS");
        download(&sums.url, &sums_file, quiet)?;
        // 체크섬 목록이 업데이트 키로 서명됐는지 먼저 (GUI 업데이트와 같은 키)
        let sig_file = tmp.join("SHA256SUMS.sig");
        download(&sums_sig.url, &sig_file, quiet)?;
        verify_sums(&std::fs::read(&sums_file)?, &std::fs::read_to_string(&sig_file)?).map_err(|e| err(format!("SHA256SUMS {e} — not installed")))?;
        download(&asset.url, &tgz, quiet)?;
        // 받은 파일이 릴리스에 올라간 것과 같은지
        let want = expected_sha(&std::fs::read_to_string(&sums_file)?, &asset.name).ok_or_else(|| err(format!("{} is not listed in SHA256SUMS", asset.name)))?;
        let got = sha256_of(&tgz)?;
        if want != got {
            return Err(err(format!("checksum mismatch for {} (expected {want}, got {got}) — not installed", asset.name)));
        }
        if !quiet {
            println!("  checksum OK ({got})");
        }
        run_quiet(std::process::Command::new("tar").arg("-xzf").arg(&tgz).arg("-C").arg(&tmp))?;
        // 지금 실행 파일과 같은 폴더에 바꿔 넣는다 (임시 이름으로 복사 → rename 이라 도중에 깨지지 않는다)
        let dir = exe.parent().ok_or_else(|| err("no install folder"))?;
        let mut done = vec![];
        for name in install_names(|n| dir.join(n).exists()) {
            // 예전 이름도 새 바이너리로 (묶음 안에는 qurry-* 가 있다)
            let src = tmp.join(name.replace("dbtool-", "qurry-"));
            if !src.exists() {
                continue;
            }
            let staged = dir.join(format!(".{name}.new"));
            std::fs::copy(&src, &staged)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&staged, std::fs::Permissions::from_mode(0o755))?;
            }
            let dst = dir.join(name);
            std::fs::rename(&staged, &dst)?;
            done.push(dst);
        }
        Ok(done)
    })();
    let _ = std::fs::remove_dir_all(&tmp);
    result
}

pub async fn run_update() -> std::io::Result<()> {
    match install(false).await? {
        Outcome::UpToDate => println!("qurry-tui {} is up to date.", env!("CARGO_PKG_VERSION")),
        Outcome::Skipped(why) => println!("{why}"),
        Outcome::Installed { version, paths } => {
            for p in &paths {
                println!("  updated {}", p.display());
            }
            println!("Qurry {version} installed. Restart qurry-tui (and your MCP client, if it runs qurry-mcp).");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::from_source;
    use std::path::Path;

    #[test]
    fn checksum_lines_and_urls() {
        let sums = "abc123  dbtool-cli_0.1.3_aarch64-apple-darwin.tar.gz\nDEF456 *dbtool_0.1.3_aarch64.dmg\n";
        assert_eq!(super::expected_sha(sums, "dbtool-cli_0.1.3_aarch64-apple-darwin.tar.gz").as_deref(), Some("abc123"));
        assert_eq!(super::expected_sha(sums, "dbtool_0.1.3_aarch64.dmg").as_deref(), Some("def456"));
        assert!(super::expected_sha(sums, "other").is_none());
        assert!(super::trusted_url("https://github.com/Hkhyyy/dbtool/releases/download/v0.1.3/x.tar.gz"));
        // 저장소 이름을 Qurry 로 바꿔도 (GitHub 이 새 주소로 돌려준다)
        assert!(super::trusted_url("https://github.com/Hkhyyy/qurry/releases/download/v0.2.0/x.tar.gz"));
        assert!(!super::trusted_url("https://github.com/someone/qurry/releases/download/v0.2.0/x.tar.gz"));
        assert!(!super::trusted_url("https://evil.example/x.tar.gz"));
        assert!(!super::trusted_url("http://github.com/Hkhyyy/dbtool/releases/download/v0.1.3/x"));
    }

    /// 이름이 바뀐 뒤: qurry-* 는 늘 설치, 예전 이름 dbtool-* 은 그 폴더에 이미 있을 때만 (MCP 설정 등이 그 이름을 부른다)
    #[test]
    fn install_names_keep_old_aliases() {
        let has = |names: &'static [&'static str]| move |n: &str| names.contains(&n);
        assert_eq!(super::install_names(has(&[])), ["qurry-tui", "qurry-mcp"]);
        assert_eq!(super::install_names(has(&["dbtool-tui", "dbtool-mcp"])), ["qurry-tui", "qurry-mcp", "dbtool-tui", "dbtool-mcp"]);
        assert_eq!(super::install_names(has(&["dbtool-mcp"])), ["qurry-tui", "qurry-mcp", "dbtool-mcp"]);
    }

    /// SHA256SUMS 는 GUI 업데이트와 같은 키로 서명돼 있어야 한다 (릴리스·계정이 털려도 체크섬만 바꿔서는 못 넘어온다)
    #[test]
    fn sums_must_be_signed_with_the_update_key() {
        // 실제 키로 "hello\n" 에 서명한 것
        const SIG: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IHNpZ25hdHVyZSBmcm9tIHRhdXJpIHNlY3JldCBrZXkKUlVRL0hTaDZuRlFnQkFMUWNzY1hnK3BORU8rU2hESWFGWnJOb0hEZTZBT1lUVFc1QmNjWEs5TVdaYnRhSllBbXVDUDJuYUR6azJCd1dLd0JaU3V3akJMRExwQiswdkhyZVFBPQp0cnVzdGVkIGNvbW1lbnQ6IHRpbWVzdGFtcDoxNzkyNzIyOTE5CWZpbGU6U0hBMjU2U1VNUwpCbytybzN4VVNOeXJlVFRHZXFGRHZidGJBV00zU1JKMk1SRXRJZVhCcEZIbS9TNUVuWm5FZVhKeGh0RG15VFNQVDFtTFUzdXpTd3VFelhVUlg2VldCUT09Cg==";
        assert!(super::verify_sums(b"hello\n", SIG).is_ok());
        assert!(super::verify_sums(b"hello!\n", SIG).is_err());
        assert!(super::verify_sums(b"hello\n", "bm90IGEgc2lnbmF0dXJl").is_err());
        // GUI(tauri.conf.json) 와 같은 공개키
        let conf: serde_json::Value = serde_json::from_str(include_str!("../../../src-tauri/tauri.conf.json")).unwrap();
        assert_eq!(conf["plugins"]["updater"]["pubkey"].as_str(), Some(super::UPDATE_PUBKEY));
    }

    /// 릴리스는 Apple Silicon · Intel macOS 와 x86_64 Linux 묶음을 올린다
    #[test]
    fn asset_suffix_per_platform() {
        assert_eq!(super::asset_suffix("macos", "aarch64"), Some("aarch64-apple-darwin.tar.gz"));
        assert_eq!(super::asset_suffix("macos", "x86_64"), Some("x86_64-apple-darwin.tar.gz"));
        assert_eq!(super::asset_suffix("linux", "x86_64"), Some("x86_64-unknown-linux-gnu.tar.gz"));
        assert_eq!(super::asset_suffix("windows", "x86_64"), None);
    }

    /// 체크섬은 직접 계산한다 (shasum 이 없는 Linux 도 있다)
    #[test]
    fn sha256_of_file() {
        let f = std::env::temp_dir().join(format!("qurry-sha-test-{}", std::process::id()));
        std::fs::write(&f, b"abc").unwrap();
        let got = super::sha256_of(&f).unwrap();
        let _ = std::fs::remove_file(&f);
        assert_eq!(got, "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }

    /// Homebrew 로 깐 것은 brew 가 관리한다 (바꿔 넣지 않고 brew upgrade 를 안내)
    #[test]
    fn homebrew_installs_are_left_to_brew() {
        let note = super::source_build_note(Path::new("/opt/homebrew/Cellar/qurry/0.1.19/bin/qurry-tui")).unwrap();
        assert!(note.contains("brew upgrade qurry"), "{note}");
        assert!(super::source_build_note(Path::new("/home/linuxbrew/.linuxbrew/Cellar/qurry/0.1.19/bin/qurry-tui")).is_some());
        assert!(super::source_build_note(Path::new("/Users/me/.local/bin/qurry-tui")).is_none());
    }

    #[test]
    fn detects_cargo_builds() {
        assert!(from_source(Path::new("/Users/me/dbtool/target/release/qurry-tui")));
        assert!(from_source(Path::new("/x/target/debug/qurry-tui")));
        assert!(!from_source(Path::new("/Users/me/.local/bin/qurry-tui")));
        assert!(!from_source(Path::new("/opt/target-tools/release/qurry-tui")));
    }
}
