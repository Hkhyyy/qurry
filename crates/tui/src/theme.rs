//! Color mode: what the terminal can show, decided once at startup from NO_COLOR · COLORTERM · TERM.
//! Screen code keeps drawing with the truecolor tokens in kit/atoms.rs; `apply` maps the finished frame to the mode
//! (nearest of the 256 / 16 palette colors, or no colors at all with reverse video for selections).
use crate::kit::{CELL_EDITED, DROP, SEL, SEL_BLUR};
use ratatui::buffer::Buffer;
use ratatui::style::{Color, Modifier};

/// tui-textarea draws selected text on this background (no input here sets its own selection style)
const TEXTAREA_SEL: Color = Color::LightBlue;

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
            // Short enough for the status text's half of an 120-cell line
            Self::Ansi16 => Some("16 colors: set COLORTERM=truecolor for more if supported"),
            Self::NoColor => Some("NO_COLOR is set: colors off (unset NO_COLOR for colors)"),
        }
    }

    /// Selections are shown in reverse video, not in a color (the grid then marks its cursor inside a range differently)
    pub fn reverses_selection(self) -> bool {
        matches!(self, Self::Ansi16 | Self::NoColor)
    }

    /// Map every cell of a drawn frame to this mode
    pub fn apply(self, buf: &mut Buffer) {
        if self == Self::TrueColor {
            return;
        }
        let mut seen = std::collections::HashMap::new();
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
            // A frame has only a handful of distinct colors: look each up once
            let mut color = |c: Color, bg: bool| *seen.entry((c, bg)).or_insert_with(|| self.color(c, bg));
            let bg = color(cell.bg, true);
            let mut fg = color(cell.fg, false);
            // 16 colors: one text color on any colored background, dark on the hues that are light in common palettes,
            // white on the dark ones (xterm's blue gives black text about 2:1); and text never in its own background's color
            if self == Self::Ansi16 && !matches!(bg, Color::Reset | Color::Black | Color::DarkGray) {
                fg = if matches!(bg, Color::Red | Color::Blue | Color::Magenta) { Color::White } else { Color::Black };
            } else if fg == bg && fg != Color::Reset {
                fg = Color::White;
            }
            cell.fg = fg;
            cell.bg = bg;
            cell.underline_color = color(cell.underline_color, false);
        }
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
    let luma = 0.299 * rf + 0.587 * gf + 0.114 * bf;
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
    use crate::kit::*;
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
        let mut buf = Buffer::empty(Rect::new(0, 0, 1, 1));
        buf[(0, 0)].set_symbol("x").set_style(st);
        mode.apply(&mut buf);
        buf[(0, 0)].clone()
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
}
