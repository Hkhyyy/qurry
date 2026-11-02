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
