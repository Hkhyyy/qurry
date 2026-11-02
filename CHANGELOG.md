# Changelog

What changed in each Qurry version, for people who use it. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow [Semantic Versioning](https://semver.org/). Betas (`vX.Y.Z-beta.N`) are listed under the stable version they lead to. Versions up to 0.2.x are listed only on [GitHub Releases](https://github.com/Hkhyyy/qurry/releases).

## [Unreleased]

### Added

- Stable and beta update channels. Stable (the default) gets the releases made from a version tag; beta also gets the pre-release built from every merge to `main`. Pick one in GUI **Settings › Appearance › Updates** or TUI **⌃k › Update Channel…**; the GUI, the TUI and `qurry-tui --update` share it. See [Updates](docs/updates.md).
- **TUI color modes**: `NO_COLOR` draws without colors (reverse video for selections and badges, bold edited cells, `+` `-` `~` in the row number) and says how to get them back; 256- and 16-color terminals get the nearest palette colors; kitty, Alacritty, WezTerm, foot and Ghostty get true color without `COLORTERM`. A window smaller than 40×10 shows "terminal too small" instead of crashing, and only the quit key acts there. See [TUI in your terminal](docs/tui.md).
- **Allow unencrypted in one click**: when a remote PostgreSQL or MySQL server without TLS refuses the connection, the error in the explorer, console, table tab or Test Connection offers **Allow unencrypted for this data source** (TUI: a confirm popup that only `y` answers). It keeps the saved password, and is never offered for Production data sources ([Data sources](docs/data-sources.md)).
- **AWS SSM tunnel**: connect through Session Manager port forwarding (GUI **SSH/SSL › Tunnel › AWS SSM**, TUI `Tunnel` › `aws ssm`) with your AWS profile, region and instance id. Qurry runs the AWS CLI v2 and its Session Manager plugin and stores no AWS secrets. Update every Qurry app on the computer first: older ones don't know the tunnel ([Data sources](docs/data-sources.md)).
- [SECURITY.md](SECURITY.md) (report a vulnerability privately through GitHub), [CONTRIBUTING.md](CONTRIBUTING.md) (toolchain, tests, commit and language rules, issue and pull request templates) and the [MCP threat model](docs/mcp-security.md): what an AI client reaches for each MCP setting, how read-only is enforced, and what Qurry does not protect against.
- This changelog.

### Changed

- **Releases**: a merge to `main` now publishes a beta pre-release (`vX.Y.Z-beta.N`). Stable releases come only from a version tag on `main`, and only they update Homebrew. Only the 20 newest betas keep their release page: an older beta can't install driver plugins any more and says to update Qurry. A beta `.deb` has the package version `X.Y.Z~beta.N`, so `apt` takes the stable `X.Y.Z` as an upgrade.
- **macOS 13 Ventura or later** is needed for the GUI. On macOS 12 or older keep Qurry 0.2.x; the TUI and the MCP server still run there ([what to do if an update already went through](docs/updates.md#macos-12-and-older)).
- The GUI and the TUI share one Rust core for statement splitting, the Production guard, completion, grid copy formats, charts and the keymap. The GUI runs it as WebAssembly, so its Content Security Policy allows `wasm-unsafe-eval`.
- The GUI Production confirmation decides like the TUI: a keyword inside a string, a quoted name or a MySQL `#` comment no longer asks. Settings under which a statement could write, side-effect functions (`nextval`, `setval`, `dblink_exec`, `lo_*`) and `SELECT … INTO` now ask. A selection run as one statement is a read only if every statement in it is (`select 1; do $$ … delete … $$` asks), and so is SQL Server text that starts another statement without `;`.
- **Explain Analyze** asks first when a rollback can't undo what the statement does (sequence values, `dblink_exec`, `SELECT … INTO OUTFILE`, a `COMMIT` or MySQL DDL after the first statement): Cancel runs nothing, and Output says afterwards what stays. On data sources other than PostgreSQL and MySQL it isn't wrapped in a transaction and asks "it changes data". GUI and TUI decide the same way ([Safety](docs/safety.md)).
- GUI console: while the WebAssembly module loads, `⌘↩` shows a loading bar and runs once it arrives. Without the module, a notice says once when a console opens that autocomplete is off.
- PostgreSQL strings with a backslash are written as `E'…'` in grid copy formats, **Filter by** and the SQL of grid edits (they used to write `'a\b'`), and MySQL value hints double backslashes. ClickHouse names in the grid's SQL double a backslash inside the quotes, as the server reads them.
- Errors for operations a driver plugin doesn't support read "<op> isn't available for the <driver> driver".
- **Explain Analyze** on PostgreSQL and MySQL rolls back every statement, reads included, and asks first before a statement that calls a function Qurry doesn't know (a user-defined one could write or reach another server; Run Anyway goes on, Explain Plan shows the plan without running it) and before an `UPDATE` or `DELETE` without `WHERE` (every row stays locked until the rollback). It asks that one question instead of the "dangerous statement" question as well, in the GUI as in the TUI. On a read-only data source it works inside a transaction you opened.
- Contributors need the `wasm32-unknown-unknown` target and `wasm-bindgen-cli` 0.2.129 ([Development](docs/development.md)).

### Fixed

- PostgreSQL nested comments and MySQL `#` comments split the way the server reads them, so a commented-out statement never runs.
- Full-text search runs each table's `SELECT` in a READ ONLY transaction on PostgreSQL and MySQL, so a view whose function writes (an `INSERT`, `nextval`) is refused instead of committed.
- Console completion: quoted names with spaces before the dot, `from sales.` with a table named `sales`, Korean identifiers, and long scripts stay fast.
- Korean names rank like their ASCII twins in the TUI palette and MCP `search_objects`; Redis key folders stop at 64 levels.
- Database Admin reports on managed services, MariaDB and large servers (#64); a failed refresh keeps the last result and says how old it is.
- MySQL backup and restore: the client that matches the server's kind and version, `--set-gtid-purged=OFF`, optional removal of `DEFINER=` clauses, and shell commands off during a restore.
- Production log: a change is logged before it runs, and undo SQL is kept only when every changed row was saved.
- PostgreSQL full-text search and value hints write a search text or value with a backslash as `E'…'`, so a server with `standard_conforming_strings = off` can't read it as SQL.
- SQL Server: a `SELECT` of columns named `open`, `close` and the like is a read again (no Production confirmation).
- TUI: the Explain Analyze note about what a rollback can't undo is a warning, not an error.
