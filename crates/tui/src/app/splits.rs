use super::*;

impl App {
    /// Unsplit: all tabs into one pane
    pub fn unsplit(&mut self) {
        for t in &mut self.tabs {
            t.group = 0;
        }
        self.panes = crate::panes::Pane::Leaf(0);
        self.group_active = [None; crate::panes::MAX];
        self.group_active[0] = self.tabs.get(self.active).map(|t| t.id);
    }

    /// Next/previous tab (within the focused pane)
    fn cycle_tab(&mut self, d: isize) {
        let g = self.focused_group();
        let idx: Vec<usize> = (0..self.tabs.len()).filter(|&i| self.tabs[i].group == g).collect();
        let Some(p) = idx.iter().position(|&i| i == self.active) else { return };
        let n = idx.len() as isize;
        self.activate(idx[(p as isize + d).rem_euclid(n) as usize]);
    }

    /// Split the focused pane (up to 4 panes): move the current tab into the new pane (right/below)
    fn split_focused(&mut self, dir: SplitDir) {
        let Some(cur) = self.tabs.get(self.active).map(|t| t.id) else {
            self.status = "Open a tab to split".into();
            return;
        };
        if self.panes.leaves().len() >= crate::panes::MAX {
            self.status = format!("Up to {} panes — close or move a tab to merge one", crate::panes::MAX);
            return;
        }
        let g = self.focused_group();
        if self.tabs.iter().filter(|t| t.group == g).count() >= 2 {
            return self.drop_tab(cur, DropTarget::NewSplit(g, dir));
        }
        // If the pane has only one tab, open a new console on the same DB in the right/lower pane
        let (conn, db) = (self.tabs[self.active].conn.clone(), self.tabs[self.active].db.clone());
        if self.kind_of(&conn) == DbKind::Redis {
            self.status = "Open another tab to split".into();
            return;
        }
        self.open_console(&conn, Some(db));
        let id = self.tabs[self.active].id;
        self.drop_tab(id, DropTarget::NewSplit(g, dir));
    }

    /// Database Explorer width: 1/4 of the screen width if unset (26~48)
    pub fn explorer_width(&self, total: u16) -> u16 {
        let auto = (total / 4).clamp(26, 48);
        self.explorer_w.unwrap_or(auto).clamp(12, total.saturating_sub(20).max(12))
    }

    pub fn set_explorer_width(&mut self, w: i16) {
        let max = self.hits.main.width.saturating_sub(20).max(12) as i16;
        self.explorer_w = Some(w.clamp(12, max) as u16);
    }

    /// Adjust the console editor/result ratio
    /// To the adjacent pane: axis = Down means up(back)/down, Right means left(back)/right.
    /// Based on the last drawn pane positions, the nearest overlapping pane in that direction
    fn focus_pane(&mut self, axis: SplitDir, back: bool) {
        let cur = self.focused_group();
        // (pane, left, right, top, bottom)
        let rect = |&(g, bar, body): &(usize, ratatui::layout::Rect, ratatui::layout::Rect)| (g, bar.x, bar.x + bar.width, bar.y, body.y + body.height);
        let Some((_, x0, x1, top, bottom)) = self.hits.groups.iter().map(rect).find(|r| r.0 == cur) else { return };
        let next = self
            .hits
            .groups
            .iter()
            .map(rect)
            .filter_map(|(g, a0, a1, t, b)| {
                // Dividers (borders) may overlap by about one cell, so allow 1 cell of slack
                let gap = match (axis, back) {
                    (SplitDir::Down, true) if a0 < x1 && x0 < a1 && b <= top + 1 => top.saturating_sub(b),
                    (SplitDir::Down, false) if a0 < x1 && x0 < a1 && t + 1 >= bottom => t.saturating_sub(bottom),
                    (SplitDir::Right, true) if t < bottom && top < b && a1 <= x0 + 1 => x0.saturating_sub(a1),
                    (SplitDir::Right, false) if t < bottom && top < b && a0 + 1 >= x1 => a0.saturating_sub(x1),
                    _ => return None,
                };
                (g != cur).then_some((gap, if axis == SplitDir::Down { a0.abs_diff(x0) } else { t.abs_diff(top) }, g))
            })
            .min()
            .map(|r| r.2);
        let side = match (axis, back) {
            (SplitDir::Down, true) => "above",
            (SplitDir::Down, false) => "below",
            (SplitDir::Right, true) => "on the left",
            (SplitDir::Right, false) => "on the right",
        };
        match next.and_then(|g| self.group_tab(g)) {
            Some(i) => self.activate(i),
            None => self.status = format!("No split {side}"),
        }
    }

    pub fn resize_console(&mut self, d: i16) {
        self.console_pct = (self.console_pct as i16 + d).clamp(10, 90) as u16;
    }
}
