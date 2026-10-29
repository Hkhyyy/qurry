//! Editor split tree (the GUI's Split Right / Split Down). Panes (groups) are leaves, splits are branches. At most MAX panes
use crate::app::SplitDir;

pub const MAX: usize = 4;

#[derive(Clone, Debug, PartialEq)]
pub enum Pane {
    /// Pane id (Tab.group)
    Leaf(usize),
    /// pct = share (%) taken by the first (a: left/top)
    Split { dir: SplitDir, pct: u16, a: Box<Pane>, b: Box<Pane> },
}

/// Branch position: the path from the root going a(false)/b(true)
pub type Path = Vec<bool>;

impl Pane {
    /// Pane ids (from left/top)
    pub fn leaves(&self) -> Vec<usize> {
        match self {
            Pane::Leaf(g) => vec![*g],
            Pane::Split { a, b, .. } => [a.leaves(), b.leaves()].concat(),
        }
    }

    pub fn is_split(&self) -> bool {
        matches!(self, Pane::Split { .. })
    }

    /// Splits pane g in direction dir and returns the new pane id (None if already MAX)
    pub fn split(&mut self, g: usize, dir: SplitDir) -> Option<usize> {
        let ls = self.leaves();
        if ls.len() >= MAX || !ls.contains(&g) {
            return None;
        }
        let new = (0..MAX).find(|i| !ls.contains(i))?;
        self.replace_leaf(g, &mut |_| Pane::Split { dir, pct: 50, a: Box::new(Pane::Leaf(g)), b: Box::new(Pane::Leaf(new)) });
        Some(new)
    }

    fn replace_leaf(&mut self, g: usize, f: &mut dyn FnMut(usize) -> Pane) -> bool {
        match self {
            Pane::Leaf(x) if *x == g => {
                *self = f(g);
                true
            }
            Pane::Leaf(_) => false,
            Pane::Split { a, b, .. } => a.replace_leaf(g, f) || b.replace_leaf(g, f),
        }
    }

    /// Removes pane g and the neighbor fills its place (the last pane isn't removed)
    pub fn remove(&mut self, g: usize) {
        if let Pane::Split { a, b, .. } = self {
            if **a == Pane::Leaf(g) {
                *self = (**b).clone();
            } else if **b == Pane::Leaf(g) {
                *self = (**a).clone();
            } else {
                a.remove(g);
                b.remove(g);
            }
        }
    }

    /// Nearest branch around pane g split in direction dir (←/→ is the left-right border, ↑/↓ the top-bottom border)
    pub fn ancestor_with_dir(&self, g: usize, want: SplitDir) -> Option<Path> {
        fn walk(p: &Pane, g: usize, want: SplitDir, path: &mut Path, best: &mut Option<Path>) -> bool {
            match p {
                Pane::Leaf(x) => *x == g,
                Pane::Split { dir, a, b, .. } => {
                    for (side, c) in [(false, a), (true, b)] {
                        path.push(side);
                        let found = walk(c, g, want, path, best);
                        path.pop();
                        if found {
                            if *dir == want && best.is_none() {
                                *best = Some(path.clone());
                            }
                            return true;
                        }
                    }
                    false
                }
            }
        }
        let mut best = None;
        walk(self, g, want, &mut vec![], &mut best);
        best
    }

    /// Branch ratio (None if path isn't a branch)
    pub fn pct_mut(&mut self, path: &[bool]) -> Option<&mut u16> {
        match (self, path.split_first()) {
            (Pane::Split { pct, .. }, None) => Some(pct),
            (Pane::Split { a, b, .. }, Some((side, rest))) => if *side { b } else { a }.pct_mut(rest),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_up_to_four_and_collapse() {
        let mut p = Pane::Leaf(0);
        assert_eq!(p.split(0, SplitDir::Right), Some(1));
        assert_eq!(p.split(0, SplitDir::Down), Some(2));
        assert_eq!(p.split(1, SplitDir::Down), Some(3));
        // 2x2: top-left · bottom-left · top-right · bottom-right
        assert_eq!(p.leaves(), [0, 2, 1, 3]);
        assert_eq!(p.split(3, SplitDir::Right), None);
        // 3 is bottom of the right column: the left-right border is the root, the top-bottom border is the right column
        assert_eq!(p.ancestor_with_dir(3, SplitDir::Right), Some(vec![]));
        assert_eq!(p.ancestor_with_dir(3, SplitDir::Down), Some(vec![true]));
        *p.pct_mut(&[true]).unwrap() = 30;
        p.remove(2);
        assert_eq!(p.leaves(), [0, 1, 3]);
        p.remove(1);
        p.remove(3);
        assert_eq!(p, Pane::Leaf(0));
        // Removed ids are reused
        assert_eq!(p.split(0, SplitDir::Down), Some(1));
    }
}
