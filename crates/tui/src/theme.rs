//! Color mode: what the terminal can show, decided once at startup from NO_COLOR · COLORTERM · TERM. Theme: dark or light
//! colors (⌃k › Theme…; Auto follows COLORFGBG).
//! Screen code keeps drawing with the dark truecolor tokens in kit/atoms.rs; `apply` maps the finished frame to the light palette
//! when it is on, then to the mode (nearest of the 256 / 16 palette colors, or no colors at all with reverse video for selections).
use crate::kit::*;
use ratatui::buffer::Buffer;
use ratatui::style::{Color, Modifier};

/// tui-textarea draws selected text on this background (no input here sets its own selection style)
const TEXTAREA_SEL: Color = Color::LightBlue;

/// The Theme setting (settings.json "tuiTheme": the shared file, but the TUI's own key — the GUI has its own theme)
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Theme {
    /// Light when the terminal says its background is light (COLORFGBG), dark otherwise
    #[default]
    Auto,
    Dark,
    Light,
}

impl Theme {
    pub const ALL: [Theme; 3] = [Theme::Auto, Theme::Dark, Theme::Light];

    pub fn name(self) -> &'static str {
        match self {
            Self::Auto => "Auto",
            Self::Dark => "Dark",
            Self::Light => "Light",
        }
    }

    /// The saved value is the name in lower case; anything else is Auto
    pub fn parse(s: Option<&str>) -> Self {
        Self::ALL.into_iter().find(|t| s == Some(t.name().to_lowercase().as_str())).unwrap_or_default()
    }

    /// Whether to draw the light palette; auto = what Auto picks for this terminal (`auto_light`)
    pub fn light(self, auto: bool) -> bool {
        match self {
            Self::Auto => auto,
            Self::Dark => false,
            Self::Light => true,
        }
    }
}

/// Auto's pick: COLORFGBG ("fg;bg", some terminals "fg;default;bg") with background 7 or 15 (white) is a light terminal;
/// anything else, or unset, is dark
pub fn auto_light(var: impl Fn(&str) -> Option<String>) -> bool {
    var("COLORFGBG").is_some_and(|v| matches!(v.rsplit(';').next(), Some("7" | "15")))
}

/// The light palette: each dark token of kit/atoms.rs and its light value (GUI tokens.css light theme where it has one).
/// C_SEQUENCE has WARN's value, so WARN's entry covers it
const LIGHT: [(Color, Color); 21] = [
    (ACCENT, Color::Rgb(0x35, 0x74, 0xf0)),
    (SEL, Color::Rgb(0xd5, 0xe1, 0xff)),
    (SEL_BLUR, Color::Rgb(0xd0, 0xd3, 0xda)),
    (SURFACE, Color::Rgb(0xeb, 0xec, 0xf0)),
    (POPUP_BG, Color::Rgb(0xf7, 0xf8, 0xfa)),
    (DROP, Color::Rgb(0xb4, 0xc8, 0xf5)),
    (OK, Color::Rgb(0x20, 0x8a, 0x3c)),
    (WARN, Color::Rgb(0xb0, 0x6f, 0x00)),
    (DANGER, Color::Rgb(0xd1, 0x38, 0x4a)),
    (NUMBER, Color::Rgb(0x17, 0x50, 0xeb)),
    (GRID_LINE, Color::Rgb(0xc9, 0xcc, 0xd6)),
    (CELL_EDITED, Color::Rgb(0xb0, 0xc8, 0xff)),
    (CELL_RANGE, Color::Rgb(0xe4, 0xe8, 0xf0)),
    (JSON_KEY, Color::Rgb(0x87, 0x10, 0x94)),
    (JSON_STRING, Color::Rgb(0x06, 0x7d, 0x17)),
    (JSON_KEYWORD, Color::Rgb(0x00, 0x33, 0xb3)),
    (JSON_PUNCT, Color::Rgb(0x6c, 0x70, 0x7e)),
    (C_DB, Color::Rgb(0x20, 0x78, 0xaa)),
    (C_VIEW, Color::Rgb(0x4a, 0x8a, 0x35)),
    (C_ROUTINE, Color::Rgb(0x25, 0x63, 0xc9)),
    (C_KEYWORD, Color::Rgb(0xb3, 0x5a, 0x00)),
];

