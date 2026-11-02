# Updates

English | [한국어](ko/updates.md)

How Qurry finds and installs new versions, the stable and beta channels, and how releases are made.

## Update channels

| Channel | What you get |
|---|---|
| **Stable** (default) | Releases made from a version tag (`vX.Y.Z`). The maintainers tag one when a set of changes is ready. |
| **Beta** | Also the pre-release built from every merge to `main` (`vX.Y.Z-beta.N`, up to a few a day). Beta offers whichever is newer, so once `X.Y.Z` is released you get that too. |

Pick the channel in the GUI under **Settings › Appearance › Updates › Update channel**, or in the TUI with **⌃k › Update Channel…**. The GUI and the TUI share the setting (`settings.json` in the [config folder](getting-started.md#where-qurry-keeps-your-settings)), and so does `qurry-tui --update`.

Going back from beta to stable doesn't install an older version: you keep the beta you have until a stable release is newer than it. To get off a beta right away, install the latest stable release from [Releases](https://github.com/Hkhyyy/qurry/releases) by hand.

## Checking and installing

Both apps check GitHub for a newer release of their channel when they start and every hour while they run (one request to the GitHub API) and tell you when there is one. You can also check right away.

- **GUI** — a `⬆ vX.Y.Z available` button appears in the status bar. It lists what changed in each version since yours. **Update & Restart** downloads the new version, checks its signature, replaces the app and opens it again with your tabs. The release page is the manual fallback. Check now from **Qurry › Check for Updates…** or **Settings › Appearance › Updates**, where you can also turn the automatic check off. In-app updates start from 0.1.9 (Apple Silicon), and from the first Intel / Linux release.
- **TUI** — the status line shows `⬆ vX.Y.Z available`. Run **⌃k › Update Qurry & Restart** to download the release, verify it against the signed `SHA256SUMS`, replace `qurry-tui` and `qurry-mcp` in place and restart with your tabs. **⌃k › Check for Updates** checks now and lists what changed in each version since yours. From a shell, `qurry-tui --update` does the same. A TUI built from source (`target/release`) is not checked or replaced; update it with `git pull` and `cargo build`. A Homebrew install is updated with `brew upgrade qurry`; Homebrew only gets stable releases.
- Turn the check off with `qurry-tui --no-update-check` or `QURRY_NO_UPDATE_CHECK=1`.

Apps from before 0.3.0 don't know the channels and only see stable releases.

## macOS 12 and older

The GUI needs macOS 13 Ventura or later from 0.3.0 on (it runs WebAssembly, which needs the Safari 16 engine). On macOS 12 or older:

- Keep Qurry 0.2.x, the last version that runs there. Don't press **Update & Restart** in it: it would install a version that doesn't open.
- If you already did and macOS says "You can't use this version of the application Qurry with this version of macOS", download the newest `0.2.x` DMG from [Releases](https://github.com/Hkhyyy/qurry/releases) and drag `Qurry.app` into Applications again. Your data sources, passwords and settings are kept. Or update macOS (System Settings › General › Software Update) and keep the new version.
- The TUI and the MCP server still run there and keep updating.

## How releases are made

For maintainers. `.github/workflows/release.yml` builds, signs and publishes:

- **Merge to `main`** → a beta pre-release `vX.Y.Z-beta.N`. `X.Y.Z` is the next stable version: the patch after the newest stable tag, or the version in `crates/core/Cargo.toml` when a PR raised it. `N` counts up for each merge.
- **Version tag on `main`** → the stable release `vX.Y.Z`, marked latest, with notes since the previous stable release, and the Homebrew formula. Tag the commit of the beta you want to ship: `git tag vX.Y.Z <sha> && git push origin vX.Y.Z`. A tag that is not on `main`, or is lower than the newest stable release, is refused and the run says how to fix it (delete the tag with `git push --delete origin vX.Y.Z` and push the right one).
- **Pull request changing the workflow** → builds only, nothing is published.

Both channels get the same files, signatures, `SHA256SUMS` and build provenance attestations ([Safety](safety.md)).

---

[← User guide](README.md)
