//! Color themes of both apps: the built-in palettes and their color tokens (themes.json, which the GUI imports as it is),
//! custom themes (a built-in with some colors changed, saved in settings.json), the contrast checks every theme is held to,
//! and the theme file that Settings › Appearance imports and exports. A custom theme may carry two images (the GUI's Background and
//! Corner, #202): the rules for their files are here, the files themselves are crates/core/src/theme_images.rs.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::OnceLock;

/// Dark and light themes; High contrast is dark with the GUI's high-contrast styles (side lines on the active tab …)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Dark,
    Light,
    Hc,
}

/// One color of a theme: what it is called in the editor, what it colors, and the pair it must keep readable
#[derive(Debug, Deserialize)]
pub struct Token {
    pub key: String,
    pub group: String,
    pub label: String,
    pub desc: String,
    /// Shown without "Advanced colors"
    #[serde(default)]
    pub basic: bool,
    /// [text, background, minimum WCAG contrast] for each surface the color is drawn on (or each color drawn on it): one of the two
    /// is this token
    #[serde(default)]
    pub checks: Vec<(String, String, f64)>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Builtin {
    pub id: String,
    pub name: String,
    pub kind: Kind,
    pub colors: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
pub struct Table {
    pub tokens: Vec<Token>,
    /// The TUI's dark truecolor tokens (crates/tui/src/kit/atoms.rs, by name) and the theme token each one draws with
    pub tui: BTreeMap<String, String>,
    pub themes: Vec<Builtin>,
}

pub fn table() -> &'static Table {
    static T: OnceLock<Table> = OnceLock::new();
    T.get_or_init(|| serde_json::from_str(include_str!("themes.json")).unwrap_or_else(|e| panic!("crates/shared/src/themes.json is not a valid theme table ({e}): fix it, then run cargo test -p qurry-shared")))
}

pub fn builtin(id: &str) -> Option<&'static Builtin> {
    table().themes.iter().find(|t| t.id == id)
}

/// A custom theme (settings.json "customThemes"): a built-in theme and the colors changed from it, and its images
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomTheme {
    pub id: String,
    pub name: String,
    pub base: String,
    #[serde(default)]
    pub colors: BTreeMap<String, String>,
    /// The GUI draws them; the TUI keeps them when it saves settings.json and never draws them. Not in a theme file
    #[serde(default, skip_serializing_if = "ThemeImages::is_empty")]
    pub images: ThemeImages,
}

/// A custom theme's images, each slot optional: Background, faint behind the GUI's SQL editor and result grid; Corner, scaled down
/// in front of the window. `file` is a name in the config folder's theme-images (image_name_ok)
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ThemeImages {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background: Option<BackgroundImage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub corner: Option<CornerImage>,
}

impl ThemeImages {
    pub fn is_empty(&self) -> bool {
        self.background.is_none() && self.corner.is_none()
    }
    /// The image files the theme uses
    pub fn files(&self) -> impl Iterator<Item = &str> {
        self.background.iter().map(|b| b.file.as_str()).chain(self.corner.iter().map(|c| c.file.as_str()))
    }
}

/// opacity 0–100; fill one of FILLS; anchor one of ANCHORS (CSS background-position); span one of SPANS: "area", one picture behind
/// the whole editor area (all splits together), or "pane", one per console or table pane. Saved without it (0.3.5-beta.1): "area"
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BackgroundImage {
    pub file: String,
    pub opacity: u8,
    pub fill: String,
    pub anchor: String,
    #[serde(default = "span_area")]
    pub span: String,
}

fn span_area() -> String {
    "area".into()
}

/// corner one of CORNERS; size SIZE_MIN–SIZE_MAX (% of the window's height); opacity 0–100
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CornerImage {
    pub file: String,
    pub corner: String,
    pub size: u8,
    pub opacity: u8,
}

pub const FILLS: [&str; 4] = ["cover", "contain", "tile", "center"];
pub const ANCHORS: [&str; 9] = ["left top", "center top", "right top", "left center", "center", "right center", "left bottom", "center bottom", "right bottom"];
pub const CORNERS: [&str; 4] = ["left top", "right top", "left bottom", "right bottom"];
pub const SPANS: [&str; 2] = ["area", "pane"];
pub const SIZE_MIN: u8 = 10;
pub const SIZE_MAX: u8 = 40;
/// The largest still image a theme takes, and the largest animated one (GIF, animated WebP)
pub const IMAGE_MAX: usize = 4 * 1024 * 1024;
pub const ANIMATED_MAX: usize = 8 * 1024 * 1024;
/// The extensions a theme image is kept under
pub const IMAGE_EXTS: [&str; 5] = ["png", "apng", "jpg", "gif", "webp"];

/// A theme image's file name as the core names it: 16 lowercase hex digits (its content's hash) and a known extension. So a name
/// that comes back from the GUI can't be a path (no `/`, `\` or `..`)
pub fn image_name_ok(name: &str) -> bool {
    name.split_once('.').is_some_and(|(stem, ext)| stem.len() == 16 && stem.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) && IMAGE_EXTS.contains(&ext))
}

/// The extension an image is kept under, from its first bytes: PNG (an animated one, APNG, as apng), JPEG, GIF or WebP (never SVG
/// or anything else)
pub fn image_ext(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some(if png_animated(bytes) { "apng" } else { "png" })
    } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        Some("jpg")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("gif")
    } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("webp")
    } else {
        None
    }
}

/// A PNG with an animation control chunk (acTL) before its first image data (IDAT): an APNG
fn png_animated(b: &[u8]) -> bool {
    let mut i = 8;
    while let (Some(len), Some(kind)) = (b.get(i..i + 4), b.get(i + 4..i + 8)) {
        match kind {
            b"acTL" => return true,
            b"IDAT" | b"IEND" => return false,
            _ => i += 12 + u32::from_be_bytes([len[0], len[1], len[2], len[3]]) as usize,
        }
    }
    false
}