/// One color in the light palette. Tokens take their light value; other dark backgrounds (Data Source colors and their lighter
/// variant on the active tab) keep their hue with the lightness turned over, so a Data Source tint stays a tint and the active tab
/// the stronger one. Named text colors meant for a dark background get a dark counterpart
pub fn to_light(c: Color, bg: bool) -> Color {
    if let Some(&(_, l)) = LIGHT.iter().find(|t| t.0 == c) {
        return l;
    }
    match (c, bg) {
        (Color::Rgb(r, g, b), true) if luma(r, g, b) < 128.0 => {
            // HSL lightness L → 1 − L with hue and saturation kept: every channel moves by 255 − max − min
            let shift = 255 - r.max(g).max(b) as i16 - r.min(g).min(b) as i16;
            let m = |v: u8| (v as i16 + shift).clamp(0, 255) as u8;
            Color::Rgb(m(r), m(g), m(b))
        }
        (Color::White, false) => Color::Black,
        (Color::Gray, false) => Color::Rgb(0x50, 0x53, 0x5c),
        (Color::Yellow, false) => to_light(WARN, false),
        (Color::Cyan, false) => Color::Rgb(0x00, 0x7f, 0x8c),
        _ => c,
    }
}

/// A cell's [fg, bg, underline] in the light palette. Black / White text was picked for its background (kit::on): pick again for the new one
fn light_cell([fg, bg, ul]: [Color; 3]) -> [Color; 3] {
    let bg = to_light(bg, true);
    let fg = match (fg, bg) {
        (Color::Black | Color::White, Color::Rgb(..)) => on(bg),
        _ => to_light(fg, false),
    };
    [fg, bg, to_light(ul, false)]
}

