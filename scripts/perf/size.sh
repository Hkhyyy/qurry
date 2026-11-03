#!/usr/bin/env bash
# Production build as CI and release ship it (wasm-opt -Oz from binaryen 132.0.0, the version the workflows install), then the sizes table.
# Run:  scripts/perf/size.sh      (honors CARGO_TARGET_DIR; pnpm 10.25.0 through npx)
set -euo pipefail
cd "$(dirname "$0")/../.."
# binaryen's wasm-opt on PATH for scripts/wasm.sh, which pnpm build runs as prebuild
bin=$(npx -y -p binaryen@132.0.0 -c 'dirname "$(command -v wasm-opt)"')
PATH="$bin:$PATH" npx -y pnpm@10.25.0 build >/dev/null
node scripts/perf/size.mjs