/// A GIF with more than one frame, an APNG, or a WebP with its animation flag (VP8X)
pub fn is_animated(bytes: &[u8]) -> bool {
    match image_ext(bytes) {
        Some("apng") => true,
        Some("gif") => gif_frames(bytes) > 1,
        Some("webp") => bytes.len() > 20 && &bytes[12..16] == b"VP8X" && bytes[20] & 0x02 != 0,
        _ => false,
    }
}

/// A GIF's frames (image descriptors), counted by walking its blocks; stops at the trailer or where the data ends
fn gif_frames(b: &[u8]) -> usize {
    // Header (6) and logical screen descriptor (7), then the global color table if its flag is set
    let Some(&flags) = b.get(10) else { return 0 };
    let mut i = 13 + if flags & 0x80 != 0 { 3 << ((flags & 7) + 1) } else { 0 };
    let skip_sub_blocks = |mut i: usize| {
        while let Some(&n) = b.get(i) {
            i += 1 + n as usize;
            if n == 0 {
                break;
            }
        }
        i
    };
    let mut frames = 0;
    while let Some(&tag) = b.get(i) {
        match tag {
            // Extension: label, then sub-blocks
            0x21 => i = skip_sub_blocks(i + 2),
            // Image: descriptor (10), local color table, LZW code size, then sub-blocks
            0x2c => {
                frames += 1;
                let Some(&f) = b.get(i + 9) else { break };
                i += 10 + if f & 0x80 != 0 { 3 << ((f & 7) + 1) } else { 0 };
                i = skip_sub_blocks(i + 1);
            }
            _ => break,
        }
    }
    frames
}

/// An image a theme can take: its extension, or why not (a still image up to 4 MB, an animated one up to 8 MB). `shown` names it
pub fn check_image(bytes: &[u8], shown: &str) -> Result<&'static str, String> {
    let ext = image_ext(bytes).ok_or_else(|| format!("{shown} isn't a PNG, JPEG, GIF or WebP image"))?;
    let (max, what) = if is_animated(bytes) { (ANIMATED_MAX, "an animated image") } else { (IMAGE_MAX, "a still image") };
    if bytes.len() > max {
        return Err(format!("{shown} is {:.1} MB: {what} can be at most {} MB", bytes.len() as f64 / (1024.0 * 1024.0), max / (1024 * 1024)));
    }
    Ok(ext)
}

fn background_ok(b: &BackgroundImage) -> bool {
    image_name_ok(&b.file) && b.opacity <= 100 && FILLS.contains(&b.fill.as_str()) && ANCHORS.contains(&b.anchor.as_str()) && SPANS.contains(&b.span.as_str())
}

fn corner_ok(c: &CornerImage) -> bool {
    image_name_ok(&c.file) && c.opacity <= 100 && (SIZE_MIN..=SIZE_MAX).contains(&c.size) && CORNERS.contains(&c.corner.as_str())
}

/// A theme's image settings are ones the GUI can draw: known file names, positions and fills, sizes and opacities in range
fn check_images(i: &ThemeImages) -> Result<(), String> {
    if let Some(f) = i.files().find(|f| !image_name_ok(f)) {
        return Err(format!("\"{f}\" isn't a theme image's file name"));
    }
    if i.background.as_ref().is_some_and(|b| !background_ok(b)) {
        return Err("The background image's opacity, fill, placement or span isn't one Qurry knows".into());
    }
    if i.corner.as_ref().is_some_and(|c| !corner_ok(c)) {
        return Err("The corner image's corner, size or opacity isn't one Qurry knows".into());
    }
    Ok(())
}

/// An image slot of a settings.json entry this version can draw
fn slot_from_value(images: &serde_json::Value, key: &str) -> Option<ThemeImages> {
    let v = images.get(key)?.clone();
    match key {
        "background" => serde_json::from_value(v).ok().filter(background_ok).map(|b| ThemeImages { background: Some(b), corner: None }),
        "corner" => serde_json::from_value(v).ok().filter(corner_ok).map(|c| ThemeImages { background: None, corner: Some(c) }),
        _ => None,
    }
}

/// A custom theme from its settings.json entry; None when this version can't use it (an unknown base, a bad color …). An image slot
/// it can't draw (a later version's fill or file type) is left out of the theme, which keeps its colors and other slot
pub fn custom_from_value(v: &serde_json::Value) -> Option<CustomTheme> {
    let mut plain = v.clone();
    let images = plain.as_object_mut()?.remove("images").unwrap_or_default();
    let mut t: CustomTheme = serde_json::from_value(plain).ok()?;
    t.images = ThemeImages {
        background: slot_from_value(&images, "background").and_then(|i| i.background),
        corner: slot_from_value(&images, "corner").and_then(|i| i.corner),
    };
    check(&t).is_ok().then_some(t)
}

/// A custom theme's settings.json entry, written over its old one (`old`): what this version doesn't know stays as it was, the other
/// fields of the entry and an image slot it couldn't read (so the GUI never showed it, and saving can't have removed it)
pub fn custom_to_value(old: Option<&serde_json::Value>, t: &CustomTheme) -> serde_json::Value {
    let mut entry = old.and_then(|o| o.as_object().cloned()).unwrap_or_default();
    let new = serde_json::to_value(t).unwrap_or_default();
    for k in ["id", "name", "base", "colors"] {
        entry.insert(k.into(), new[k].clone());
    }
    let mut images = new.get("images").and_then(|i| i.as_object().cloned()).unwrap_or_default();
    if let Some(old_images) = old.and_then(|o| o.get("images")).and_then(|i| i.as_object()) {
        for (k, v) in old_images {
            if !images.contains_key(k) && slot_from_value(&serde_json::Value::Object(old_images.clone()), k).is_none() {
                images.insert(k.clone(), v.clone());
            }
        }
    }
    if images.is_empty() {
        entry.remove("images");
    } else {
        entry.insert("images".into(), images.into());
    }
    entry.into()
}

/// Sync with OS: the theme for a dark and for a light system appearance (the TUI: terminal background, COLORFGBG)
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ThemeSync {
    pub dark: String,
    pub light: String,
}

impl Default for ThemeSync {
    fn default() -> Self {
        Self { dark: "dark".into(), light: "light".into() }
    }
}