fn luma(r: u8, g: u8, b: u8) -> f32 {
    0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ColorMode {
    /// 24-bit colors (COLORTERM=truecolor / 24bit)
    #[default]
    TrueColor,
    /// xterm 256-color palette (TERM=…-256color)
    Ansi256,
    /// The 16 terminal colors (any other TERM)
    Ansi16,
    /// No colors (NO_COLOR set): bold · reverse · dim only
    NoColor,
}

impl ColorMode {
    pub fn from_env() -> Self {
        Self::detect(|k| std::env::var(k).ok())
    }

    /// NO_COLOR (any non-empty value, see no-color.org) wins, then COLORTERM, then TERM
    pub fn detect(var: impl Fn(&str) -> Option<String>) -> Self {
        let var = |k| var(k).filter(|v| !v.is_empty());
        if var("NO_COLOR").is_some() {
            return Self::NoColor;
        }
        if var("COLORTERM").is_some_and(|v| v == "truecolor" || v == "24bit") {
            return Self::TrueColor;
        }
        match var("TERM") {
            // Also terminals known for 24-bit colors, whose COLORTERM is lost over ssh or sudo
            Some(t) if t.ends_with("-direct") || ["xterm-kitty", "alacritty", "wezterm", "foot", "xterm-ghostty"].contains(&t.as_str()) => Self::TrueColor,
            Some(t) if t.contains("256color") => Self::Ansi256,
            _ => Self::Ansi16,
        }
    }

    /// Status line text at startup when the terminal gets few or no colors, saying how to get them back
    pub fn notice(self) -> Option<&'static str> {
        match self {
            Self::TrueColor | Self::Ansi256 => None,
            // Short enough for the status text's half of an 80-cell line
            Self::Ansi16 => Some("16 colors: try COLORTERM=truecolor"),
            Self::NoColor => Some("Colors off: unset NO_COLOR"),
        }
    }

    /// Selections are shown in reverse video, not in a color (the grid then marks its cursor inside a range differently)
    pub fn reverses_selection(self) -> bool {
        matches!(self, Self::Ansi16 | Self::NoColor)
    }

    /// Map every cell of a drawn frame to the light palette (light) and to this mode
    pub fn apply(self, buf: &mut Buffer, light: bool) {
        if self == Self::TrueColor && !light {
            return;
        }
        // A frame has only a handful of distinct colors, mostly in runs of equal cells: map each color once and
        // reuse the previous cell's result while the colors don't change
        let mut seen = Vec::new();
        let mut last: Option<([Color; 3], [Color; 3])> = None;
        for cell in buf.content.iter_mut() {
            // Terminal themes give the 16 colors very different brightness; reverse video reads in all of them
            let selection = self.reverses_selection() && [SEL, DROP, CELL_EDITED].contains(&cell.bg);
            let reverse = selection
                || (self == Self::NoColor
                    // Also the unfocused selection (the thick border tells where the focus is), text selected in an input
                    // (tui-textarea's default selection style) and badges (dark text on a light background)
                    && (cell.bg == SEL_BLUR || cell.bg == TEXTAREA_SEL || (cell.bg != Color::Reset && cell.fg == Color::Black)));
            if reverse || self == Self::NoColor {
                let mut m = cell.modifier;
                if reverse {
                    m |= Modifier::REVERSED;
                }
                if self == Self::NoColor && cell.fg == Color::DarkGray {
                    m |= Modifier::DIM;
                }
                cell.set_style(ratatui::style::Style::reset().add_modifier(m));
                cell.underline_color = Color::Reset;
                continue;
            }
            let key = [cell.fg, cell.bg, cell.underline_color];
            let [fg, bg, ul] = match last {
                Some((k, v)) if k == key => v,
                _ => {
                    let v = self.map_colors(if light { light_cell(key) } else { key }, &mut seen, light);
                    last = Some((key, v));
                    v
                }
            };
            cell.fg = fg;
            cell.bg = bg;
            cell.underline_color = ul;
        }
    }

    /// A cell's [fg, bg, underline] in this mode. RGB colors go through `seen`, the frame's colors already mapped
    /// (a short list: a frame has a few dozen at most); named colors need no lookup
    fn map_colors(self, [fg, bg, ul]: [Color; 3], seen: &mut Vec<(Color, bool, Color)>, light: bool) -> [Color; 3] {
        if self == Self::TrueColor {
            return [fg, bg, ul];
        }
        let mut color = |c: Color, bg: bool| {
            if !matches!(c, Color::Rgb(..)) {
                return self.color(c, bg);
            }
            // Only the 16-color mapping depends on whether it is a background
            let bg = bg && self == Self::Ansi16;
            match seen.iter().find(|e| e.0 == c && e.1 == bg) {
                Some(e) => e.2,
                None => {
                    let m = match (self, c) {
                        // The light palette keeps the normal of the 16 colors: light terminals make their bright ones dark or pale
                        // at will, while the normal ones stay mid-tones (text on them is picked below as in the dark palette)
                        (Self::Ansi16, Color::Rgb(r, g, b)) if light => normal16(ansi16(r, g, b, bg)),
                        _ => self.color(c, bg),
                    };
                    seen.push((c, bg, m));
                    m
                }
            }
        };
        let bg = color(bg, true);
        let mut fg = color(fg, false);
        // 16 colors: one text color on any colored background, dark on the hues that are light in common palettes,
        // white on the dark ones (xterm's blue gives black text about 2:1); and text never in its own background's color.
        // Surfaces (gray backgrounds) keep the text color: dark grays in the dark palette, light ones in the light palette
        let surfaces = if light { [Color::Reset, Color::White, Color::Gray] } else { [Color::Reset, Color::Black, Color::DarkGray] };
        if self == Self::Ansi16 && !surfaces.contains(&bg) {
            fg = if matches!(bg, Color::Red | Color::Blue | Color::Magenta) { Color::White } else { Color::Black };
        } else if fg == bg && fg != Color::Reset {
            fg = if light { Color::Black } else { Color::White };
        }
        [fg, bg, color(ul, false)]
    }

    /// One color in this mode (named terminal colors are already fine everywhere except NoColor)
    pub fn color(self, c: Color, bg: bool) -> Color {
        match (self, c) {
            (Self::NoColor, _) => Color::Reset,
            // The grid's range is nearest to the selection's color: navy keeps the cursor inside it visible
            (Self::Ansi256, CELL_EDITED) => Color::Indexed(17),
            (Self::Ansi256, Color::Rgb(r, g, b)) => Color::Indexed(ansi256(r, g, b)),
            (Self::Ansi16, Color::Rgb(r, g, b)) => ansi16(r, g, b, bg),
            _ => c,
        }
    }
}

