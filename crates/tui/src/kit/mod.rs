//! TUI spec (same names · colors as the GUI's src/ui). Screen code uses this instead of numeric colors · hand-built lines.
//! atoms (colors · text fragments) → molecules (one line: Row · key hints · form field) → organisms (panels · popups · confirm dialogs).
//! Preview: "TUI" in Storybook (redraw with pnpm tui-snapshots)
pub mod atoms;
pub mod molecules;
pub mod organisms;
pub use atoms::*;
pub use molecules::*;
pub use organisms::*;
