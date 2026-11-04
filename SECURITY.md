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

An installed version trusts only the public key it was built with, so changing the key needs a bridge release.

### Rotating the key

1. Make a new key pair: `pnpm tauri signer generate -w <file>`, with a password. Back it up offline.
2. In a pull request, put the new public key in both places above. Re-sign the test signature in `sums_must_be_signed_with_the_update_key` (a signature of `hello\n`) with the new key (`pnpm tauri signer sign -f <file> <hello file>`, then use the `.sig` contents) so the test passes.
3. Merge it and release it as a stable version while the secrets still hold the **old** key. This bridge release is signed with the old key, so every installed version accepts it, and it trusts the new key from then on.
4. Give users time to update to the bridge release. Then replace both secrets with the new key. Every release after that (beta and stable) is signed with the new key.

Versions older than the bridge release can't verify releases signed with the new key, so their in-app update and `qurry-tui --update` fail at the signature check. Users still on them reinstall as described below.

### If the private key is lost

Installed versions can't accept anything signed with another key, so there is no in-app path to a new key. Make a new key pair, put it in both secrets and both public key places (as in steps 1 and 2 above), and release. Users of every earlier version must reinstall once from the [Releases](https://github.com/Hkhyyy/qurry/releases) page (GUI: the DMG, AppImage or deb; TUI and MCP: `brew upgrade qurry` or the `qurry-cli_…` archive); their data sources, passwords and settings are kept. After that, updates work as before.

Tell them in the release notes of that release and of the stable releases after it, at the top of `.github/release-notes.md`: say that in-app updates from older versions fail with a signature error because the signing key changed, that this is expected and not an attack, and link the download page. Users on an old version only see this if they open the release page, so also pin an issue or discussion with the same text.

### If the private key leaks

A leaked key lets whoever holds it sign updates that every installed version accepts, if they can also get them served from this repository's releases.

1. Contain: check the repository and the maintainer's GitHub account for releases, tags, workflow changes, secrets and access tokens you didn't make. Delete those releases and revoke those tokens.
2. Rotate right away, as in "Rotating the key", without waiting between steps 3 and 4: the bridge release is still signed with the leaked key (the only key installed versions accept), and the secrets are replaced with the new key as soon as it is published. Then destroy every copy of the leaked key.
3. Tell users: publish a [security advisory](https://github.com/Hkhyyy/qurry/security/advisories) that says which releases may be affected, asks everyone to update to the bridge release now (or reinstall from the Releases page), and explains how to check a download against its build provenance attestation (`gh attestation verify <file> --repo Hkhyyy/qurry`). Put the same notice at the top of the release notes.

Versions older than the bridge release keep trusting the leaked key until they update, so the advisory should say this plainly.
