# Security policy

## Supported versions

Only the latest stable release (the one marked **Latest** on the [Releases](https://github.com/Hkhyyy/qurry/releases) page) gets security fixes. Fixes are not backported to older versions; the GUI and `qurry-tui --update` update to the latest release. `qurry-mcp` comes with `qurry-tui`, not with the GUI: update it with `brew upgrade qurry` or `qurry-tui --update` and restart the AI client that runs it.

## Reporting a vulnerability

Please don't open a public issue for a vulnerability. Report it privately through GitHub: [Security › Report a vulnerability](https://github.com/Hkhyyy/qurry/security/advisories/new).

If that form is not available, open an issue that only asks for a private contact, without any details, and the maintainer will reach out.

Include what you can:

- the Qurry version and app (GUI, `qurry-tui` or `qurry-mcp`) and your OS
- the database kind and version, if it matters
- steps to reproduce, and what an attacker gains

Qurry has one maintainer, so expect an answer within a week. Once a fix is released, the advisory is published with credit to you, unless you'd rather not be named.

## Scope

Examples of what counts:

- a way around a read-only data source, the MCP server's read-only checks or its per-data-source MCP setting (see the [MCP threat model](docs/mcp-security.md))
- saved passwords sent to a server they were not saved for, or readable by another local user
- a crafted data source file, query result or driver response that runs code or writes files outside what you asked for
- a way to install an update that was not signed with the release key

What an AI agent does with data it was allowed to read is described in the [MCP threat model](docs/mcp-security.md) under "What Qurry does not protect against"; those cases are not vulnerabilities on their own.

## Release signing key

One key signs everything Qurry installs over the network: the GUI's in-app update bundles (their signatures go into each release's `latest.json`) and each release's `SHA256SUMS` (as `SHA256SUMS.sig`), which `qurry-tui --update` and the driver plugin download check. It is a [Tauri updater](https://v2.tauri.app/plugin/updater/) (minisign) key pair, and Qurry has one maintainer who holds it.

### Where it lives

- The private key and its password are GitHub Actions secrets of this repository, `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. They are never committed. `.github/workflows/release.yml` passes them only to the Sign step of the `publish` job, which runs only for pushes to `main` and version tags on `main`, not to the build jobs or to pull requests.
- Secrets can't be read back from GitHub, so keep an offline backup of the private key file and its password outside GitHub (without it, a lost key can't be recovered; see below).
- The public key is in the repository, in two places that must match (the test `sums_must_be_signed_with_the_update_key` in `crates/tui/src/selfupdate.rs` checks this):
  - `plugins.updater.pubkey` in `src-tauri/tauri.conf.json` (the GUI updater)
  - `UPDATE_PUBKEY` in `crates/core/src/update.rs` (`qurry-tui --update` and driver plugins)

An installed version trusts only the public key it was built with, so changing the key needs a bridge release. Installed versions update in the app only to the release marked **Latest** (or, on the beta channel, the newest beta), so a version reaches the bridge in the app only while the bridge (or a beta built like it) is the newest release it is offered.

### Rotating the key

1. Make a new key pair: `pnpm tauri signer generate -w <file>`, with a password. Back it up offline.
2. In a pull request, put the new public key in both places above. Re-sign the test signature in `sums_must_be_signed_with_the_update_key` (a signature of `hello\n`) with the new key (`pnpm tauri signer sign -f <file> <hello file>`, then use the `.sig` contents) so the test passes. At the top of `.github/release-notes.md`, add `## What's Changed` with the item `* [note] This release can't install driver plugins. Update to the next release to install them; drivers installed before keep working` (the update window shows it, see "If the private key is lost").
3. Merge the pull request and release it as a stable version (tag its merge commit on `main`, `git tag vX.Y.Z <sha> && git push origin vX.Y.Z`) while the secrets still hold the **old** key. This bridge release is signed with the old key, so every installed version accepts it, and it trusts only the new key from then on.
4. Keep the bridge as Latest for two weeks, so installed versions find it in the app. Don't release another stable version in that time, and avoid merging to `main`: every merge publishes a beta built the same way as the bridge (new public key, old signature), with the same driver plugin limitation (below). Two weeks is a trade-off: a longer window lets more people update in the app, a shorter one ends the plugin limitation sooner.
5. Switch to the new key and release the next stable version:
   1. Replace both secrets with the new key. Every release from now on (beta and stable) is signed with it.
   2. In a pull request, replace the bridge item at the top of `.github/release-notes.md` with the key notice from "If the private key is lost" (for the versions older than the bridge, which can't install this release), and merge it. Its beta is the first one signed with the new key.
   3. Tag that merge commit (or any later commit on `main`) with a version higher than the bridge: `git tag vX.Y.Z <sha> && git push origin vX.Y.Z`. Stable releases come only from version tags on `main`, and the workflow refuses a tag lower than the newest stable release. This works even when nothing else changed on `main` since the bridge.

Known limitation of the bridge: it trusts only the new key, but its own `SHA256SUMS.sig` is signed with the old one. A driver plugin is downloaded from the release of the running version and checked against that signature, so on the bridge release (and on any beta built between steps 3 and 5) installing a driver plugin fails with "SHA256SUMS signature check failed". Drivers installed before keep working. There is no workaround on the bridge itself: users who need a new driver update to the release from step 5 (the GUI's in-app update and `qurry-tui --update` accept it), where installing drivers works again.

Versions older than the bridge release can't verify releases signed with the new key. Once the release from step 5 is Latest they are offered only that one, and their in-app update and `qurry-tui --update` fail at the signature check, so there is no in-app path for them any more. Users still on them reinstall as described below; the key notice in the update window tells them so.

### If the private key is lost

Installed versions can't accept anything signed with another key, so there is no in-app path to a new key. Make a new key pair, put it in both secrets and both public key places (as in step 1 and the first part of step 2 above), put the notice below in `.github/release-notes.md`, and release. Users of every earlier version must reinstall once from the [Releases](https://github.com/Hkhyyy/qurry/releases) page (GUI: the DMG, AppImage or deb; TUI and MCP: `brew upgrade qurry` or the `qurry-cli_…` archive); their data sources, passwords and settings are kept. After that, updates work as before.

Tell them in the update window, which shows only the items under `## What's Changed` in the release notes (GUI, TUI and versions before 0.3.0 alike): an old version shows it when it finds the update, before the install fails at the signature check. Put this at the very top of `.github/release-notes.md`:

```markdown
## What's Changed
* [breaking] The update signing key changed. If updating fails with a signature error, that is expected, not an attack: download Qurry again from https://github.com/Hkhyyy/qurry/releases (your data sources, passwords and settings are kept)
```

Text elsewhere in the notes only shows on the release page. Also pin an issue or discussion with the same text.

Keep the notice in `.github/release-notes.md`, so it is on every release, for six months after the first release signed with the new key, then remove it (and unpin the issue) in a pull request. An old version shows the notes of the release it is offered when it checks, which can be any later stable release, so the notice has to stay on all of them for as long as people may still open an old version.

### If the private key leaks

A leaked key lets whoever holds it sign updates that every installed version accepts, if they can also get them served from this repository's releases.

1. Contain: check the repository and the maintainer's GitHub account for releases, tags, workflow changes, secrets and access tokens you didn't make. Delete those releases and revoke those tokens.
2. Rotate right away, as in "Rotating the key", skipping the two weeks in step 4: the bridge release is still signed with the leaked key (the only key installed versions accept), and the secrets are replaced with the new key as soon as it is published. Then destroy every copy of the leaked key. Without the window, almost no one reaches the bridge in the app, so expect every user to reinstall.
3. Tell users: publish a [security advisory](https://github.com/Hkhyyy/qurry/security/advisories) that says which releases may be affected, that every version older than the bridge must be reinstalled from the [Releases](https://github.com/Hkhyyy/qurry/releases) page (the in-app update and `qurry-tui --update` fail there with a signature error), and how to check a download against its build provenance attestation (`gh attestation verify <file> --repo Hkhyyy/qurry`). The key notice from "If the private key is lost" goes into `.github/release-notes.md` in step 5 as usual.

Versions older than the bridge release keep trusting the leaked key until they are reinstalled, so the advisory should say this plainly.
