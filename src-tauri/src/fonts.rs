//! Installed font families for Settings › Appearance › Editor / Grid font (a WebView can't list them).
//! macOS: CoreText. Linux: fontconfig's fc-list. Elsewhere: none (the picker then offers the default and the saved family).
//! Also the font files added there: copied into <config>/fonts and handed to the WebView as bytes (it registers them with FontFace)

use crate::error::{AppError, AppResult};
use serde::Serialize;
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use tauri::Manager;
use tauri_plugin_dialog::DialogExt;

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct FontFamily {
    pub name: String,
    /// Every face of the family is monospaced
    pub mono: bool,
}

/// One entry per family from (family, monospaced) per face: a family is monospaced only if all its faces are.
/// Hidden system families (".SF NS") and empty names are left out; sorted case-insensitively
fn families(faces: impl IntoIterator<Item = (String, bool)>) -> Vec<FontFamily> {
    let mut by_name: BTreeMap<(String, String), bool> = BTreeMap::new();
    for (name, mono) in faces {
        let name = name.trim();
        if name.is_empty() || name.starts_with('.') {
            continue;
        }
        let m = by_name.entry((name.to_lowercase(), name.to_string())).or_insert(true);
        *m &= mono;
    }
    by_name.into_iter().map(|((_, name), mono)| FontFamily { name, mono }).collect()
}

/// `fc-list --format '%{family[0]}\t%{spacing}\n'` lines: spacing 90 (dual width), 100 (mono) and 110 (charcell) are monospaced, none or 0 isn't
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn parse_fc_list(out: &str) -> Vec<FontFamily> {
    families(out.lines().map(|l| {
        let (name, spacing) = l.split_once('\t').unwrap_or((l, ""));
        (name.to_string(), spacing.trim().parse::<u32>().is_ok_and(|s| s >= 90))
    }))
}

#[cfg(target_os = "macos")]
fn list() -> Vec<FontFamily> {
    use objc2_core_foundation::{CFArray, CFDictionary, CFNumber, CFRetained, CFString, CFType};
    use objc2_core_text::{kCTFontFamilyNameAttribute, kCTFontSymbolicTrait, kCTFontTraitsAttribute, CTFontCollection, CTFontDescriptor, CTFontSymbolicTraits};
    // SAFETY: CoreText documents the descriptors array as CTFontDescriptors and the traits as a dictionary keyed by CFStrings;
    // every value read from them is checked with downcast
    unsafe {
        let Some(descs) = CTFontCollection::from_available_fonts(None).matching_font_descriptors() else {
            return Vec::new();
        };
        let descs = CFRetained::cast_unchecked::<CFArray<CTFontDescriptor>>(descs);
        let faces = descs.iter().filter_map(|d| {
            let name = d.attribute(kCTFontFamilyNameAttribute)?.downcast::<CFString>().ok()?.to_string();
            let sym = d
                .attribute(kCTFontTraitsAttribute)
                .and_then(|t| t.downcast::<CFDictionary>().ok())
                .and_then(|t| CFRetained::cast_unchecked::<CFDictionary<CFString, CFType>>(t).get(kCTFontSymbolicTrait))
                .and_then(|n| n.downcast::<CFNumber>().ok())
                .and_then(|n| n.as_i64())
                .unwrap_or(0);
            Some((name, sym as u32 & CTFontSymbolicTraits::TraitMonoSpace.bits() != 0))
        });
        families(faces.collect::<Vec<_>>())
    }
}

#[cfg(target_os = "linux")]
fn list() -> Vec<FontFamily> {
    std::process::Command::new("fc-list")
        .args(["--format", "%{family[0]}\t%{spacing}\n"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| parse_fc_list(&String::from_utf8_lossy(&o.stdout)))
        .unwrap_or_default()
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn list() -> Vec<FontFamily> {
    Vec::new()
}

/// Installed font families, sorted, each flagged monospaced. Listed afresh on each call (on a blocking thread),
/// so a font installed while the app runs shows up the next time the picker opens
#[tauri::command]
pub async fn font_families() -> Vec<FontFamily> {
    tauri::async_runtime::spawn_blocking(list).await.unwrap_or_default()
}

/// A font file added in Settings: `file` in the fonts folder, registered by the GUI as the family `name` (the file name without extension)
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct CustomFont {
    pub name: String,
    pub file: String,
}

const MAX_FONT_BYTES: u64 = 20 * 1024 * 1024;
/// Longest family name made from a picked file's name (a " 2" suffix may follow)
const MAX_NAME: usize = 64;

/// The extension a font file is kept under, from its first bytes; None if it isn't a TrueType, OpenType, WOFF or WOFF2 font
fn font_ext(head: &[u8]) -> Option<&'static str> {
    match head.get(..4)? {
        [0, 1, 0, 0] | b"true" => Some("ttf"),
        b"OTTO" => Some("otf"),
        b"wOFF" => Some("woff"),
        b"wOF2" => Some("woff2"),
        _ => None,
    }
}

/// Family (and file) name from a picked file's name: letters, digits, `-` and `_` kept, anything else (dots, slashes, quotes) a space,
/// spaces collapsed; "" if nothing is left
fn safe_name(stem: &str) -> String {
    let kept: String = stem.chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { ' ' }).collect();
    kept.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The font files in `dir`, sorted by name: only regular files (no links) named as `add_font` names them
fn custom_in(dir: &Path) -> Vec<CustomFont> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut fonts: Vec<CustomFont> = entries
        .filter_map(|e| {
            let e = e.ok()?;
            if !e.file_type().ok()?.is_file() {
                return None;
            }
            let file = e.file_name().into_string().ok()?;
            let (stem, ext) = file.rsplit_once('.')?;
            (["ttf", "otf", "woff", "woff2"].contains(&ext) && !stem.is_empty() && safe_name(stem) == stem).then(|| CustomFont { name: stem.into(), file: file.clone() })
        })
        .collect();
    fonts.sort_by_key(|f| f.name.to_lowercase());
    fonts
}