/// The theme settings both apps share (settings.json "theme", "themeSync", "customThemes")
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThemeSettings {
    /// A built-in or custom theme id, or "system" (Sync with OS); None when never picked (each app has its default)
    pub theme: Option<String>,
    pub sync: ThemeSync,
    pub custom: Vec<CustomTheme>,
}

/// The pick for Sync with OS
pub const SYSTEM: &str = "system";

/// A theme with every color
#[derive(Clone, Debug, PartialEq)]
pub struct Resolved {
    pub id: String,
    pub name: String,
    pub kind: Kind,
    pub custom: bool,
    pub colors: BTreeMap<String, String>,
}

impl Resolved {
    pub fn color(&self, token: &str) -> &str {
        self.colors.get(token).map(String::as_str).unwrap_or("#000000")
    }
}

impl ThemeSettings {
    /// A built-in or custom theme by id
    pub fn find(&self, id: &str) -> Option<Resolved> {
        if let Some(b) = builtin(id) {
            return Some(Resolved { id: b.id.clone(), name: b.name.clone(), kind: b.kind, custom: false, colors: b.colors.clone() });
        }
        let c = self.custom.iter().find(|c| c.id == id)?;
        let b = builtin(&c.base)?;
        let mut colors = b.colors.clone();
        colors.extend(c.colors.iter().filter(|(k, _)| b.colors.contains_key(*k)).map(|(k, v)| (k.clone(), v.clone())));
        Some(Resolved { id: c.id.clone(), name: c.name.clone(), kind: b.kind, custom: true, colors })
    }

    /// The theme to draw: `default` when none was picked, Sync with OS by `os_dark`; one that no longer exists is Dark
    pub fn resolve(&self, default: &str, os_dark: bool) -> Resolved {
        let pick = self.theme.as_deref().unwrap_or(default);
        let id = if pick == SYSTEM { if os_dark { &self.sync.dark } else { &self.sync.light } } else { pick };
        self.find(id).or_else(|| self.find("dark")).expect("themes.json has Dark")
    }

    /// Every theme in picker order: the built-in ones, then the custom ones
    pub fn all(&self) -> Vec<Resolved> {
        table().themes.iter().map(|t| t.id.as_str()).chain(self.custom.iter().map(|c| c.id.as_str())).filter_map(|id| self.find(id)).collect()
    }
}

/// #rrggbb (lower or upper case), nothing else
pub fn rgb(hex: &str) -> Option<(u8, u8, u8)> {
    let h = hex.strip_prefix('#').filter(|h| h.len() == 6 && h.bytes().all(|b| b.is_ascii_hexdigit()))?;
    let p = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).ok();
    Some((p(0)?, p(2)?, p(4)?))
}

/// WCAG 2 contrast ratio of two #rrggbb colors (1 when either isn't one)
pub fn contrast(a: &str, b: &str) -> f64 {
    let lum = |(r, g, b): (u8, u8, u8)| {
        let lin = |v: u8| {
            let v = v as f64 / 255.0;
            if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
        };
        0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b)
    };
    let (Some(a), Some(b)) = (rgb(a), rgb(b)) else { return 1.0 };
    let (x, y) = (lum(a), lum(b));
    (x.max(y) + 0.05) / (x.min(y) + 0.05)
}

/// A pair below its minimum: shown next to the token in the editor, never fixed silently
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Warning {
    pub token: String,
    pub fg: String,
    pub bg: String,
    pub ratio: f64,
    pub min: f64,
}

/// The tokens with a pair (text ≥ 4.5:1, dim text, icons and the accent ≥ 3:1) a palette doesn't meet: one warning per token, for
/// its pair furthest below the minimum
pub fn warnings(colors: &BTreeMap<String, String>) -> Vec<Warning> {
    let get = |k: &str| colors.get(k).map(String::as_str).unwrap_or("");
    table()
        .tokens
        .iter()
        .filter_map(|t| {
            t.checks
                .iter()
                .map(|(fg, bg, min)| Warning { token: t.key.clone(), fg: fg.clone(), bg: bg.clone(), ratio: contrast(get(fg), get(bg)), min: *min })
                .filter(|w| w.ratio < w.min)
                .min_by(|a, b| (a.ratio / a.min).total_cmp(&(b.ratio / b.min)))
        })
        .collect()
}

/// A theme file larger than this is refused unread (one holds ~40 colors and a name: about 1.5 KB)
pub const FILE_MAX: usize = 16 * 1024;
/// The "qurryTheme" value of a theme without images (what older versions read), and of one with them
const FILE_VERSION: u64 = 1;
const FILE_VERSION_IMAGES: u64 = 2;
/// The length of `n` bytes in base64
pub const fn b64_len(n: usize) -> usize {
    n.div_ceil(3) * 4
}
/// A theme file with images is at most this: two images at the animated cap, in base64, and the rest of a theme file
pub const FILE_IMAGES_MAX: usize = 2 * b64_len(ANIMATED_MAX) + FILE_MAX;
const NAME_MAX: usize = 64;

/// The name, base and colors are the ones a custom theme may have: a known built-in, known tokens, #rrggbb only
pub fn check(t: &CustomTheme) -> Result<(), String> {
    let name = t.name.trim();
    if name.is_empty() || name.chars().count() > NAME_MAX || name.chars().any(char::is_control) {
        return Err(format!("The theme name must be 1 to {NAME_MAX} characters on one line"));
    }
    if builtin(&t.base).is_none() {
        let ids: Vec<_> = table().themes.iter().map(|t| t.id.as_str()).collect();
        return Err(format!("\"base\" must be a built-in theme ({}), not \"{}\"", ids.join(", "), t.base));
    }
    for (k, v) in &t.colors {
        if !table().tokens.iter().any(|t| &t.key == k) {
            return Err(format!("Unknown color \"{k}\" (Settings › Appearance › Theme lists the colors a theme has)"));
        }
        if rgb(v).is_none() {
            return Err(format!("\"{k}\" is \"{v}\": colors are written #rrggbb"));
        }
    }
    check_images(&t.images)
}

