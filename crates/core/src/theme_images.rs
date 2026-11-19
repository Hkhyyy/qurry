//! The custom themes' image files (#202): copies in the config folder's `theme-images`, named by their content's hash
//! (`<16 hex>.<ext>`), the only names read or deleted here. Which files a theme may take is qurry_shared::theme (check_image,
//! image_name_ok); the theme stores the name only. Errors name the file, never a full path.
use crate::error::{AppError, AppResult};
use qurry_shared::theme;
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// The folder in the config folder
pub const DIR: &str = "theme-images";
/// A file this new is never pruned: it may be in a theme being edited, not saved yet
const GRACE: Duration = Duration::from_secs(60 * 60);

/// Copy the picked image `src` into `dir`, checked by its first bytes and size; its file name. The same image picked twice is
/// one file
pub fn import(dir: &Path, src: &Path) -> AppResult<String> {
    let shown = src.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "The file".into());
    let mut bytes = Vec::new();
    std::fs::File::open(src)
        .and_then(|f| f.take(theme::ANIMATED_MAX as u64 + 1).read_to_end(&mut bytes))
        .map_err(|e| AppError::Msg(format!("Couldn't read {shown} ({})", e.kind())))?;
    let ext = theme::check_image(&bytes, &shown).map_err(AppError::Msg)?;
    let hash = Sha256::digest(&bytes);
    let name = format!("{}.{ext}", hash[..8].iter().map(|b| format!("{b:02x}")).collect::<String>());
    std::fs::create_dir_all(dir).map_err(|e| AppError::Msg(format!("Couldn't create the theme images folder ({})", e.kind())))?;
    let path = dir.join(&name);
    // create_new: an existing file with this name is this image already
    match std::fs::OpenOptions::new().write(true).create_new(true).open(&path) {
        Ok(mut f) => {
            if let Err(e) = f.write_all(&bytes) {
                let _ = std::fs::remove_file(&path);
                return Err(AppError::Msg(format!("Couldn't copy {shown} ({})", e.kind())));
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(AppError::Msg(format!("Couldn't copy {shown} ({})", e.kind()))),
    }
    Ok(name)
}

/// A theme image's path, after checking `name` is one `import` gives (so no other path is read or deleted)
fn path_of(dir: &Path, name: &str) -> AppResult<PathBuf> {
    if !theme::image_name_ok(name) {
        return Err(AppError::Msg(format!("\"{name}\" isn't a theme image's file name")));
    }
    Ok(dir.join(name))
}

/// A theme image's bytes, for the GUI to draw
pub fn read(dir: &Path, name: &str) -> AppResult<Vec<u8>> {
    std::fs::read(path_of(dir, name)?).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => AppError::Msg(format!("The theme image {name} is missing: choose it again")),
        k => AppError::Msg(format!("Couldn't read the theme image {name} ({k})")),
    })
}