/// Copy the font file `src` into `dir`, checked to be a font (by its first bytes) of at most 20 MB. Named after `src`, made safe;
/// a name already taken (in any case or format) gets " 2", " 3"…, so no font is replaced
fn add_font(dir: &Path, src: &Path) -> Result<CustomFont, String> {
    let shown = src.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let mut bytes = Vec::new();
    std::fs::File::open(src)
        .and_then(|f| f.take(MAX_FONT_BYTES + 1).read_to_end(&mut bytes))
        .map_err(|e| format!("Couldn't read {shown}: {e}"))?;
    if bytes.len() as u64 > MAX_FONT_BYTES {
        return Err(format!("{shown} is larger than 20 MB"));
    }
    let ext = font_ext(&bytes).ok_or_else(|| format!("{shown} isn't a font file (TrueType, OpenType, WOFF or WOFF2)"))?;
    std::fs::create_dir_all(dir).map_err(|e| format!("Couldn't create the fonts folder {} ({e})", dir.display()))?;
    let base: String = safe_name(&src.file_stem().unwrap_or_default().to_string_lossy()).chars().take(MAX_NAME).collect();
    let base = match base.trim_end() {
        "" => "Font".to_string(),
        b => b.to_string(),
    };
    let taken: Vec<String> = custom_in(dir).into_iter().map(|f| f.name.to_lowercase()).collect();
    for n in 1.. {
        let name = if n == 1 { base.clone() } else { format!("{base} {n}") };
        if taken.contains(&name.to_lowercase()) {
            continue;
        }
        let file = format!("{name}.{ext}");
        let path = dir.join(&file);
        // create_new: never writes over a file that appeared meanwhile
        match std::fs::OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut f) => {
                return match f.write_all(&bytes) {
                    Ok(()) => Ok(CustomFont { name, file }),
                    Err(e) => {
                        let _ = std::fs::remove_file(&path);
                        Err(format!("Couldn't copy {shown} into {} ({e})", dir.display()))
                    }
                };
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(format!("Couldn't copy {shown} into {} ({e})", dir.display())),
        }
    }
    unreachable!()
}

/// The path of an added font file: only a name `custom_in` lists, so no other path can be read or deleted
fn added(dir: &Path, file: &str) -> Result<PathBuf, String> {
    custom_in(dir).iter().find(|f| f.file == file).map(|f| dir.join(&f.file)).ok_or_else(|| format!("No added font file named {file}"))
}

fn remove_font(dir: &Path, file: &str) -> Result<(), String> {
    std::fs::remove_file(added(dir, file)?).map_err(|e| format!("Couldn't delete {file} ({e})"))
}

/// `fonts` in the config folder (the same folder as connections.json)
fn fonts_dir(app: &tauri::AppHandle) -> AppResult<PathBuf> {
    Ok(app.path().app_config_dir().map_err(|e| AppError::Msg(e.to_string()))?.join("fonts"))
}

/// The font files added in Settings, sorted by name
#[tauri::command]
pub async fn custom_fonts(app: tauri::AppHandle) -> AppResult<Vec<CustomFont>> {
    Ok(custom_in(&fonts_dir(&app)?))
}

/// "Add Font File…": a .ttf / .otf / .woff / .woff2 picked in the open dialog (opened here, so the WebView never names a path) is
/// copied into the fonts folder. None if cancelled
#[tauri::command]
pub async fn add_font_file(app: tauri::AppHandle) -> AppResult<Option<CustomFont>> {
    let picked = app.dialog().file().add_filter("Fonts", &["ttf", "otf", "woff", "woff2"]).blocking_pick_file();
    let Some(src) = picked.and_then(|p| p.into_path().ok()) else { return Ok(None) };
    add_font(&fonts_dir(&app)?, &src).map(Some).map_err(AppError::Msg)
}

