#!/usr/bin/env bash
# TUI budget in release mode: cold start to the first frame (scripts/perf/tui_start.py, in a pty), then frame time and key dispatch
# on a 10k × 30 grid and a 100 KB console (crates/tui/src/tests/perf.rs, ignored tests driving the test Harness).
# Run:  scripts/perf/tui.sh [runs]      (honors CARGO_TARGET_DIR)
set -euo pipefail
cd "$(dirname "$0")/../.."
runs=${1:-20}
cargo build --release -p qurry-tui --quiet
# A copy: another checkout sharing CARGO_TARGET_DIR may relink target/release/qurry-tui meanwhile
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
cp "${CARGO_TARGET_DIR:-target}/release/qurry-tui" "$tmp/"
python3 scripts/perf/tui_start.py "$tmp/qurry-tui" "$runs"
cargo test --release --quiet -p qurry-tui perf_ -- --ignored --nocapture --test-threads=1 2>/dev/null | grep -o 'PERF.*'