/// A theme file's image (version 2): its type (the extension it is kept under), its bytes in base64 and the slot's settings. As read,
/// nothing in it is checked yet: the core decodes it, checks it as Choose… does and copies it
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileBackground {
    #[serde(rename = "type")]
    pub kind: String,
    pub data: String,
    pub opacity: u8,
    pub fill: String,
    pub anchor: String,
    #[serde(default = "span_area")]
    pub span: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileCorner {
    #[serde(rename = "type")]
    pub kind: String,
    pub data: String,
    pub corner: String,
    pub size: u8,
    pub opacity: u8,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileImages {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background: Option<FileBackground>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub corner: Option<FileCorner>,
}

impl BackgroundImage {
    /// This slot as a theme file carries it (its image `data` in base64, of type `kind`)
    pub fn to_file(&self, kind: &str, data: String) -> FileBackground {
        FileBackground { kind: kind.into(), data, opacity: self.opacity, fill: self.fill.clone(), anchor: self.anchor.clone(), span: self.span.clone() }
    }
}

impl CornerImage {
    pub fn to_file(&self, kind: &str, data: String) -> FileCorner {
        FileCorner { kind: kind.into(), data, corner: self.corner.clone(), size: self.size, opacity: self.opacity }
    }
}

impl FileBackground {
    /// The slot with its image copied as `file`, if its settings are ones Qurry can draw
    pub fn slot(&self, file: String) -> Option<BackgroundImage> {
        Some(BackgroundImage { file, opacity: self.opacity, fill: self.fill.clone(), anchor: self.anchor.clone(), span: self.span.clone() }).filter(background_ok)
    }
}

impl FileCorner {
    pub fn slot(&self, file: String) -> Option<CornerImage> {
        Some(CornerImage { file, corner: self.corner.clone(), size: self.size, opacity: self.opacity }).filter(corner_ok)
    }
}

/// The file Export… writes: name, base and colors (version 1, which every version reads); with `images`, version 2 with them
pub fn file_text(t: &CustomTheme, images: Option<&FileImages>) -> String {
    let mut v = serde_json::json!({ "qurryTheme": FILE_VERSION, "name": t.name, "base": t.base, "colors": t.colors });
    if let Some(i) = images {
        v["qurryTheme"] = FILE_VERSION_IMAGES.into();
        v["images"] = serde_json::to_value(i).unwrap_or_default();
    }
    serde_json::to_string_pretty(&v).unwrap_or_default() + "\n"
}

/// A theme file (Import…): {"qurryTheme": 1, "name", "base", "colors": {token: "#rrggbb"}} and nothing else, or version 2 with
/// "images" too ({"background": {...}, "corner": {...}}, each with "type" and "data" and the slot's settings). The theme comes back
/// without an id (the caller gives it one when it is saved) and without images; a version 2 file's images come back as they are in it,
/// for the core to check. Each size is checked before anything is read: the file's (16 KB, or for version 2 FILE_IMAGES_MAX)
pub fn parse_file(text: &str) -> Result<(CustomTheme, FileImages), String> {
    let too_large = |max: usize| format!("Not a Qurry theme: the file is {} KB, a theme file is at most {} KB", text.len().div_ceil(1024), max / 1024);
    if text.len() > FILE_IMAGES_MAX {
        return Err(too_large(FILE_IMAGES_MAX));
    }
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields, rename_all = "camelCase")]
    struct File {
        qurry_theme: u64,
        name: String,
        base: String,
        colors: BTreeMap<String, String>,
        images: Option<FileImages>,
    }
    let f: File = serde_json::from_str(text).map_err(|e| format!("Not a Qurry theme file ({e})"))?;
    let images = match (f.qurry_theme, f.images) {
        (FILE_VERSION, None) if text.len() <= FILE_MAX => FileImages::default(),
        (FILE_VERSION, None) => return Err(too_large(FILE_MAX)),
        (FILE_VERSION, Some(_)) => return Err("Not a Qurry theme file (a version 1 file has no \"images\")".into()),
        (FILE_VERSION_IMAGES, Some(i)) => i,
        (FILE_VERSION_IMAGES, None) => return Err("Not a Qurry theme file (a version 2 file has \"images\")".into()),
        (v, _) => return Err(format!("This theme file is version {v} — this Qurry reads versions {FILE_VERSION} and {FILE_VERSION_IMAGES}: update Qurry")),
    };
    let t = CustomTheme { id: String::new(), name: f.name.trim().to_string(), base: f.base, colors: f.colors.into_iter().map(|(k, v)| (k, v.to_ascii_lowercase())).collect(), ..Default::default() };
    check(&t)?;
    Ok((t, images))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The test like the TUI light theme's, over every built-in theme: text 4.5:1, dim text, icons and the accent 3:1, on each surface
    /// they are drawn on (panels, odd rows, hover, the current line, the selection, the Production tint)
    #[test]
    fn every_builtin_theme_meets_the_contrast_minimums() {
        assert_eq!(table().themes.len(), 7);
        for t in &table().themes {
            let w = warnings(&t.colors);
            assert!(w.is_empty(), "{} ({}): {w:?}", t.name, t.id);
            // Comments stay dimmer than code (#162)
            let c = &t.colors;
            assert!(contrast(&c["text"], &c["bg"]) > contrast(&c["comment"], &c["bg"]), "{}: comment as bright as text", t.id);
        }
    }

    #[test]
    fn a_low_contrast_pair_fails_the_check() {
        let mut colors = builtin("dark").unwrap().colors.clone();
        colors.insert("comment".into(), "#3a3c40".into());
        colors.insert("onAccent".into(), "#3574f0".into());
        let w = warnings(&colors);
        assert_eq!(w.iter().map(|w| w.token.as_str()).collect::<Vec<_>>(), ["onAccent", "comment"]);
        assert!(w[1].ratio < 4.5 && w[1].min == 4.5 && w[1].fg == "comment" && w[1].bg == "panel2", "{w:?}");
        // Checked on every surface a color is drawn on, not only the editor's background: Dracula's published comment blue as dim
        // text is fine on the background but not on odd rows or the selection (the warning names the worst)
        let mut colors = builtin("dracula").unwrap().colors.clone();
        colors.insert("muted".into(), "#6272a4".into());
        colors.insert("nullText".into(), "#6272a4".into());
        assert!(contrast("#6272a4", &colors["bg"]) >= 3.0);
        let w = warnings(&colors);
        assert_eq!(w.iter().map(|w| (w.token.as_str(), w.bg.as_str())).collect::<Vec<_>>(), [("muted", "sel"), ("nullText", "sel")], "{w:?}");
        // A surface made too close to the text it carries warns at the text
        let mut colors = builtin("solarized-dark").unwrap().colors.clone();
        colors.insert("panel2".into(), "#2e5560".into());
        let w = warnings(&colors);
        assert!(w.iter().any(|w| w.token == "text" && w.bg == "panel2") && w.iter().any(|w| w.token == "keyword" && w.bg == "panel2"), "{w:?}");
        // A custom theme is checked on its colors over its base's
        let s = ThemeSettings { custom: vec![CustomTheme { id: "c1".into(), name: "Dim".into(), base: "dracula".into(), colors: [("text".into(), "#44475a".into())].into(), ..Default::default() }], ..Default::default() };
        let w = warnings(&s.find("c1").unwrap().colors);
        assert!(w.iter().any(|w| w.token == "text" && w.ratio < 4.5), "{w:?}");
    }

    #[test]
    fn every_theme_has_every_token_as_rrggbb() {
        let keys: Vec<_> = table().tokens.iter().map(|t| t.key.as_str()).collect();
        for t in &table().themes {
            assert_eq!(t.colors.keys().map(String::as_str).collect::<std::collections::BTreeSet<_>>(), keys.iter().copied().collect(), "{}", t.id);
            assert!(t.colors.values().all(|v| rgb(v).is_some()), "{}", t.id);
        }
        for (atom, tok) in &table().tui {
            assert!(keys.contains(&tok.as_str()), "tui {atom} → {tok}");
        }
        for t in &table().tokens {
            for (fg, bg, _) in &t.checks {
                assert!(keys.contains(&fg.as_str()) && keys.contains(&bg.as_str()) && (fg == &t.key || bg == &t.key), "{}", t.key);
            }
        }
    }

    #[test]
    fn contrast_is_the_wcag_ratio() {
        assert!((contrast("#000000", "#ffffff") - 21.0).abs() < 1e-9);
        assert!((contrast("#ffffff", "#ffffff") - 1.0).abs() < 1e-9);
        assert!((contrast("#767676", "#ffffff") - 4.54).abs() < 0.01);
        assert_eq!(contrast("#fff", "#000000"), 1.0, "only #rrggbb");
    }

    #[test]
    fn picks_resolve_to_a_theme() {
        let custom = CustomTheme { id: "c-1".into(), name: "Night".into(), base: "dracula".into(), colors: [("bg".into(), "#101010".into()), ("nope".into(), "#ffffff".into())].into(), ..Default::default() };
        let s = ThemeSettings { theme: Some("c-1".into()), sync: ThemeSync { dark: "one-dark".into(), light: "solarized-light".into() }, custom: vec![custom] };
        let r = s.resolve("dark", true);
        assert_eq!((r.id.as_str(), r.kind, r.custom, r.color("bg"), r.color("text")), ("c-1", Kind::Dark, true, "#101010", builtin("dracula").unwrap().colors["text"].as_str()));
        assert!(!r.colors.contains_key("nope"));
        let sys = ThemeSettings { theme: Some(SYSTEM.into()), ..s.clone() };
        assert_eq!(sys.resolve("dark", true).id, "one-dark");
        assert_eq!(sys.resolve("dark", false).id, "solarized-light");
        assert_eq!(ThemeSettings::default().resolve("system", false).id, "light", "the app's default when none was picked");
        assert_eq!(ThemeSettings { theme: Some("deleted".into()), ..Default::default() }.resolve("dark", true).id, "dark");
        assert_eq!(s.all().iter().map(|t| t.id.as_str()).collect::<Vec<_>>(), ["dark", "light", "hc", "dracula", "one-dark", "solarized-dark", "solarized-light", "c-1"]);
    }

    #[test]
    fn theme_files_round_trip_and_anything_else_is_refused() {
        let t = CustomTheme { id: "c-9".into(), name: "Paper".into(), base: "solarized-light".into(), colors: [("text".into(), "#3d4f55".into())].into(), ..Default::default() };
        let back = parse_file(&file_text(&t, None)).unwrap().0;
        assert_eq!(back, CustomTheme { id: String::new(), ..t.clone() });
        // Upper-case hex is read, stored lower case
        assert_eq!(parse_file(r##"{"qurryTheme": 1, "name": "A", "base": "dark", "colors": {"bg": "#ABCDEF"}}"##).unwrap().0.colors["bg"], "#abcdef");
        let refused = |text: &str, want: &str| {
            let e = parse_file(text).unwrap_err();
            assert!(e.contains(want), "{text}: {e}");
        };
        refused(r##"{"qurryTheme": 1, "name": "A", "base": "dark", "colors": {"bg": "#abc"}}"##, "#rrggbb");
        refused(r##"{"qurryTheme": 1, "name": "A", "base": "dark", "colors": {"bg": "red"}}"##, "#rrggbb");
        refused(r##"{"qurryTheme": 1, "name": "A", "base": "dark", "colors": {"bg": "#12345g"}}"##, "#rrggbb");
        refused(r##"{"qurryTheme": 1, "name": "A", "base": "dark", "colors": {"background": "#123456"}}"##, "Unknown color \"background\"");
        refused(r##"{"qurryTheme": 1, "name": "A", "base": "monokai", "colors": {}}"##, "built-in theme");
        refused(r##"{"qurryTheme": 1, "name": " ", "base": "dark", "colors": {}}"##, "name");
        refused(r##"{"qurryTheme": 3, "name": "A", "base": "dark", "colors": {}}"##, "update Qurry");
        refused(r##"{"qurryTheme": 1, "name": "A", "base": "dark", "colors": {}, "script": "x"}"##, "unknown field");
        refused(r##"{"name": "A", "base": "dark", "colors": {}}"##, "Not a Qurry theme file");
        refused(r##"{"qurryTheme": 1, "name": "A", "base": "dark", "colors": {"bg": 5}}"##, "Not a Qurry theme file");
        refused("[]", "Not a Qurry theme file");
        refused(&format!(r##"{{"qurryTheme": 1, "name": "{}", "base": "dark", "colors": {{}}}}"##, "x".repeat(FILE_MAX)), "at most 16 KB");
        refused(&format!(r##"{{"qurryTheme": 1, "name": "{}", "base": "dark", "colors": {{}}}}"##, "x".repeat(65)), "1 to 64");
    }

    #[test]
    fn theme_files_with_images_are_version_2_and_as_strict() {
        let t = CustomTheme { id: "custom-1".into(), name: "Pics".into(), base: "dark".into(), colors: [("bg".into(), "#101010".into())].into(), ..Default::default() };
        let bg = BackgroundImage { file: "0123456789abcdef.png".into(), opacity: 20, fill: "contain".into(), anchor: "right bottom".into(), span: "pane".into() };
        let cn = CornerImage { file: "fedcba9876543210.gif".into(), corner: "left top".into(), size: 30, opacity: 90 };
        let images = FileImages { background: Some(bg.to_file("png", "iVBORw0K".into())), corner: Some(cn.to_file("gif", "R0lGODlh".into())) };
        let text = file_text(&t, Some(&images));
        // Never the local file names
        assert!(!text.contains("0123456789abcdef") && !text.contains("fedcba9876543210"), "{text}");
        let v: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(v["qurryTheme"], 2);
        assert_eq!(v["images"]["background"], serde_json::json!({ "type": "png", "data": "iVBORw0K", "opacity": 20, "fill": "contain", "anchor": "right bottom", "span": "pane" }));
        let (back, back_images) = parse_file(&text).unwrap();
        assert_eq!((back.name.as_str(), back.colors["bg"].as_str(), back.images.is_empty()), ("Pics", "#101010", true));
        assert_eq!(back_images, images);
        // The slots' settings back with a copy's name; settings Qurry can't draw give none
        assert_eq!(back_images.background.as_ref().unwrap().slot(bg.file.clone()), Some(bg.clone()));
        assert_eq!(back_images.corner.as_ref().unwrap().slot(cn.file.clone()), Some(cn.clone()));
        assert_eq!(FileBackground { fill: "stretch".into(), ..images.background.clone().unwrap() }.slot(bg.file.clone()), None);
        // Without images: version 1, as before
        assert_eq!(serde_json::from_str::<serde_json::Value>(&file_text(&t, None)).unwrap()["qurryTheme"], 1);
        let refused = |text: &str, want: &str| {
            let e = parse_file(text).unwrap_err();
            assert!(e.contains(want), "{}: {e}", &text[..text.len().min(120)]);
        };
        let with = |images: &str| format!(r##"{{"qurryTheme": 2, "name": "A", "base": "dark", "colors": {{}}, "images": {images}}}"##);
        refused(&with(r#"{"background": {"type": "png", "data": "", "opacity": 1, "fill": "cover", "anchor": "center", "file": "../x"}}"#), "unknown field");
        refused(&with(r#"{"frame": {}}"#), "unknown field");
        refused(&with(r#"{"corner": {"type": "gif", "corner": "left top", "size": 20, "opacity": 100}}"#), "missing field");
        refused(r##"{"qurryTheme": 2, "name": "A", "base": "dark", "colors": {}}"##, "version 2 file has");
        refused(r##"{"qurryTheme": 1, "name": "A", "base": "dark", "colors": {}, "images": {}}"##, "version 1 file has no");
        // Size: a version 1 file over 16 KB as before; any file over the version 2 cap before it is parsed
        refused(&format!(r##"{{"qurryTheme": 1, "name": "{}", "base": "dark", "colors": {{}}}}"##, "a".repeat(FILE_MAX)), "at most 16 KB");
        refused(&format!("{{{}", " ".repeat(FILE_IMAGES_MAX)), &format!("at most {} KB", FILE_IMAGES_MAX / 1024));
        let large = with(&format!(r#"{{"background": {{"type": "png", "data": "{}", "opacity": 1, "fill": "cover", "anchor": "center"}}}}"#, "A".repeat(FILE_MAX * 2)));
        assert_eq!(parse_file(&large).unwrap().1.background.unwrap().data.len(), FILE_MAX * 2, "a version 2 file may be larger than 16 KB");
        assert_eq!(b64_len(3), 4);
        assert_eq!(b64_len(4), 8);
    }

    /// A GIF of `frames` 1×1 frames (each with a graphic control extension), with a global color table if `gct`
    fn gif(frames: usize, gct: bool) -> Vec<u8> {
        let mut b = b"GIF89a".to_vec();
        b.extend([1, 0, 1, 0, if gct { 0x80 } else { 0 }, 0, 0]);
        if gct {
            b.extend([0; 6]);
        }
        b.extend([0x21, 0xff, 11]);
        b.extend(b"NETSCAPE2.0");
        b.extend([3, 1, 0, 0, 0]);
        for _ in 0..frames {
            b.extend([0x21, 0xf9, 4, 0, 10, 0, 0, 0]);
            b.extend([0x2c, 0, 0, 0, 0, 1, 0, 1, 0, 0, 2, 2, 0x4c, 1, 0]);
        }
        b.push(0x3b);
        b
    }
    /// A WebP with a VP8X chunk whose flags are `flags`
    fn webp(flags: u8) -> Vec<u8> {
        let mut b = b"RIFF\0\0\0\0WEBPVP8X".to_vec();
        b.extend([10, 0, 0, 0, flags, 0, 0, 0]);
        b
    }

    #[test]
    fn theme_images_are_known_by_their_first_bytes() {
        assert_eq!(image_ext(b"\x89PNG\r\n\x1a\n...."), Some("png"));
        assert_eq!(image_ext(&[0xff, 0xd8, 0xff, 0xe0]), Some("jpg"));
        assert_eq!(image_ext(&gif(1, false)), Some("gif"));
        assert_eq!(image_ext(&webp(0)), Some("webp"));
        // Never SVG (it can carry script), nor a file named .png that isn't one
        assert_eq!(image_ext(br#"<svg xmlns="http://www.w3.org/2000/svg"/>"#), None);
        assert_eq!(image_ext(b"<?xml version=\"1.0\"?><svg/>"), None);
        assert_eq!(image_ext(b"PNG"), None);
        assert_eq!(image_ext(b""), None);
        assert_eq!(image_ext(b"RIFF\0\0\0\0WAVE"), None);
    }

    /// A PNG of these chunks (length, type, data, a dummy CRC)
    fn png(chunks: &[(&[u8; 4], &[u8])]) -> Vec<u8> {
        let mut b = b"\x89PNG\r\n\x1a\n".to_vec();
        for (kind, data) in chunks {
            b.extend((data.len() as u32).to_be_bytes());
            b.extend(*kind);
            b.extend(*data);
            b.extend([0; 4]);
        }
        b
    }

    #[test]
    fn an_animated_png_is_kept_as_apng_and_capped_as_animated() {
        let ihdr: (&[u8; 4], &[u8]) = (b"IHDR", &[0; 13]);
        let still = png(&[ihdr, (b"IDAT", b"x"), (b"IEND", b"")]);
        let apng = png(&[ihdr, (b"acTL", &[0, 0, 0, 2, 0, 0, 0, 0]), (b"IDAT", b"x"), (b"IEND", b"")]);
        // acTL after the image data isn't an APNG (a viewer draws it still)
        let late = png(&[ihdr, (b"IDAT", b"x"), (b"acTL", &[0; 8]), (b"IEND", b"")]);
        assert_eq!([image_ext(&still), image_ext(&apng), image_ext(&late)], [Some("png"), Some("apng"), Some("png")]);
        assert!(!is_animated(&still) && is_animated(&apng));
        assert!(image_name_ok("0123456789abcdef.apng"));
        let pad = |mut b: Vec<u8>, to: usize| {
            b.resize(to, 0);
            b
        };
        assert!(check_image(&pad(still, IMAGE_MAX + 1), "a.png").unwrap_err().ends_with("a still image can be at most 4 MB"));
        assert_eq!(check_image(&pad(apng.clone(), IMAGE_MAX + 1), "a.png"), Ok("apng"));
        assert!(check_image(&pad(apng, ANIMATED_MAX + 1), "a.png").unwrap_err().ends_with("an animated image can be at most 8 MB"));
        // Cut short: no panic
        assert_eq!(image_ext(&png(&[ihdr])[..20]), Some("png"));
    }

    #[test]
    fn animated_gifs_and_webps_are_told_from_still_ones() {
        assert!(!is_animated(&gif(1, false)));
        assert!(is_animated(&gif(2, false)));
        assert!(is_animated(&gif(3, true)), "past a global color table");
        assert!(!is_animated(&webp(0x10)));
        assert!(is_animated(&webp(0x02)));
        assert!(!is_animated(b"\x89PNG\r\n\x1a\n"));
        // Cut short: no panic, counts what is there
        let g = gif(2, true);
        assert!(!is_animated(&g[..30]));
    }

    #[test]
    fn theme_images_are_capped_at_4_mb_still_and_8_mb_animated() {
        let pad = |mut b: Vec<u8>, to: usize| {
            b.resize(to, 0);
            b
        };
        assert_eq!(check_image(&pad(b"\x89PNG\r\n\x1a\n".to_vec(), IMAGE_MAX), "a.png"), Ok("png"));
        let e = check_image(&pad(b"\x89PNG\r\n\x1a\n".to_vec(), IMAGE_MAX + 1), "a.png").unwrap_err();
        assert_eq!(e, "a.png is 4.0 MB: a still image can be at most 4 MB");
        // The padding goes after the trailer: still two frames
        assert_eq!(check_image(&pad(gif(2, false), IMAGE_MAX + 1), "a.gif"), Ok("gif"));
        assert!(check_image(&pad(gif(2, false), ANIMATED_MAX + 1), "a.gif").unwrap_err().ends_with("an animated image can be at most 8 MB"));
        assert!(check_image(&pad(gif(1, false), IMAGE_MAX + 1), "a.gif").unwrap_err().ends_with("a still image can be at most 4 MB"));
        assert_eq!(check_image(b"<svg/>", "logo.svg").unwrap_err(), "logo.svg isn't a PNG, JPEG, GIF or WebP image");
    }

    #[test]
    fn a_theme_image_name_is_a_plain_name_the_core_gave() {
        assert!(image_name_ok("0123456789abcdef.png"));
        assert!(image_name_ok("0123456789abcdef.webp"));
        for bad in ["../0123456789abcdef.png", "0123456789abcdef.png/..", "a/0123456789abcdef.png", "a\\0123456789abcdef.png", "..", "", "0123456789ABCDEF.png", "0123456789abcdef.svg", "0123456789abcde.png", "0123456789abcdef.png.png", "/etc/passwd"] {
            assert!(!image_name_ok(bad), "{bad}");
        }
    }

    #[test]
    fn a_custom_themes_images_are_checked_kept_in_settings_and_left_out_of_its_file() {
        let images = ThemeImages {
            background: Some(BackgroundImage { file: "0123456789abcdef.png".into(), opacity: 15, fill: "cover".into(), anchor: "right bottom".into(), span: "pane".into() }),
            corner: Some(CornerImage { file: "fedcba9876543210.gif".into(), corner: "right bottom".into(), size: 20, opacity: 100 }),
        };
        let t = CustomTheme { id: "custom-1".into(), name: "Pics".into(), base: "dark".into(), colors: BTreeMap::new(), images: images.clone() };
        assert_eq!(check(&t), Ok(()));
        // settings.json: written and read back as they are; a theme without images writes no "images"
        let json = serde_json::to_value(&t).unwrap();
        assert_eq!(json["images"]["corner"], serde_json::json!({ "file": "fedcba9876543210.gif", "corner": "right bottom", "size": 20, "opacity": 100 }));
        assert_eq!(serde_json::from_value::<CustomTheme>(json).unwrap(), t);
        assert!(serde_json::to_value(CustomTheme { images: ThemeImages::default(), ..t.clone() }).unwrap().get("images").is_none());
        // Older settings without images read as none
        assert!(serde_json::from_str::<CustomTheme>(r#"{"id": "custom-1", "name": "A", "base": "dark"}"#).unwrap().images.is_empty());
        assert_eq!(images.files().collect::<Vec<_>>(), ["0123456789abcdef.png", "fedcba9876543210.gif"]);
        // The file leaves them out
        assert!(!file_text(&t, None).contains("images") && !file_text(&t, None).contains("0123456789abcdef"));
        assert!(parse_file(&file_text(&t, None)).unwrap().0.images.is_empty());
        let with = |i: ThemeImages| check(&CustomTheme { images: i, ..t.clone() });
        let bg = images.background.clone().unwrap();
        let cn = images.corner.clone().unwrap();
        assert!(with(ThemeImages { background: Some(BackgroundImage { file: "../x.png".into(), ..bg.clone() }), corner: None }).unwrap_err().contains("file name"));
        assert!(with(ThemeImages { background: Some(BackgroundImage { opacity: 101, ..bg.clone() }), corner: None }).is_err());
        assert!(with(ThemeImages { background: Some(BackgroundImage { fill: "stretch".into(), ..bg.clone() }), corner: None }).is_err());
        assert!(with(ThemeImages { background: Some(BackgroundImage { anchor: "middle".into(), ..bg }), corner: None }).is_err());
        assert!(with(ThemeImages { background: None, corner: Some(CornerImage { size: 41, ..cn.clone() }) }).is_err());
        assert!(with(ThemeImages { background: None, corner: Some(CornerImage { size: 9, ..cn.clone() }) }).is_err());
        assert!(with(ThemeImages { background: None, corner: Some(CornerImage { corner: "center".into(), ..cn }) }).is_err());
    }


    #[test]
    fn an_image_slot_this_version_cant_draw_is_left_out_on_read_and_kept_on_write() {
        let entry = serde_json::json!({
            "id": "custom-1", "name": "Later", "base": "dark", "colors": { "bg": "#101010" }, "tags": ["mine"],
            "images": {
                "background": { "file": "0123456789abcdef.avif", "opacity": 15, "fill": "cover", "anchor": "center" },
                "corner": { "file": "fedcba9876543210.gif", "corner": "right bottom", "size": 20, "opacity": 100 },
                "frame": { "file": "1111111111111111.png" }
            }
        });
        // Read: the theme and its colors and corner; the background (an .avif) and the unknown "frame" left out
        let t = custom_from_value(&entry).unwrap();
        assert_eq!(t.colors["bg"], "#101010");
        assert!(t.images.background.is_none());
        assert_eq!(t.images.corner.as_ref().unwrap().file, "fedcba9876543210.gif");
        // A new fill in the background: also left out, the theme kept
        let mut later = entry.clone();
        later["images"]["background"]["file"] = "0123456789abcdef.png".into();
        later["images"]["background"]["fill"] = "stretch".into();
        assert!(custom_from_value(&later).unwrap().images.background.is_none());
        // Written back after a change (name, corner moved): the unread slots and the unknown field stay as they were
        let changed = CustomTheme { name: "Later 2".into(), images: ThemeImages { corner: Some(CornerImage { corner: "left top".into(), ..t.images.corner.clone().unwrap() }), ..t.images.clone() }, ..t.clone() };
        let back = custom_to_value(Some(&entry), &changed);
        assert_eq!(back["name"], "Later 2");
        assert_eq!(back["tags"], serde_json::json!(["mine"]));
        assert_eq!(back["images"]["background"], entry["images"]["background"]);
        assert_eq!(back["images"]["frame"], entry["images"]["frame"]);
        assert_eq!(back["images"]["corner"]["corner"], "left top");
        // A slot this version reads and the user removed stays removed
        let removed = custom_to_value(Some(&entry), &CustomTheme { images: ThemeImages::default(), ..t.clone() });
        assert!(removed["images"].get("corner").is_none() && removed["images"].get("background").is_some());
        // A theme this version can't use at all (a later base) isn't read
        let mut unknown_base = entry.clone();
        unknown_base["base"] = "monokai".into();
        assert!(custom_from_value(&unknown_base).is_none());
        // A new theme has no old entry
        let fresh = custom_to_value(None, &CustomTheme { id: "custom-2".into(), name: "New".into(), base: "light".into(), ..Default::default() });
        assert_eq!(fresh, serde_json::json!({ "id": "custom-2", "name": "New", "base": "light", "colors": {} }));
    }


    #[test]
    fn the_background_spans_the_editor_area_unless_saved_per_pane() {
        let slot = |extra: &str| serde_json::from_str::<serde_json::Value>(&format!(r#"{{"id": "custom-1", "name": "A", "base": "dark", "images": {{"background": {{"file": "0123456789abcdef.png", "opacity": 15, "fill": "cover", "anchor": "center"{extra}}}}}}}"#)).unwrap();
        // Saved without it (0.3.5-beta.1): the whole area
        assert_eq!(custom_from_value(&slot("")).unwrap().images.background.unwrap().span, "area");
        let pane = custom_from_value(&slot(r#", "span": "pane""#)).unwrap();
        assert_eq!(pane.images.background.as_ref().unwrap().span, "pane");
        // Written back as read, and kept by the settings.json round trip
        let back = custom_to_value(Some(&slot(r#", "span": "pane""#)), &pane);
        assert_eq!(back["images"]["background"]["span"], "pane");
        assert_eq!(custom_from_value(&back).unwrap(), pane);
        // An unknown span: the slot is left out (and kept in the file), the theme read
        let later = slot(r#", "span": "window""#);
        let t = custom_from_value(&later).unwrap();
        assert!(t.images.background.is_none());
        assert_eq!(custom_to_value(Some(&later), &t)["images"]["background"]["span"], "window");
        assert!(check(&CustomTheme { images: ThemeImages { background: Some(BackgroundImage { span: "window".into(), ..pane.images.background.clone().unwrap() }), corner: None }, ..pane.clone() }).is_err());
    }

}