/// Deletes an added font file (the copy; the original is untouched)
#[tauri::command]
pub async fn remove_font_file(app: tauri::AppHandle, file: String) -> AppResult<()> {
    remove_font(&fonts_dir(&app)?, &file).map_err(AppError::Msg)
}

/// An added font file's bytes (an ArrayBuffer in the WebView, for FontFace: no asset protocol or CSP font-src change needed)
#[tauri::command]
pub async fn font_file(app: tauri::AppHandle, file: String) -> AppResult<tauri::ipc::Response> {
    let path = added(&fonts_dir(&app)?, &file).map_err(AppError::Msg)?;
    Ok(tauri::ipc::Response::new(std::fs::read(path)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn f(name: &str, mono: bool) -> FontFamily {
        FontFamily { name: name.into(), mono }
    }

    #[test]
    fn families_dedup_sort_and_mono_only_if_every_face_is() {
        let faces = [("Menlo", true), ("arial", false), ("Menlo", true), ("Osaka", false), ("Osaka", true), (".SF NS", false), ("  ", true), ("Courier New", true)];
        assert_eq!(
            families(faces.map(|(n, m)| (n.to_string(), m))),
            vec![f("arial", false), f("Courier New", true), f("Menlo", true), f("Osaka", false)]
        );
    }

    #[test]
    fn fc_list_lines() {
        let out = "DejaVu Sans Mono\t100\nDejaVu Sans\t\nDejaVu Sans Mono\t100\nNoto Sans Mono CJK JP\t90\nTerminus\t110\nCantarell\t0\nbroken line\n";
        assert_eq!(
            parse_fc_list(out),
            vec![f("broken line", false), f("Cantarell", false), f("DejaVu Sans", false), f("DejaVu Sans Mono", true), f("Noto Sans Mono CJK JP", true), f("Terminus", true)]
        );
    }

    /// A file in `dir` holding a font header (or `head`) padded to `len` bytes
    fn file(dir: &Path, name: &str, head: &[u8], len: usize) -> PathBuf {
        let mut bytes = head.to_vec();
        bytes.resize(len.max(head.len()), 0);
        let p = dir.join(name);
        std::fs::write(&p, bytes).unwrap();
        p
    }
    const TTF: &[u8] = &[0, 1, 0, 0];

    #[test]
    fn font_file_names_are_made_safe() {
        assert_eq!(safe_name("JetBrainsMono-Regular"), "JetBrainsMono-Regular");
        assert_eq!(safe_name("My  Font.v2"), "My Font v2");
        assert_eq!(safe_name("../../etc/passwd"), "etc passwd");
        assert_eq!(safe_name("a\"b;c{d}\\e,f"), "a b c d e f");
        assert_eq!(safe_name("나눔고딕코딩"), "나눔고딕코딩");
        assert_eq!(safe_name(" .. "), "");
    }

    #[test]
    fn adds_a_font_named_after_its_file_with_the_extension_its_bytes_say() {
        let src = tempfile::tempdir().unwrap();
        let fonts = tempfile::tempdir().unwrap();
        let dir = fonts.path().join("fonts");
        // The extension comes from the magic bytes, not the picked name
        let f = add_font(&dir, &file(src.path(), "Fira Code.ttf", b"OTTO", 100)).unwrap();
        assert_eq!(f, CustomFont { name: "Fira Code".into(), file: "Fira Code.otf".into() });
        assert_eq!(std::fs::read(dir.join("Fira Code.otf")).unwrap().len(), 100);
        add_font(&dir, &file(src.path(), "b.woff2", b"wOF2", 10)).unwrap();
        add_font(&dir, &file(src.path(), "c.woff", b"wOFF", 10)).unwrap();
        add_font(&dir, &file(src.path(), "d.ttf", b"true", 10)).unwrap();
        assert_eq!(custom_in(&dir).iter().map(|f| f.file.as_str()).collect::<Vec<_>>(), ["b.woff2", "c.woff", "d.ttf", "Fira Code.otf"]);
    }

    #[test]
    fn a_picked_name_cannot_leave_the_fonts_folder() {
        let src = tempfile::tempdir().unwrap();
        let fonts = tempfile::tempdir().unwrap();
        let dir = fonts.path().join("fonts");
        std::fs::create_dir(src.path().join("x")).unwrap();
        let f = add_font(&dir, &file(src.path(), "..%2F..%2Fevil.ttf", TTF, 10)).unwrap();
        assert_eq!(f.file, "2F 2Fevil.ttf");
        // Only files listed in the folder can be read or removed
        for bad in ["../x", "../fonts/2F 2Fevil.ttf", "/etc/passwd", "2F 2Fevil", "", "."] {
            assert!(added(&dir, bad).is_err(), "{bad}");
        }
        assert_eq!(added(&dir, "2F 2Fevil.ttf").unwrap(), dir.join("2F 2Fevil.ttf"));
        assert!(std::fs::read_dir(fonts.path()).unwrap().count() == 1);
    }

    #[test]
    fn refuses_files_over_20_mb_and_files_that_are_not_fonts() {
        let src = tempfile::tempdir().unwrap();
        let fonts = tempfile::tempdir().unwrap();
        let dir = fonts.path().join("fonts");
        let max = MAX_FONT_BYTES as usize;
        assert_eq!(add_font(&dir, &file(src.path(), "big.ttf", TTF, max + 1)).unwrap_err(), "big.ttf is larger than 20 MB");
        assert!(add_font(&dir, &file(src.path(), "ok.ttf", TTF, max)).is_ok());
        let not_font = add_font(&dir, &file(src.path(), "notes.ttf", b"PK\x03\x04", 100)).unwrap_err();
        assert_eq!(not_font, "notes.ttf isn't a font file (TrueType, OpenType, WOFF or WOFF2)");
        assert!(add_font(&dir, &file(src.path(), "tiny.ttf", b"OT", 2)).is_err());
        assert!(add_font(&dir, &src.path().join("missing.ttf")).unwrap_err().starts_with("Couldn't read missing.ttf"));
        assert_eq!(custom_in(&dir).iter().map(|f| f.file.as_str()).collect::<Vec<_>>(), ["ok.ttf"]);
    }

    #[test]
    fn a_taken_name_gets_a_number_instead_of_replacing_the_font() {
        let src = tempfile::tempdir().unwrap();
        let fonts = tempfile::tempdir().unwrap();
        let dir = fonts.path().join("fonts");
        let a = file(src.path(), "Hack.ttf", TTF, 10);
        assert_eq!(add_font(&dir, &a).unwrap().name, "Hack");
        assert_eq!(add_font(&dir, &file(src.path(), "hack.woff2", b"wOF2", 20)).unwrap().name, "hack 2");
        assert_eq!(add_font(&dir, &a).unwrap(), CustomFont { name: "Hack 3".into(), file: "Hack 3.ttf".into() });
        assert_eq!(std::fs::read(dir.join("Hack.ttf")).unwrap().len(), 10);
        // A long name is cut so the number still fits
        let long = "L".repeat(100);
        let f = add_font(&dir, &file(src.path(), &format!("{long}.ttf"), TTF, 10)).unwrap();
        assert_eq!(f.name.len(), MAX_NAME);
        assert_eq!(add_font(&dir, &file(src.path(), &format!("{long}.ttf"), TTF, 10)).unwrap().name, format!("{} 2", &long[..MAX_NAME]));
        assert_eq!(custom_in(&dir).len(), 5);
    }

    #[test]
    fn removes_an_added_font_and_lists_only_what_it_could_have_added() {
        let src = tempfile::tempdir().unwrap();
        let fonts = tempfile::tempdir().unwrap();
        let dir = fonts.path().join("fonts");
        assert!(custom_in(&dir).is_empty());
        add_font(&dir, &file(src.path(), "Hack.ttf", TTF, 10)).unwrap();
        // Files Qurry wouldn't have made are left out (and so can't be read or removed through it)
        file(&dir, "notes.txt", b"x", 1);
        file(&dir, ".hidden.ttf", TTF, 4);
        file(&dir, "a  b.ttf", TTF, 4);
        std::fs::create_dir(dir.join("Dir.ttf")).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(src.path().join("Hack.ttf"), dir.join("Link.ttf")).unwrap();
        assert_eq!(custom_in(&dir), vec![CustomFont { name: "Hack".into(), file: "Hack.ttf".into() }]);
        assert!(remove_font(&dir, "notes.txt").is_err());
        remove_font(&dir, "Hack.ttf").unwrap();
        assert!(custom_in(&dir).is_empty());
        assert_eq!(remove_font(&dir, "Hack.ttf").unwrap_err(), "No added font file named Hack.ttf");
        assert!(dir.join("notes.txt").exists());
        assert!(src.path().join("Hack.ttf").exists());
    }

    /// The real list on this Mac (macOS ships Menlo)
    #[cfg(target_os = "macos")]
    #[test]
    fn lists_installed_families() {
        let all = list();
        assert!(all.iter().any(|f| f.mono), "no monospaced family in {} families", all.len());
        assert!(all.iter().any(|f| !f.mono));
        assert!(all.contains(&f("Menlo", true)), "{all:?}");
        assert!(all.windows(2).all(|w| w[0].name.to_lowercase() <= w[1].name.to_lowercase()));
    }
}
