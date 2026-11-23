//! The main window reopens with the frame it had (#206). tauri-plugin-window-state saves it on quit in the config folder
//! (`.window-state.json`: per window its size, position and maximized / fullscreen flags, nothing else) and restores
//! maximized / fullscreen. Size and position are placed here instead, because the plugin
//! - sets the saved size even when the display it was on is gone, so the window can come back larger than the display left,
//!   and keeps the position when only a corner of it is on a display;
//! - saves physical pixels in the scale of the display the window was on but sets them in the scale of the display it
//!   opens on, so between a Retina and a 1x display it would open on the wrong one at half or double the size.

use tauri::Manager;
use tauri_plugin_window_state::{AppHandleExt, StateFlags, WindowExt};

/// What the plugin saves: not visibility or decorations, so a saved state can never hide the window
pub(crate) const FLAGS: StateFlags = StateFlags::SIZE.union(StateFlags::POSITION).union(StateFlags::MAXIMIZED).union(StateFlags::FULLSCREEN);

/// A rectangle: x, y, width, height
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Frame {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

/// A display: its work area (without the menu bar and the Dock) in logical points, and its scale
#[derive(Debug, Clone, Copy)]
pub(crate) struct Display {
    pub area: Frame,
    pub scale: f64,
}

fn overlap(a: Frame, b: Frame) -> u64 {
    let w = (a.x + a.w as i32).min(b.x + b.w as i32) - a.x.max(b.x);
    let h = (a.y + a.h as i32).min(b.y + b.h as i32) - a.y.max(b.y);
    if w <= 0 || h <= 0 { 0 } else { w as u64 * h as u64 }
}

fn scaled(f: Frame, scale: f64, deco: (u32, u32)) -> Frame {
    let v = |n: f64| (n / scale).round();
    Frame { x: v(f.x as f64) as i32, y: v(f.y as f64) as i32, w: v(f.w as f64) as u32 + deco.0, h: v(f.h as f64) as u32 + deco.1 }
}

/// Where the saved frame opens, in logical points (outer frame). `saved` is as the plugin saved it: the outer position and
/// the inner size in physical pixels of the display the window was on; `deco` is the title bar's share in points.
/// That display is the one the frame, read in its scale, lies on the most; none (it is gone) → `primary`, else the first.
/// The frame is then moved, and shrunk if needed, to lie inside that display's work area. `None` without displays
pub(crate) fn place(saved: Frame, deco: (u32, u32), displays: &[Display], primary: Option<Display>) -> Option<Frame> {
    let share = |d: &Display| {
        let f = scaled(saved, d.scale, deco);
        (overlap(f, d.area) as f64 / (f.w as f64 * f.h as f64).max(1.0), f, d.area)
    };
    let best = displays.iter().map(share).filter(|(s, ..)| *s > 0.0).max_by(|a, b| a.0.total_cmp(&b.0));
    let (_, frame, area) = best.or_else(|| primary.or_else(|| displays.first().copied()).map(|d| share(&d)))?;
    let w = frame.w.min(area.w);
    let h = frame.h.min(area.h);
    let x = frame.x.clamp(area.x, area.x + (area.w - w) as i32);
    let y = frame.y.clamp(area.y, area.y + (area.h - h) as i32);
    Some(Frame { x, y, w, h })
}

#[derive(serde::Deserialize)]
struct Saved {
    width: u32,
    height: u32,
    x: i32,
    y: i32,
    // The position before maximizing: what the plugin restores for a maximized window
    #[serde(default)]
    prev_x: i32,
    #[serde(default)]
    prev_y: i32,
    #[serde(default)]
    maximized: bool,
}

/// Put `window` where it was saved, fitted to the displays connected now, then maximize it or make it fullscreen again.
/// Nothing saved, or any error: the window stays where Tauri put it
pub(crate) fn restore<R: tauri::Runtime>(window: &tauri::WebviewWindow<R>) {
    let _ = place_window(window);
    let _ = window.restore_state(StateFlags::MAXIMIZED.union(StateFlags::FULLSCREEN));
}

fn place_window<R: tauri::Runtime>(window: &tauri::WebviewWindow<R>) -> Result<(), Box<dyn std::error::Error>> {
    let app = window.app_handle();
    let file = std::fs::read(app.path().app_config_dir()?.join(app.filename()))?;
    let mut all: std::collections::HashMap<String, Saved> = serde_json::from_slice(&file)?;
    let Some(s) = all.remove(window.label()).filter(|s| s.width > 0 && s.height > 0) else { return Ok(()) };
    let (x, y) = if s.maximized { (s.prev_x, s.prev_y) } else { (s.x, s.y) };
    let display = |m: &tauri::Monitor| {
        let (r, scale) = (m.work_area(), m.scale_factor());
        let p = r.position.to_logical::<f64>(scale);
        let z = r.size.to_logical::<f64>(scale);
        Display { area: Frame { x: p.x.round() as i32, y: p.y.round() as i32, w: z.width.round() as u32, h: z.height.round() as u32 }, scale }
    };
    let displays: Vec<Display> = window.available_monitors()?.iter().map(display).collect();
    let primary = window.primary_monitor()?.as_ref().map(display);
    // The title bar's share, from the window as Tauri made it (same decorations)
    let scale = window.scale_factor()?;
    let (outer, inner) = (window.outer_size()?.to_logical::<f64>(scale), window.inner_size()?.to_logical::<f64>(scale));
    let deco = ((outer.width - inner.width).max(0.0).round() as u32, (outer.height - inner.height).max(0.0).round() as u32);
    let Some(f) = place(Frame { x, y, w: s.width, h: s.height }, deco, &displays, primary) else { return Ok(()) };
    // Logical points go through as they are, whatever display the window is on now; minWidth / minHeight still apply
    window.set_size(tauri::LogicalSize::new(f.w.saturating_sub(deco.0), f.h.saturating_sub(deco.1)))?;
    window.set_position(tauri::LogicalPosition::new(f.x, f.y))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn f(x: i32, y: i32, w: u32, h: u32) -> Frame {
        Frame { x, y, w, h }
    }
    const DECO: (u32, u32) = (0, 28);
    // Work areas of a Retina laptop display (below the menu bar) and a 1x external one to its right
    const LAPTOP: Display = Display { area: f(0, 25, 1512, 957), scale: 2.0 };
    const EXTERNAL: Display = Display { area: f(1512, 0, 2560, 1440), scale: 1.0 };
    const BOTH: [Display; 2] = [LAPTOP, EXTERNAL];

    #[test]
    fn a_frame_on_the_laptop_comes_back_there_in_points() {
        // Saved at (100, 100) with a 1200×800 content in points: physical pixels are twice that
        assert_eq!(place(f(200, 200, 2400, 1600), DECO, &BOTH, Some(LAPTOP)), Some(f(100, 100, 1200, 828)));
    }

    #[test]
    fn a_frame_on_the_1x_display_comes_back_there_and_not_on_the_laptop() {
        // (3000, 100) and 1400×900 on the 1x display: read in the laptop's scale it would be (1500, 50), on the laptop's edge
        assert_eq!(place(f(3000, 100, 1400, 900), DECO, &BOTH, Some(LAPTOP)), Some(f(2672, 100, 1400, 928)));
    }

    #[test]
    fn a_frame_on_a_display_that_is_gone_opens_on_the_primary_one() {
        // Saved on a 2x display left of the laptop, now unplugged
        assert_eq!(place(f(-3000, 200, 2400, 1600), DECO, &BOTH, Some(LAPTOP)), Some(f(0, 100, 1200, 828)));
    }

    #[test]
    fn a_frame_larger_than_the_display_left_shrinks_to_it() {
        // 2400×1300 at (1600, 100) on the 1x external display, which is gone: read in the laptop's scale it lies on the laptop
        assert_eq!(place(f(1600, 100, 2400, 1300), DECO, &[LAPTOP], Some(LAPTOP)), Some(f(312, 50, 1200, 678)));
        let small = Display { area: f(0, 25, 1280, 775), scale: 1.0 };
        assert_eq!(place(f(1600, 100, 2400, 1300), DECO, &[small], Some(small)), Some(f(0, 25, 1280, 775)));
    }

    #[test]
    fn a_frame_partly_off_screen_is_pulled_onto_the_display_it_lies_on_most() {
        // Hanging off the laptop's left and top edges
        assert_eq!(place(f(-600, -100, 2400, 1600), DECO, &BOTH, Some(LAPTOP)), Some(f(0, 25, 1200, 828)));
        // Its right part beyond the external display
        assert_eq!(place(f(3500, 100, 1000, 800), DECO, &BOTH, Some(LAPTOP)), Some(f(3072, 100, 1000, 828)));
    }

    #[test]
    fn without_a_primary_display_the_first_one_is_used() {
        assert_eq!(place(f(-9000, 0, 1440, 900), DECO, &[EXTERNAL], None), Some(f(1512, 0, 1440, 928)));
    }

    #[test]
    fn no_display_places_nothing() {
        assert_eq!(place(f(0, 0, 1440, 900), DECO, &[], None), None);
    }
}
