#!/usr/bin/env bash
# Release: set <version> in the Qurry crate manifests and sync only the workspace versions in Cargo.lock
# (external deps stay as locked, so the --locked builds after it still pass).
set -euo pipefail
cd "$(dirname "$0")/.."

v=${1:?usage: scripts/set-version.sh <version>}

shopt -s nullglob
drivers=(drivers/*/Cargo.toml)
if [ ${#drivers[@]} -eq 0 ]; then
  echo "set-version: no drivers/*/Cargo.toml found" >&2
  exit 1
fi

for f in crates/core/Cargo.toml crates/tui/Cargo.toml crates/mcp/Cargo.toml crates/driver/Cargo.toml src-tauri/Cargo.toml "${drivers[@]}"; do
  if [ ! -f "$f" ]; then
    echo "set-version: $f is missing" >&2
    exit 1
  fi
  V="$v" perl -0pi -e 's/^version = "[^"]*"/version = "$ENV{V}"/m' "$f"
  if ! grep -qx "version = \"$v\"" "$f"; then
    echo "set-version: no version line set in $f" >&2
    exit 1
  fi
done

cargo update --workspace
