# Performance budget scripts

One command each; each prints a compact table. Results are not kept in the repo. Run from the repository root after
`pnpm install` (the Playwright browsers too: `npx playwright install chromium webkit`). The Cargo steps honor `CARGO_TARGET_DIR`.

| What | Command | Takes |
| --- | --- | --- |
| Shipped sizes: shared WASM (raw / gzip / brotli, against the 1 MB / 350 KB budget) and the JS / CSS chunks | `scripts/perf/size.sh` | ~1 min |
| GUI in Chromium and WebKit: cold load, WASM init, completion cost per keystroke (2k-table schema), ⌃Space latency, React commits | `node scripts/perf/web.mjs [--engines chromium,webkit] [--runs 10] [--json]` | ~1 min |
| TUI: cold start to the first frame, frame time and key dispatch on a 10k × 30 grid and a 100 KB console | `scripts/perf/tui.sh [runs]` | ~3 min (release build) |
| Shared WASM under the production CSP, and the app without it (pass / fail) | `node scripts/perf/wasm-smoke.mjs [chromium\|webkit] <preview url>` | seconds |

## What each one measures

- `size.sh` builds the way CI and release ship (`pnpm build` with `wasm-opt -Oz` from binaryen 132.0.0 on `PATH`), then
  `size.mjs` reads `dist/assets`. KB = 1024 bytes; gzip level 9, brotli quality 11.
- `web.mjs` needs `dist/` (run `size.sh` first) and starts its own `vite preview` and `vite` dev servers on free ports, one
  browser at a time.
  - Production build: the `?mock` backend is dev only, so a stub backend answers every command like an empty install. A fresh
    browser context per run (empty HTTP cache); the first two runs are warm-up. "App shell rendered" is the first animation
    frame with `.app` in the DOM; "WASM ready" is the end of the `qurry:shared-wasm-init` measure (`src/lib/shared.ts`).
  - Completion: the dev `?mock` app with 2,000 tables × 20 columns as completion metadata. "suggest/keystroke" replays the
    provider's synchronous steps (`src/lib/completion.ts`: `getValue`, `hintContext`, `suggest`, mapping to Monaco items)
    through the app's own `shared()` module and a Monaco model, at every caret while typing. Keep it in step with the provider.
    WebKit's clock has 1 ms steps, so each keystroke runs over several indexes (each fed the same keystrokes) and is divided.
    "suggest calls while typing" wraps `CompletionIndex.suggest` while really typing in a console (clock resolution applies).
  - "⌃Space → first row" includes Monaco's own widget delay (~100 ms), not only the provider.
- `tui.sh` builds `qurry-tui` in release, runs `tui_start.py` (a pty, 160 × 40, empty scratch `--config-dir`, answering the
  terminal queries crossterm sends; the frame is complete when the status line's last words arrive), then the ignored tests in
  `crates/tui/src/tests/perf.rs`. A frame there is `ui::draw` plus ratatui's diff into a `TestBackend`, without terminal writes.
