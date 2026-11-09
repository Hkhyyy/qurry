//! Color themes of both apps: the built-in palettes and their color tokens (themes.json, which the GUI imports as it is),
//! custom themes (a built-in with some colors changed, saved in settings.json), the contrast checks every theme is held to,
//! and the theme file that Settings › Appearance imports and exports.
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

/// A custom theme (settings.json "customThemes"): a built-in theme and the colors changed from it
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomTheme {
    pub id: String,
    pub name: String,
    pub base: String,
    #[serde(default)]
    pub colors: BTreeMap<String, String>,
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
/// The "qurryTheme" value this version reads and writes
const FILE_VERSION: u64 = 1;
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
    Ok(())
}

/// The file Export… writes
pub fn file_text(t: &CustomTheme) -> String {
    let v = serde_json::json!({ "qurryTheme": FILE_VERSION, "name": t.name, "base": t.base, "colors": t.colors });
    serde_json::to_string_pretty(&v).unwrap_or_default() + "\n"
}

/// A theme file (Import…): {"qurryTheme": 1, "name", "base", "colors": {token: "#rrggbb"}} and nothing else. The theme comes back
/// without an id (the caller gives it one when it is saved)
pub fn parse_file(text: &str) -> Result<CustomTheme, String> {
    if text.len() > FILE_MAX {
        return Err(format!("Not a Qurry theme: the file is {} KB, a theme file is at most {} KB", text.len().div_ceil(1024), FILE_MAX / 1024));
    }
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields, rename_all = "camelCase")]
    struct File {
        qurry_theme: u64,
        name: String,
        base: String,
        colors: BTreeMap<String, String>,
    }
    let f: File = serde_json::from_str(text).map_err(|e| format!("Not a Qurry theme file ({e})"))?;
    if f.qurry_theme != FILE_VERSION {
        return Err(format!("This theme file is version {} — this Qurry reads version {FILE_VERSION}: update Qurry", f.qurry_theme));
    }
    let t = CustomTheme { id: String::new(), name: f.name.trim().to_string(), base: f.base, colors: f.colors.into_iter().map(|(k, v)| (k, v.to_ascii_lowercase())).collect() };
    check(&t)?;
    Ok(t)
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
        }
    }

    #[test]
    fn a_low_contrast_pair_fails_the_check() {
        let mut colors = builtin("dark").unwrap().colors.clone();
        colors.insert("comment".into(), "#3a3c40".into());
        colors.insert("onAccent".into(), "#3574f0".into());
        let w = warnings(&colors);
        assert_eq!(w.iter().map(|w| w.token.as_str()).collect::<Vec<_>>(), ["onAccent", "comment"]);
        assert!(w[1].ratio < 3.0 && w[1].min == 3.0 && w[1].fg == "comment" && w[1].bg == "panel2", "{w:?}");
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
        let s = ThemeSettings { custom: vec![CustomTheme { id: "c1".into(), name: "Dim".into(), base: "dracula".into(), colors: [("text".into(), "#44475a".into())].into() }], ..Default::default() };
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
        let custom = CustomTheme { id: "c-1".into(), name: "Night".into(), base: "dracula".into(), colors: [("bg".into(), "#101010".into()), ("nope".into(), "#ffffff".into())].into() };
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
        let t = CustomTheme { id: "c-9".into(), name: "Paper".into(), base: "solarized-light".into(), colors: [("text".into(), "#3d4f55".into())].into() };
        let back = parse_file(&file_text(&t)).unwrap();
        assert_eq!(back, CustomTheme { id: String::new(), ..t.clone() });
        // Upper-case hex is read, stored lower case
        assert_eq!(parse_file(r##"{"qurryTheme": 1, "name": "A", "base": "dark", "colors": {"bg": "#ABCDEF"}}"##).unwrap().colors["bg"], "#abcdef");
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
        refused(r##"{"qurryTheme": 2, "name": "A", "base": "dark", "colors": {}}"##, "update Qurry");
        refused(r##"{"qurryTheme": 1, "name": "A", "base": "dark", "colors": {}, "script": "x"}"##, "unknown field");
        refused(r##"{"name": "A", "base": "dark", "colors": {}}"##, "Not a Qurry theme file");
        refused(r##"{"qurryTheme": 1, "name": "A", "base": "dark", "colors": {"bg": 5}}"##, "Not a Qurry theme file");
        refused("[]", "Not a Qurry theme file");
        refused(&format!(r##"{{"qurryTheme": 1, "name": "{}", "base": "dark", "colors": {{}}}}"##, "x".repeat(FILE_MAX)), "at most 16 KB");
        refused(&format!(r##"{{"qurryTheme": 1, "name": "{}", "base": "dark", "colors": {{}}}}"##, "x".repeat(65)), "1 to 64");
    }
}