/// The RGB value of an xterm 256-palette color: the 6×6×6 cube (16–231) or the gray ramp (232–255)
pub fn rgb256(i: u8) -> (u8, u8, u8) {
    const LEVELS: [u8; 6] = [0, 95, 135, 175, 215, 255];
    match i {
        16..=231 => {
            let n = i - 16;
            (LEVELS[n as usize / 36], LEVELS[n as usize / 6 % 6], LEVELS[n as usize % 6])
        }
        232..=255 => (8 + 10 * (i - 232), 8 + 10 * (i - 232), 8 + 10 * (i - 232)),
        _ => (0, 0, 0),
    }
}

/// Nearest xterm 256-palette color (0–15 are skipped: they depend on the terminal's theme). Distance in brightness and color difference,
/// the latter counted twice so dark tints keep their hue (by plain RGB distance the selection blue turns teal)
pub fn ansi256(r: u8, g: u8, b: u8) -> u8 {
    let yuv = |(r, g, b): (u8, u8, u8)| {
        let y = 0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32;
        (y, b as f32 - y, r as f32 - y)
    };
    let (y, u, v) = yuv((r, g, b));
    let dist = |i: u8| {
        let (y2, u2, v2) = yuv(rgb256(i));
        (y - y2).powi(2) + 2.0 * ((u - u2).powi(2) + (v - v2).powi(2))
    };
    (16..=255).min_by(|a, b| dist(*a).total_cmp(&dist(*b))).unwrap()
}

/// The normal variant of a bright 16-palette color
fn normal16(c: Color) -> Color {
    use Color::*;
    match c {
        LightRed => Red,
        LightYellow => Yellow,
        LightGreen => Green,
        LightCyan => Cyan,
        LightBlue => Blue,
        LightMagenta => Magenta,
        c => c,
    }
}