/// Delete the images no theme uses (`used`), except ones copied in the last hour; how many were deleted. Files with other names
/// are left alone
pub fn prune<'a>(dir: &Path, used: impl IntoIterator<Item = &'a str>) -> AppResult<usize> {
    let used: Vec<&str> = used.into_iter().collect();
    let Ok(entries) = std::fs::read_dir(dir) else { return Ok(0) };
    let now = SystemTime::now();
    let mut gone = 0;
    for e in entries.flatten() {
        let Ok(name) = e.file_name().into_string() else { continue };
        let Ok(meta) = e.metadata() else { continue };
        let fresh = meta.modified().ok().and_then(|m| now.duration_since(m).ok()).is_none_or(|age| age < GRACE);
        if !meta.is_file() || !theme::image_name_ok(&name) || used.contains(&name.as_str()) || fresh {
            continue;
        }
        if std::fs::remove_file(path_of(dir, &name)?).is_ok() {
            gone += 1;
        }
    }
    Ok(gone)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n-a-tiny-png";

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("qurry-theme-images-{tag}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }
    fn picked(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
        let p = dir.join(name);
        std::fs::write(&p, bytes).unwrap();
        p
    }
    fn age(path: &Path, secs: u64) {
        std::fs::File::options().write(true).open(path).unwrap().set_modified(SystemTime::now() - Duration::from_secs(secs)).unwrap();
    }

    #[test]
    fn an_image_is_copied_under_its_hash_once() {
        let (src, dir) = (tmp("src"), tmp("dir"));
        let name = import(&dir, &picked(&src, "Cat Picture.png", PNG)).unwrap();
        assert!(theme::image_name_ok(&name) && name.ends_with(".png"), "{name}");
        assert_eq!(read(&dir, &name).unwrap(), PNG);
        // The same bytes under another name: the same file
        assert_eq!(import(&dir, &picked(&src, "again.png", PNG)).unwrap(), name);
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
    }

    #[test]
    fn svg_other_files_and_oversized_images_are_refused_without_their_path() {
        let (src, dir) = (tmp("src"), tmp("dir"));
        let e = import(&dir, &picked(&src, "logo.svg", br#"<svg xmlns="http://www.w3.org/2000/svg"><script>x</script></svg>"#)).unwrap_err().to_string();
        assert_eq!(e, "logo.svg isn't a PNG, JPEG, GIF or WebP image");
        let e = import(&dir, &picked(&src, "fake.png", b"not an image")).unwrap_err().to_string();
        assert!(e.contains("isn't a PNG") && !e.contains(src.to_str().unwrap()), "{e}");
        let mut big = PNG.to_vec();
        big.resize(theme::IMAGE_MAX + 1, 0);
        let e = import(&dir, &picked(&src, "big.png", &big)).unwrap_err().to_string();
        assert!(e.starts_with("big.png is 4.0 MB: a still image can be at most 4 MB"), "{e}");
        // Larger than the animated cap: read no further than it
        big.resize(theme::ANIMATED_MAX + 100, 0);
        assert!(import(&dir, &picked(&src, "huge.png", &big)).is_err());
        let e = import(&dir, &src.join("gone.png")).unwrap_err().to_string();
        assert!(e.starts_with("Couldn't read gone.png") && !e.contains(src.to_str().unwrap()), "{e}");
        assert_eq!(std::fs::read_dir(&dir).map(|d| d.count()).unwrap_or(0), 0, "nothing copied");
    }

    #[test]
    fn only_a_plain_image_name_is_read_or_deleted() {
        let dir = tmp("dir");
        let outside = dir.parent().unwrap().join(format!("{}.png", "0".repeat(16)));
        for bad in ["../secret.png", "/etc/passwd", "..\\x.png", "a/0123456789abcdef.png", "0123456789abcdef.svg", ".."] {
            let e = read(&dir, bad).unwrap_err().to_string();
            assert!(e.contains("isn't a theme image's file name"), "{bad}: {e}");
        }
        assert!(read(&dir, "0123456789abcdef.png").unwrap_err().to_string().contains("is missing"));
        assert!(!outside.exists());
    }

    #[test]
    fn unused_images_are_pruned_after_an_hour_and_nothing_else() {
        let (src, dir) = (tmp("src"), tmp("dir"));
        let used = import(&dir, &picked(&src, "a.png", PNG)).unwrap();
        let old = import(&dir, &picked(&src, "b.png", b"\x89PNG\r\n\x1a\nother")).unwrap();
        let new = import(&dir, &picked(&src, "c.png", b"\x89PNG\r\n\x1a\nfresh")).unwrap();
        let other = picked(&dir, "notes.txt", b"mine");
        for n in [&used, &old] {
            age(&dir.join(n), 2 * 3600);
        }
        age(&other, 2 * 3600);
        assert_eq!(prune(&dir, [used.as_str()]).unwrap(), 1);
        assert!(dir.join(&used).exists() && !dir.join(&old).exists() && dir.join(&new).exists() && other.exists());
        assert_eq!(prune(&tmp("none").join("missing"), []).unwrap(), 0);
    }
}
