#!/usr/bin/env bash
# pnpm wasm: build crates/shared-wasm for the GUI into src/wasm/pkg (gitignored).
# Needs the wasm32-unknown-unknown Rust target and wasm-bindgen-cli at the exact version of the wasm-bindgen crate.
# wasm-opt (binaryen) shrinks the module further when it is installed (CI and release builds install it); without it the build still works.
set -euo pipefail
cd "$(dirname "$0")/.."

# CI restored or built src/wasm/pkg for these sources already (.github/actions/wasm): nothing to do, and no toolchain needed
if [ -n "${QURRY_WASM_PREBUILT:-}" ] && [ -f src/wasm/pkg/qurry_shared_wasm_bg.wasm ]; then exit 0; fi

want=$(sed -n 's/^wasm-bindgen = "=\(.*\)"/\1/p' crates/shared-wasm/Cargo.toml)

if command -v rustup >/dev/null && ! rustup target list --installed | grep -qx wasm32-unknown-unknown; then
  echo "pnpm wasm: the Rust target wasm32-unknown-unknown is missing. Install it with:" >&2
  echo "  rustup target add wasm32-unknown-unknown" >&2
  exit 1
fi
have=$(wasm-bindgen --version 2>/dev/null | cut -d' ' -f2 || true)
if [ "$have" != "$want" ]; then
  echo "pnpm wasm: wasm-bindgen-cli $want is required (found: ${have:-none}). Install it with:" >&2
  echo "  cargo install wasm-bindgen-cli --version $want --locked" >&2
  exit 1
fi

cargo build --locked --target wasm32-unknown-unknown -p qurry-shared-wasm --profile wasm
# Generate into a temp dir and copy only files whose bytes changed: rewriting identical files would make a running
# Vite / Storybook dev server reload the page whenever pnpm test or pnpm build starts in another terminal
out=$(mktemp -d)
trap 'rm -rf "$out"' EXIT
wasm-bindgen --target web --out-dir "$out" "${CARGO_TARGET_DIR:-target}/wasm32-unknown-unknown/wasm/qurry_shared_wasm.wasm"
if command -v wasm-opt >/dev/null; then
  # Same features rustc enables by default for wasm32-unknown-unknown (wasm-opt rejects the module otherwise)
  wasm-opt -Oz --enable-bulk-memory --enable-nontrapping-float-to-int --enable-reference-types --enable-multivalue -o "$out/qurry_shared_wasm_bg.wasm" "$out/qurry_shared_wasm_bg.wasm"
fi
mkdir -p src/wasm/pkg
for f in "$out"/*; do
  cmp -s "$f" "src/wasm/pkg/${f##*/}" || cp "$f" src/wasm/pkg/
done
