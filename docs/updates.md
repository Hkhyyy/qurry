# Updates

English | [한국어](ko/updates.md)

How Qurry finds and installs new versions, and the stable and beta channels.

## Update channels

| Channel | What you get |
|---|---|
| **Stable** (default) | Releases made from a version tag (`vX.Y.Z`). The maintainers tag one when a set of changes is ready. |
| **Beta** | Also the pre-release built from every merge to `main` (`vX.Y.Z-beta.N`, up to a few a day). Beta offers whichever is newer, so once `X.Y.Z` is released you get that too. |

Pick the channel in the GUI under **Settings › Updates › Channel**, or in the TUI with **⌃k › Update Channel…**. The GUI and the TUI share the setting (`settings.json` in the [config folder](getting-started.md#where-qurry-keeps-your-settings)), and so does `qurry-tui --update`.

Betas are built from every merge to `main`, stable releases from version tags; how a maintainer makes and signs them: [Releases and signing](release-signing.md).

Going back from beta to stable doesn't install an older version: you keep the beta you have until a stable release is newer than it. To get off a beta right away, install the latest stable release from [Releases](https://github.com/Hkhyyy/qurry/releases) by hand.

## Checking and installing

Both apps check GitHub for a newer release of their channel when they start and every hour while they run (one request to the GitHub API) and tell you when there is one. You can also check right away.

- **GUI** — a `⬆ vX.Y.Z available` button appears in the status bar. It lists what changed in each version since yours. **Update & Restart** downloads the new version, checks its signature, replaces the app and opens it again with your tabs. The release page is the manual fallback. Check now from **Qurry › Check for Updates…** or **Settings › Updates**, where you can also turn the automatic check off. In-app updates start from 0.1.9 (Apple Silicon), and from the first Intel / Linux release.
- **TUI** — the status line shows `⬆ vX.Y.Z available`. Run **⌃k › Update Qurry & Restart** to download the release, verify it against the signed `SHA256SUMS`, replace `qurry-tui` and `qurry-mcp` in place and restart with your tabs. **⌃k › Check for Updates** checks now and lists what changed in each version since yours. From a shell, `qurry-tui --update` does the same. A TUI built from source (`target/release`) is not checked or replaced; update it with `git pull` and `cargo build`. A Homebrew install is updated with `brew upgrade qurry`; Homebrew only gets stable releases.
- Turn the check off with `qurry-tui --no-update-check` or `QURRY_NO_UPDATE_CHECK=1`.

Apps from before 0.3.0 don't know the channels and only see stable releases. They look for one among the 30 newest releases, so the release workflow keeps only the 20 newest betas; the pages of older betas are deleted. Driver plugins download from the release of the app's own version, so a beta whose page was deleted can no longer install drivers: it says so, and updating Qurry fixes it.

## macOS 12 and older

The GUI needs macOS 13 Ventura or later from 0.3.0 on (it runs WebAssembly, which needs the Safari 16 engine). On macOS 12 or older:

- Keep Qurry 0.2.x, the last version that runs there. Don't press **Update & Restart** in it: it would install a version that doesn't open.
- If you already did and macOS says "You can't use this version of the application Qurry with this version of macOS", download the newest `0.2.x` DMG from [Releases](https://github.com/Hkhyyy/qurry/releases) and drag `Qurry.app` into Applications again. Your data sources, passwords and settings are kept. Or update macOS (System Settings › General › Software Update) and keep the new version.
- The TUI and the MCP server still run there and keep updating.

---

[← User guide](README.md)
