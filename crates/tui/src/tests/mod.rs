//! Flow tests that feed keys through the harness: they pin down what the TUI does today across modules (explorer, console, tabs, keymap),
//! so moving code between files can't change behavior unnoticed. Flows already covered next to their code (input/: Production confirm when
//! counting fails, Redis key folders, Data Source form saves …) aren't repeated here
mod actions;
mod columns;
mod confirm;
mod console;
mod explorer;
mod keymap;
mod marks;
mod notices;
mod perf;
mod tabs;
mod temporary;
mod tls_optin;
