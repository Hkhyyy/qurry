//! In-app update on macOS: keep the Qurry.app folder and swap only the Contents inside.
//! Replacing the whole folder (Tauri updater default) makes it a different file even at the same path, so the icon pinned in the Dock keeps pointing at the old app
//! and the relaunched app shows up as one more Dock icon.
use std::io::{Error, ErrorKind, Result};
use std::path::{Component, Path, PathBuf};

/// App folder containing the executable (…/Qurry.app/Contents/MacOS/qurry). None when running outside an app bundle (cargo build)
pub fn app_bundle(exe: &Path) -> Option<PathBuf> {
    let macos = exe.parent()?;
    let contents = macos.parent()?;
    let app = contents.parent()?;
    (macos.file_name()? == "MacOS" && contents.file_name()? == "Contents" && app.extension()? == "app").then(|| app.to_path_buf())
}

/// Swap the Contents of the update archive (tar.gz, Qurry.app/ at the top) with app's Contents.
/// The caller (updater's download) has already verified the signature. On failure the current app is left as is
pub fn install_contents(app: &Path, package: &[u8]) -> Result<()> {
    // The swap is atomic only within the same file system: keep the work folder inside the app
    let work = app.join(format!(".update-{}", std::process::id()));
    if work.exists() {
        std::fs::remove_dir_all(&work)?;
    }
    std::fs::create_dir(&work)?;
    let result = (|| {
        let new = unpack(package, &work)?;
        let current = app.join("Contents");
        if let Err(e) = swap(&new, &current) {
            if e.kind() == ErrorKind::PermissionDenied {
                return Err(e);
            }
            // File systems without swap (RENAME_SWAP): move aside, put in, and roll back on failure
            let old = work.join("old-Contents");
            std::fs::rename(&current, &old)?;
            if let Err(e) = std::fs::rename(&new, &current) {
                let _ = std::fs::rename(&old, &current);
                return Err(e);
            }
        }
        Ok(())
    })();
    // After the swap the old Contents is in the work folder
    let _ = std::fs::remove_dir_all(&work);
    if result.is_ok() {
        // Make Finder·LaunchServices re-read the changed app
        let _ = std::fs::File::open(app).and_then(|f| f.set_modified(std::time::SystemTime::now()));
    }
    result
}

/// Unpack the archive under work and return the new Contents path. Every entry must be <app>.app/Contents/…
fn unpack(package: &[u8], work: &Path) -> Result<PathBuf> {
    let bad = |p: &Path| Error::new(ErrorKind::InvalidData, format!("unexpected path in the update: {}", p.display()));
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(package));
    let mut top: Option<std::ffi::OsString> = None;
    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.into_owned();
        let comps: Vec<Component> = path.components().collect();
        let ok = comps.iter().all(|c| matches!(c, Component::Normal(_)))
            && match comps.as_slice() {
                [Component::Normal(t)] => top.get_or_insert_with(|| t.to_os_string()) == t,
                [Component::Normal(t), Component::Normal(c), ..] => top.get_or_insert_with(|| t.to_os_string()) == t && *c == "Contents",
                _ => false,
            };
        // unpack_in also skips entries escaping work (including via links)
        if !ok || !entry.unpack_in(work)? {
            return Err(bad(&path));
        }
    }
    let new = work.join(top.ok_or_else(|| bad(Path::new("")))?).join("Contents");
    if !new.is_dir() {
        return Err(bad(&new));
    }
    Ok(new)
}

