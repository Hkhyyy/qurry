# Workspaces

Keep separate sets of tabs and split panes — one for a production incident, one for a report you are writing — and switch between them without losing anything: consoles keep their session, open transactions and results while you are elsewhere.

- **GUI**: the thin bar at the far left lists the workspaces (`+` makes one; right-click to rename or delete). `⌥1`…`⌥9` switches; right-click a tab › **Move to Workspace**.
- **TUI**: `⌥1`…`⌥9` switches, `⌥n` new, `⌥⇧N` rename (or `⌃k` › Switch / New / Rename / Delete Workspace…, Move Tab to Workspace…). In the explorer, `w` moves the selected data source to another workspace. With more than one workspace a strip at the top shows them (click to switch). The explorer toggle moved from `⌥1` to `⌥0`.
- **Each workspace has its own Database Explorer**: the data sources you put in it, expanded the way you left it. A new workspace starts empty — add data sources with `+` in the explorer (new, or one you already have), or right-click a data source › Add to Workspace / Remove from This Workspace (TUI: the **Workspace** field in the data source form (`e` / `n`) moves it, or `⌃k` › Add Data Source to This Workspace… / Remove Data Source from This Workspace). The data source itself (host, password) is shared; a new one goes into the workspace you are in.
- Workspaces are saved with their tabs and explorer and come back the next time you start.

---

[← User guide](README.md)
