//! Qurry 엔진. GUI(Tauri) · TUI · MCP 서버가 모두 이 crate 의 `app::AppState` 를 쓴다.
/// 진단 로그 끄기 (TUI 는 화면을 쓰므로 stderr 로그를 막는다)
pub static QUIET: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[macro_export]
macro_rules! log {
    ($($t:tt)*) => {
        if !$crate::QUIET.load(std::sync::atomic::Ordering::Relaxed) {
            eprintln!($($t)*);
        }
    };
}

pub mod app;
pub mod audit;
pub mod backup;
pub mod db;
pub mod dba;
pub mod ddl;
pub mod erd;
pub mod error;
pub mod fake;
pub mod guard;
pub mod hints;
pub mod history;
pub mod meta;
pub mod plugin;
pub mod migrate;
pub mod mysql;
pub mod plan;
pub mod rag;
pub mod redisdb;
pub mod search_cache;
pub mod secrets;
pub mod session;
pub mod sql;
pub mod store;
pub mod subset;
pub mod transfer;
pub mod tunnel;
pub mod update;

/// Tauri 앱과 같은 설정 폴더 (Data Source 목록·암호화된 비밀번호 공유)
/// macOS: ~/Library/Application Support/com.hyunkyo.dbtool
pub const APP_IDENTIFIER: &str = "com.hyunkyo.dbtool";

pub fn config_dir() -> std::path::PathBuf {
    dirs::config_dir().unwrap_or_else(|| std::path::PathBuf::from(".")).join(APP_IDENTIFIER)
}

/// MCP export_csv 가 쓰는 폴더 (~/Downloads/qurry-exports). 남이 정한 경로에 파일을 쓰지 않도록 여기로만 쓴다
pub fn export_dir() -> std::path::PathBuf {
    dirs::download_dir().or_else(dirs::home_dir).unwrap_or_else(|| std::path::PathBuf::from(".")).join("qurry-exports")
}

/// export_dir 안의 새 파일 경로. 이름에 경로가 들어가면 거부하고, 같은 이름이 있으면 덮어쓰지 않고 " (2)" 를 붙인다
pub fn export_path(name: &str) -> error::AppResult<std::path::PathBuf> {
    export_path_in(&export_dir(), name)
}

/// dir 안에 새 파일을 만들어 그 경로를 돌려준다 (있는지 보고 만드는 사이에 끼어들 틈이 없게 create_new 로 바로 만든다)
fn export_path_in(dir: &std::path::Path, name: &str) -> error::AppResult<std::path::PathBuf> {
    let name = name.trim();
    if name.is_empty() || name.starts_with('.') || name.contains(['/', '\\', '\0']) || name.contains("..") {
        return Err(error::AppError::Msg("file_name must be a plain file name like report.csv (no folders)".into()));
    }
    let name = if name.to_lowercase().ends_with(".csv") { name.to_string() } else { format!("{name}.csv") };
    std::fs::create_dir_all(dir)?;
    let stem = name.trim_end_matches(".csv").trim_end_matches(".CSV").to_string();
    let mut path = dir.join(&name);
    let mut n = 2;
    loop {
        // create_new 는 이미 있으면(심볼릭 링크 포함) 실패한다
        match std::fs::OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(_) => return Ok(path),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                path = dir.join(format!("{stem} ({n}).csv"));
                n += 1;
            }
            Err(e) => return Err(e.into()),
        }
    }
}

/// 설정 폴더의 Data Source·비밀번호로 앱 상태를 연다
pub fn open_state(dir: Option<std::path::PathBuf>) -> error::AppResult<app::AppState> {
    let dir = dir.unwrap_or_else(config_dir);
    std::fs::create_dir_all(&dir)?;
    let secrets = secrets::SecretFile::open(&dir)?;
    Ok(app::AppState::new(store::Store::new(dir), secrets))
}

#[cfg(test)]
mod tests {
    /// 같은 이름을 연달아 받아도 (아직 쓰기 전이어도) 서로 다른 새 파일 — 이름을 고를 때 바로 만든다
    #[test]
    fn export_path_claims_a_new_file() {
        let dir = std::env::temp_dir().join(format!("qurry-exp-{}", uuid::Uuid::new_v4()));
        let a = super::export_path_in(&dir, "report").unwrap();
        let b = super::export_path_in(&dir, "report.csv").unwrap();
        assert_ne!(a, b);
        assert!(a.exists() && b.exists());
        assert!(b.ends_with("report (2).csv"));
        assert!(super::export_path_in(&dir, "../x").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
