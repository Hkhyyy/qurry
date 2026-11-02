# Changelog

What changed in each Qurry version, for people who use it. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow [Semantic Versioning](https://semver.org/). Betas (`vX.Y.Z-beta.N`) are listed under the stable version they lead to. Versions up to 0.2.x are listed only on [GitHub Releases](https://github.com/Hkhyyy/qurry/releases).

## [Unreleased]

### Added

- Stable and beta update channels. Stable (the default) gets the releases made from a version tag; beta also gets the pre-release built from every merge to `main`. Pick one in GUI **Settings › Appearance › Updates** or TUI **⌃k › Update Channel…**; the GUI, the TUI and `qurry-tui --update` share it. See [Updates](docs/updates.md).
- This changelog.

### Changed

- **Releases**: a merge to `main` now publishes a beta pre-release (`vX.Y.Z-beta.N`). Stable releases come only from a version tag on `main`, and only they update Homebrew.
- **macOS 13 Ventura or later** is needed for the GUI. On macOS 12 or older keep Qurry 0.2.x; the TUI and the MCP server still run there ([what to do if an update already went through](docs/updates.md#macos-12-and-older)).
- The GUI and the TUI share one Rust core for statement splitting, the Production guard, completion, grid copy formats, charts and the keymap. The GUI runs it as WebAssembly, so its Content Security Policy allows `wasm-unsafe-eval`.
- The GUI Production confirmation decides like the TUI: a keyword inside a string, a quoted name or a MySQL `#` comment no longer asks. Settings under which a statement could write, side-effect functions (`nextval`, `setval`, `dblink_exec`, `lo_*`) and `SELECT … INTO` now ask.
- GUI console: while the WebAssembly module loads, `⌘↩` shows a loading bar and runs once it arrives. Without the module, a notice says once when a console opens that autocomplete is off.
- Grid copy writes PostgreSQL strings with a backslash as `E'…'`, and MySQL value hints double backslashes.
- Errors for operations a driver plugin doesn't support read "<op> isn't available for the <driver> driver".
- Contributors need the `wasm32-unknown-unknown` target and `wasm-bindgen-cli` 0.2.129 ([Development](docs/development.md)).

### Fixed

- PostgreSQL nested comments and MySQL `#` comments split the way the server reads them, so a commented-out statement never runs.
- Console completion: quoted names with spaces before the dot, `from sales.` with a table named `sales`, Korean identifiers, and long scripts stay fast.
- Korean names rank like their ASCII twins in the TUI palette and MCP `search_objects`; Redis key folders stop at 64 levels.
- Database Admin reports on managed services, MariaDB and large servers (#64); a failed refresh keeps the last result and says how old it is.
- MySQL backup and restore: the client that matches the server's kind and version, `--set-gtid-purged=OFF`, optional removal of `DEFINER=` clauses, and shell commands off during a restore.
- Production log: a change is logged before it runs, and undo SQL is kept only when every changed row was saved.