/// Atomically swap two paths (APFS renamex_np RENAME_SWAP)
fn swap(a: &Path, b: &Path) -> Result<()> {
    use std::os::unix::ffi::OsStrExt;
    let a = std::ffi::CString::new(a.as_os_str().as_bytes())?;
    let b = std::ffi::CString::new(b.as_os_str().as_bytes())?;
    // SAFETY: both are NUL-terminated strings alive for the call, and renamex_np only reads them
    if unsafe { libc::renamex_np(a.as_ptr(), b.as_ptr(), libc::RENAME_SWAP) } == 0 { Ok(()) } else { Err(Error::last_os_error()) }
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::os::unix::fs::MetadataExt;

    /// Same shape as the update archive (Qurry.app/ at the top)
    fn tar_gz(files: &[(&str, &str)]) -> Vec<u8> {
        let mut b = tar::Builder::new(flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast()));
        for (path, body) in files {
            let mut h = tar::Header::new_gnu();
            h.set_size(body.len() as u64);
            h.set_mode(0o755);
            h.set_entry_type(tar::EntryType::Regular);
            // set_path rejects "..", so write the name field directly (mimicking a bad archive)
            h.as_old_mut().name[..path.len()].copy_from_slice(path.as_bytes());
            h.set_cksum();
            b.append(&h, body.as_bytes()).unwrap();
        }
        let mut gz = b.into_inner().unwrap();
        gz.flush().unwrap();
        gz.finish().unwrap()
    }

    fn app_with(dir: &std::path::Path, version: &str) -> std::path::PathBuf {
        let app = dir.join("Qurry.app");
        std::fs::create_dir_all(app.join("Contents/MacOS")).unwrap();
        std::fs::write(app.join("Contents/Info.plist"), version).unwrap();
        std::fs::write(app.join("Contents/MacOS/qurry"), version).unwrap();
        app
    }

    /// Keep the Dock-pinned icon pointing at the same app: the Qurry.app folder stays the same file (inode), only the contents are the new version
    #[test]
    fn replaces_contents_but_keeps_the_app_folder() {
        let dir = tempfile::tempdir().unwrap();
        let app = app_with(dir.path(), "old");
        let inode = std::fs::metadata(&app).unwrap().ino();
        let pkg = tar_gz(&[("Qurry.app/Contents/Info.plist", "new"), ("Qurry.app/Contents/MacOS/qurry", "new-bin")]);

        super::install_contents(&app, &pkg).unwrap();

        assert_eq!(std::fs::metadata(&app).unwrap().ino(), inode);
        assert_eq!(std::fs::read_to_string(app.join("Contents/Info.plist")).unwrap(), "new");
        assert_eq!(std::fs::read_to_string(app.join("Contents/MacOS/qurry")).unwrap(), "new-bin");
        // No old contents or work folder are left behind
        let left: Vec<_> = std::fs::read_dir(&app).unwrap().map(|e| e.unwrap().file_name()).collect();
        assert_eq!(left, ["Contents"]);
    }

    /// Archives with paths escaping the bundle or without Contents are rejected, and the current app is left as is
    #[test]
    fn rejects_bad_packages_and_keeps_the_app() {
        let dir = tempfile::tempdir().unwrap();
        let app = app_with(dir.path(), "old");
        for pkg in [
            tar_gz(&[("Qurry.app/Contents/Info.plist", "new"), ("Qurry.app/../../evil", "x")]),
            tar_gz(&[("Qurry.app/Other/file", "x")]),
        ] {
            assert!(super::install_contents(&app, &pkg).is_err());
            assert_eq!(std::fs::read_to_string(app.join("Contents/Info.plist")).unwrap(), "old");
            assert!(!dir.path().join("evil").exists());
            let left: Vec<_> = std::fs::read_dir(&app).unwrap().map(|e| e.unwrap().file_name()).collect();
            assert_eq!(left, ["Contents"]);
        }
    }

    #[test]
    fn app_folder_of_the_executable() {
        assert_eq!(super::app_bundle(std::path::Path::new("/Applications/Qurry.app/Contents/MacOS/qurry")).unwrap(), std::path::Path::new("/Applications/Qurry.app"));
        assert!(super::app_bundle(std::path::Path::new("/Users/me/dbtool/target/release/qurry")).is_none());
    }
}
