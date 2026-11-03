//! Installed font families for Settings › Appearance › Editor / Grid font (a WebView can't list them).
//! macOS: CoreText. Linux: fontconfig's fc-list. Elsewhere: none (the picker then offers the default and the saved family)

use serde::Serialize;
use std::collections::BTreeMap;

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