/// Nearest of the 16 terminal colors by hue, so tinted colors keep their hue (a plain RGB distance turns most of them gray).
/// Grays go by brightness; text takes the bright variant once it isn't dark, backgrounds keep the normal one unless they are light
fn ansi16(r: u8, g: u8, b: u8, bg: bool) -> Color {
    use Color::*;
    // Unfocused selection is a gray barely lighter than the surfaces around it: keep it visible
    if Color::Rgb(r, g, b) == SEL_BLUR {
        return DarkGray;
    }
    let (rf, gf, bf) = (r as f32, g as f32, b as f32);
    let (max, min) = (rf.max(gf).max(bf), rf.min(gf).min(bf));
    let luma = luma(r, g, b);
    if max - min < 24.0 {
        return match luma {
            l if l < 64.0 => Black,
            l if l < 150.0 => DarkGray,
            l if l < 215.0 => Gray,
            _ => White,
        };
    }
    let d = max - min;
    let hue = if max == rf {
        60.0 * ((gf - bf) / d).rem_euclid(6.0)
    } else if max == gf {
        60.0 * ((bf - rf) / d + 2.0)
    } else {
        60.0 * ((rf - gf) / d + 4.0)
    };
    let bright = if bg { luma >= 160.0 } else { luma >= 100.0 };
    let i = (((hue + 30.0) / 60.0) as usize) % 6;
    let (normal, light) = [(Red, LightRed), (Yellow, LightYellow), (Green, LightGreen), (Cyan, LightCyan), (Blue, LightBlue), (Magenta, LightMagenta)][i];
    if bright {
        light
    } else {
        normal
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::layout::Rect;
    use ratatui::style::Style;

    fn env<'a>(vars: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |k| vars.iter().find(|v| v.0 == k).map(|v| v.1.to_string())
    }

    #[test]
    fn mode_comes_from_no_color_then_colorterm_then_term() {
        use ColorMode::*;
        assert_eq!(ColorMode::detect(env(&[("NO_COLOR", "1"), ("COLORTERM", "truecolor")])), NoColor);
        // An empty NO_COLOR doesn't count (no-color.org)
        assert_eq!(ColorMode::detect(env(&[("NO_COLOR", ""), ("COLORTERM", "truecolor")])), TrueColor);
        assert_eq!(ColorMode::detect(env(&[("COLORTERM", "24bit"), ("TERM", "xterm")])), TrueColor);
        assert_eq!(ColorMode::detect(env(&[("TERM", "xterm-direct")])), TrueColor);
        assert_eq!(ColorMode::detect(env(&[("TERM", "xterm-256color")])), Ansi256);
        assert_eq!(ColorMode::detect(env(&[("TERM", "tmux-256color"), ("COLORTERM", "")])), Ansi256);
        assert_eq!(ColorMode::detect(env(&[("TERM", "xterm")])), Ansi16);
        assert_eq!(ColorMode::detect(env(&[("TERM", "screen")])), Ansi16);
        // Truecolor terminals whose COLORTERM was lost (ssh, sudo)
        for t in ["xterm-kitty", "alacritty", "wezterm", "foot", "xterm-ghostty"] {
            assert_eq!(ColorMode::detect(env(&[("TERM", t)])), TrueColor, "{t}");
        }
        assert_eq!(ColorMode::detect(env(&[])), Ansi16);
    }

    #[test]
    fn notice_says_how_to_get_colors_back() {
        assert_eq!(ColorMode::TrueColor.notice(), None);
        assert!(ColorMode::NoColor.notice().unwrap().contains("unset NO_COLOR"));
        assert_eq!(ColorMode::Ansi256.notice(), None);
        assert!(ColorMode::Ansi16.notice().unwrap().contains("COLORTERM=truecolor"));
    }

    #[test]
    fn rgb_maps_to_the_nearest_256_palette_color() {
        assert_eq!(ansi256(0, 0, 0), 16);
        assert_eq!(ansi256(255, 255, 255), 231);
        assert_eq!(ansi256(95, 135, 175), 67);
        // Near-grays go to the gray ramp, which is finer than the cube
        assert_eq!(ansi256(50, 52, 56), 236);
        assert_eq!(ansi256(30, 31, 34), 234);
        // Dark tints keep their hue: the selection stays blue-gray, the production color red
        assert_eq!(rgb256(ansi256(46, 67, 110)), (95, 95, 135));
        assert_eq!(rgb256(ansi256(0x6e, 0x2a, 0x2a)), (95, 0, 0));
        assert_eq!(rgb256(ansi256(247, 84, 100)), (255, 95, 95));
        // Tokens that must stay apart stay apart
        let i = |c: Color| ColorMode::Ansi256.color(c, true);
        assert_ne!(i(SEL), i(SEL_BLUR));
        assert_ne!(i(SEL_BLUR), i(POPUP_BG));
        assert_ne!(i(SEL), i(CELL_EDITED), "the grid's cursor inside its range");
        assert_ne!(i(SEL_BLUR), i(CELL_EDITED));
        assert_eq!(i(Color::Cyan), Color::Cyan, "named colors are left to the terminal");
    }

    #[test]
    fn rgb_maps_to_the_16_colors_by_hue() {
        let fg = |c: Color| ColorMode::Ansi16.color(c, false);
        let bg = |c: Color| ColorMode::Ansi16.color(c, true);
        assert_eq!(fg(ACCENT), Color::LightBlue);
        assert_eq!(fg(DANGER), Color::LightRed);
        assert_eq!(fg(WARN), Color::LightYellow);
        assert_eq!(fg(OK), Color::LightGreen);
        assert_eq!(fg(NUMBER), Color::LightCyan);
        assert_eq!(bg(SEL), Color::Blue);
        assert_eq!(bg(SURFACE), Color::Black);
        assert_eq!(bg(POPUP_BG), Color::Black);
        assert_eq!(bg(SEL_BLUR), Color::DarkGray, "unfocused selection stays visible next to the surfaces");
        // Data Source colors keep their hue
        assert_eq!(bg(hex("#6e2a2a").unwrap()), Color::Red);
        assert_eq!(bg(hex("#2f5634").unwrap()), Color::Green);
        assert_eq!(bg(hex("#28406e").unwrap()), Color::Blue);
        assert_eq!(bg(hex("#4f2f68").unwrap()), Color::Magenta);
        assert_eq!(bg(hex("#45474d").unwrap()), Color::DarkGray);
    }

    fn cell(mode: ColorMode, st: Style) -> ratatui::buffer::Cell {
        cell_in(mode, false, st)
    }

    fn cell_in(mode: ColorMode, light: bool, st: Style) -> ratatui::buffer::Cell {
        let mut buf = Buffer::empty(Rect::new(0, 0, 1, 1));
        buf[(0, 0)].set_symbol("x").set_style(st);
        mode.apply(&mut buf, light);
        buf[(0, 0)].clone()
    }

    /// A whole frame maps each cell as it would alone: runs of equal cells, colors seen before, and the same color as text and background
    #[test]
    fn whole_frame_maps_like_single_cells() {
        let styles = [
            Style::default().fg(ACCENT).bg(SURFACE),
            Style::default().fg(ACCENT).bg(SURFACE),
            Style::default().fg(SURFACE).bg(ACCENT),
            Style::default().fg(DIM).bg(hex("#6e2a2a").unwrap()),
            Style::default().fg(ACCENT).bg(SURFACE),
            Style::default().fg(Color::Yellow).bg(SEL_BLUR).underline_color(ACCENT),
        ];
        for (mode, light) in [ColorMode::Ansi256, ColorMode::Ansi16, ColorMode::TrueColor].into_iter().flat_map(|m| [(m, false), (m, true)]) {
            let mut buf = Buffer::empty(Rect::new(0, 0, styles.len() as u16, 1));
            for (x, st) in styles.iter().enumerate() {
                buf[(x as u16, 0)].set_symbol("x").set_style(*st);
            }
            mode.apply(&mut buf, light);
            for (x, st) in styles.iter().enumerate() {
                assert_eq!(buf[(x as u16, 0)], cell_in(mode, light, *st), "{mode:?} light {light} cell {x}");
            }
        }
    }

    #[test]
    fn no_color_keeps_selection_and_badges_visible_with_reverse_video() {
        let c = cell(ColorMode::NoColor, Style::default().fg(ACCENT).bg(SEL).add_modifier(Modifier::BOLD));
        assert_eq!((c.fg, c.bg), (Color::Reset, Color::Reset));
        assert!(c.modifier.contains(Modifier::REVERSED | Modifier::BOLD));
        assert!(cell(ColorMode::NoColor, Style::default().bg(SEL_BLUR)).modifier.contains(Modifier::REVERSED));
        // Badge: dark text on a light background
        assert!(cell(ColorMode::NoColor, Style::default().fg(Color::Black).bg(WARN)).modifier.contains(Modifier::REVERSED));
        // Text selected in an input (tui-textarea's own selection style)
        let c = cell(ColorMode::NoColor, tui_textarea::TextArea::default().selection_style());
        assert_eq!((c.fg, c.bg), (Color::Reset, Color::Reset));
        assert!(c.modifier.contains(Modifier::REVERSED));
        // Surfaces and Data Source colors just lose their color
        assert!(cell(ColorMode::NoColor, Style::default().bg(POPUP_BG)).modifier.is_empty());
        assert!(cell(ColorMode::NoColor, Style::default().bg(hex("#6e2a2a").unwrap())).modifier.is_empty());
        // Muted text stays muted
        let c = cell(ColorMode::NoColor, Style::default().fg(DIM));
        assert_eq!(c.fg, Color::Reset);
        assert!(c.modifier.contains(Modifier::DIM));
    }

    #[test]
    fn sixteen_colors_keep_text_readable() {
        // Text on a colored Data Source row turns dark
        let c = cell(ColorMode::Ansi16, Style::default().bg(hex("#5f5b24").unwrap()));
        assert_eq!((c.fg, c.bg), (Color::Black, Color::Yellow));
        // … but white on the hues that are dark in common palettes (xterm's blue gives black text about 2:1)
        for (h, bg) in [("#6e2a2a", Color::Red), ("#28406e", Color::Blue), ("#4f2f68", Color::Magenta)] {
            let c = cell(ColorMode::Ansi16, Style::default().fg(C_DB).bg(hex(h).unwrap()));
            assert_eq!((c.fg, c.bg), (Color::White, bg), "{h}");
        }
        let c = cell(ColorMode::Ansi16, Style::default().fg(C_DB).bg(hex("#2f5634").unwrap()));
        assert_eq!((c.fg, c.bg), (Color::Black, Color::Green));
        // The selection is reverse video, whatever the terminal theme makes of blue
        let c = cell(ColorMode::Ansi16, Style::default().fg(ACCENT).bg(SEL).add_modifier(Modifier::BOLD));
        assert_eq!((c.fg, c.bg), (Color::Reset, Color::Reset));
        assert!(c.modifier.contains(Modifier::REVERSED | Modifier::BOLD));
        // Text on the status line surface keeps its color
        let c = cell(ColorMode::Ansi16, Style::default().fg(ACCENT).bg(SURFACE));
        assert_eq!((c.fg, c.bg), (Color::LightBlue, Color::Black));
        // Muted text on the unfocused selection doesn't vanish into it
        let c = cell(ColorMode::Ansi16, Style::default().fg(DIM).bg(SEL_BLUR));
        assert_eq!((c.fg, c.bg), (Color::White, Color::DarkGray));
        // Truecolor leaves the frame alone
        let c = cell(ColorMode::TrueColor, Style::default().fg(ACCENT).bg(SEL));
        assert_eq!((c.fg, c.bg), (ACCENT, SEL));
    }

    #[test]
    fn auto_theme_follows_colorfgbg_and_the_setting_wins() {
        for (v, light) in [("0;15", true), ("0;7", true), ("0;default;15", true), ("15;0", false), ("7;0", false), ("15;default;0", false), ("0;8", false), ("0;115", false), ("", false)] {
            assert_eq!(auto_light(env(&[("COLORFGBG", v)])), light, "COLORFGBG={v}");
        }
        assert!(!auto_light(env(&[])), "unset: dark, as before");
        assert_eq!(Theme::parse(None), Theme::Auto);
        assert_eq!(Theme::parse(Some("dark")), Theme::Dark);
        assert_eq!(Theme::parse(Some("light")), Theme::Light);
        assert_eq!(Theme::parse(Some("auto")), Theme::Auto);
        assert_eq!(Theme::parse(Some("solarized")), Theme::Auto, "a value this version doesn't know");
        for auto in [false, true] {
            assert_eq!(Theme::Auto.light(auto), auto);
            assert!(!Theme::Dark.light(auto));
            assert!(Theme::Light.light(auto));
        }
    }

    /// WCAG contrast ratio of two RGB colors (or black / white)
    fn contrast(a: Color, b: Color) -> f32 {
        let lum = |c: Color| {
            let (r, g, b) = match c {
                Color::Rgb(r, g, b) => (r, g, b),
                Color::Black => (0, 0, 0),
                Color::White => (255, 255, 255),
                c => panic!("{c:?}"),
            };
            let lin = |v: u8| {
                let v = v as f32 / 255.0;
                if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
            };
            0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b)
        };
        let (x, y) = (lum(a), lum(b));
        (x.max(y) + 0.05) / (x.min(y) + 0.05)
    }

    #[test]
    fn light_palette_keeps_text_readable_in_truecolor() {
        let white = Color::Rgb(255, 255, 255);
        let tc = |st: Style| cell_in(ColorMode::TrueColor, true, st);
        // Every text color token reads on a white terminal, the popups and the selection
        for t in [ACCENT, OK, WARN, DANGER, NUMBER, JSON_KEY, JSON_STRING, JSON_KEYWORD, JSON_PUNCT, C_DB, C_VIEW, C_SEQUENCE, C_ROUTINE, C_KEYWORD] {
            for bg in [white, to_light(POPUP_BG, true), to_light(SURFACE, true), to_light(SEL, true)] {
                assert!(contrast(to_light(t, false), bg) >= 3.0, "{t:?} on {bg:?}: {}", contrast(to_light(t, false), bg));
            }
        }
        // Light surfaces and selections, each one apart from the others
        let bgs = [SEL, SEL_BLUR, SURFACE, POPUP_BG, CELL_EDITED, CELL_RANGE, DROP].map(|c| to_light(c, true));
        for (i, a) in bgs.iter().enumerate() {
            assert!(contrast(*a, white) < 1.8, "{a:?} is a light background");
            assert!(!bgs[i + 1..].contains(a), "{a:?} twice");
        }
        assert_eq!(tc(Style::default().fg(ACCENT).bg(SEL)).bg, to_light(SEL, true));
        // Bright text meant for a dark background turns dark: titles, the edited marker, the focused grid
        assert_eq!(tc(Style::default().fg(Color::White)).fg, Color::Black);
        for c in [Color::Gray, Color::Yellow, Color::Cyan] {
            assert!(contrast(tc(Style::default().fg(c)).fg, white) >= 3.0, "{c:?}");
        }
        // Badges ([ PROD ], update available): their text picked again for the light color
        for bg in [DANGER, WARN] {
            let c = tc(Style::default().fg(on(bg)).bg(bg));
            assert!(contrast(c.fg, c.bg) >= 4.0, "{bg:?}: {c:?}");
        }
        // Production row: a light tint of the same red with the terminal's dark text; the active tab's lighter variant becomes the
        // stronger red, its text picked again for it
        let prod = hex("#6e2a2a").unwrap();
        let row = tc(Style::default().bg(prod)).bg;
        let Color::Rgb(r, g, b) = row else { panic!() };
        assert!(r > g && g == b && contrast(Color::Rgb(0, 0, 0), row) >= 7.0, "{row:?}");
        let tab = tc(Style::default().fg(on(lighten(prod))).bg(lighten(prod)));
        assert!(contrast(tab.bg, white) > contrast(row, white), "the active tab stands out more than the row");
        assert!(contrast(tab.fg, tab.bg) >= 4.5, "{tab:?}");
        // Dark Data Source colors as text (the tree's connection dot) stay as they are: dark already
        assert_eq!(tc(Style::default().fg(prod)).fg, prod);
    }

    #[test]
    fn light_palette_in_256_and_16_colors_and_no_color() {
        let i = |c: Color| ColorMode::Ansi256.color(to_light(c, true), true);
        assert_ne!(i(SEL), i(SEL_BLUR));
        assert_ne!(i(SEL_BLUR), i(POPUP_BG));
        assert_ne!(i(SEL), i(CELL_EDITED), "the grid's cursor inside its range");
        assert_ne!(i(SEL_BLUR), i(CELL_EDITED));
        assert_ne!(i(SURFACE), Color::Indexed(231), "the status line is not the white of the terminal");
        let c = cell_in(ColorMode::Ansi256, true, Style::default().fg(Color::White).bg(CELL_EDITED));
        assert_eq!((c.fg, c.bg), (Color::Black, i(CELL_EDITED)), "edited cell: dark text on a light blue");
        // 16 colors: light surfaces, text in the normal (darker) colors and kept on the surfaces
        let c = cell_in(ColorMode::Ansi16, true, Style::default().fg(ACCENT).bg(SURFACE));
        assert_eq!((c.fg, c.bg), (Color::Blue, Color::White));
        let c = cell_in(ColorMode::Ansi16, true, Style::default().fg(DANGER).bg(SEL_BLUR));
        assert_eq!((c.fg, c.bg), (Color::Red, Color::Gray));
        for (t, want) in [(OK, Color::Green), (WARN, Color::Yellow), (JSON_STRING, Color::Green), (JSON_KEY, Color::Magenta), (NUMBER, Color::Blue)] {
            assert_eq!(cell_in(ColorMode::Ansi16, true, Style::default().fg(t)).fg, want, "{t:?}");
        }
        // A Data Source row keeps its hue in the normal color, text as in the dark palette; the selection is still reverse video
        let c = cell_in(ColorMode::Ansi16, true, Style::default().bg(hex("#6e2a2a").unwrap()));
        assert_eq!((c.fg, c.bg), (Color::White, Color::Red));
        let c = cell_in(ColorMode::Ansi16, true, Style::default().fg(Color::Gray).bg(hex("#2f5634").unwrap()));
        assert_eq!((c.fg, c.bg), (Color::Black, Color::Green));
        let c = cell_in(ColorMode::Ansi16, true, Style::default().fg(ACCENT).bg(SEL));
        assert_eq!((c.fg, c.bg), (Color::Reset, Color::Reset));
        assert!(c.modifier.contains(Modifier::REVERSED));
        // NO_COLOR keeps its monochrome rules whatever the theme
        for st in [Style::default().fg(ACCENT).bg(SEL), Style::default().fg(Color::Black).bg(WARN), Style::default().fg(DIM), Style::default().bg(hex("#6e2a2a").unwrap()), Style::default().fg(Color::White)] {
            assert_eq!(cell_in(ColorMode::NoColor, true, st), cell(ColorMode::NoColor, st), "{st:?}");
        }
    }
}
