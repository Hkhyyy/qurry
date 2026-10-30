//! Flow tests that feed keys through the harness: they pin down what the TUI does today across modules (explorer, console, tabs, keymap),
//! so moving code between files can't change behavior unnoticed. Flows already covered next to their code (input.rs: Production confirm when
//! counting fails, Redis key folders, Data Source form saves …) aren't repeated here
mod console;
mod explorer;
mod keymap;
mod tabs;
