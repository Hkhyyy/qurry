//! Qurry engine. The GUI (Tauri) · TUI · MCP server all use this crate's `app::AppState`.
/// Turn off diagnostic logs (the TUI owns the screen, so stderr logs are suppressed)
pub static QUIET: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// MCP server: open drivers so they can't touch anything outside the server (local files·URLs·extension installs) (DuckDB enable_external_access = false)
pub static SANDBOX: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

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
pub mod workspaces;

/// Same config folder as the Tauri app (shares the Data Source list·encrypted passwords)
/// macOS: ~/Library/Application Support/com.hyunkyo.dbtool
pub const APP_IDENTIFIER: &str = "com.hyunkyo.dbtool";

pub fn config_dir() -> std::path::PathBuf {
    dirs::config_dir().unwrap_or_else(|| std::path::PathBuf::from(".")).join(APP_IDENTIFIER)
}

/// Folder used by MCP export_csv (~/Downloads/qurry-exports). Files are written only here, never to a path chosen by someone else
pub fn export_dir() -> std::path::PathBuf {
    dirs::download_dir().or_else(dirs::home_dir).unwrap_or_else(|| std::path::PathBuf::from(".")).join("qurry-exports")
}

/// New file path inside export_dir. Names containing a path are rejected; an existing name gets " (2)" instead of being overwritten
pub fn export_path(name: &str) -> error::AppResult<std::path::PathBuf> {
    export_path_in(&export_dir(), name)
}

/// Create a new file in dir and return its path (created directly with create_new, so nothing can slip in between checking and creating)
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
        // create_new fails if it already exists (including symlinks)
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

/// Open the app state with the config folder's Data Sources·passwords
pub fn open_state(dir: Option<std::path::PathBuf>) -> error::AppResult<app::AppState> {
    let dir = dir.unwrap_or_else(config_dir);
    std::fs::create_dir_all(&dir)?;
    let secrets = secrets::SecretFile::open(&dir)?;
    Ok(app::AppState::new(store::Store::new(dir), secrets))
}

#[cfg(test)]
mod tests {
    /// Requesting the same name repeatedly (even before writing) gives distinct new files — the file is created when the name is picked
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
