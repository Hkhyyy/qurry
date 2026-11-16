use super::*;

/// Autocomplete list: below the cursor (above if there is no room)
pub(super) fn draw_completion(f: &mut Frame, cmp: &crate::app::Completion, x: u16, y: u16) {
    use qurry_core::complete::ItemKind;
    use unicode_width::UnicodeWidthStr;
    let screen = f.area();
    let shown = cmp.items.len().min(10);
    let lw = cmp.items.iter().map(|i| i.label.width()).max().unwrap_or(0);
    let dw = cmp.items.iter().map(|i| i.detail.width()).max().unwrap_or(0);
    let w = ((2 + lw + 2 + dw + 2) as u16).clamp(20, 70).min(screen.width);
    let h = shown as u16 + 2;
    let x = x.saturating_sub(cmp.prefix as u16 + 3).min(screen.right().saturating_sub(w));
    let y = if y + 1 + h <= screen.bottom() { y + 1 } else { y.saturating_sub(h) };
    let r = Rect { x, y, width: w, height: h.min(screen.height) };
    let off = cmp.sel.saturating_sub(shown.saturating_sub(1));
    let items: Vec<ListItem> = cmp
        .items
        .iter()
        .enumerate()
        .skip(off)
        .take(shown)
        .map(|(i, it)| {
            let (glyph, color) = match it.kind {
                ItemKind::Column => ("·", C_SEQUENCE),
                ItemKind::Table => ("▦", C_DB),
                ItemKind::Schema => ("◇", Color::Gray),
                ItemKind::Keyword => ("k", C_KEYWORD),
                ItemKind::Value => ("=", crate::kit::NUMBER),
                ItemKind::Join => ("⋈", crate::kit::C_VIEW),
                ItemKind::Function => ("ƒ", crate::kit::C_ROUTINE),
                ItemKind::Snippet => ("»", ACCENT),
            };
            ListItem::new(Row::new(format!("{:<lw$} ", it.label)).lead(icon(glyph, color)).hint(it.detail.clone()).selected(i == cmp.sel, true).line())
        })
        .collect();
    clear(f, r);
    let title = format!(" {}/{} ", cmp.sel + 1, cmp.items.len());
    f.render_widget(List::new(items).block(panel(title, true)).style(Style::default().bg(POPUP_BG)), r);
}
