import { defineConfig, devices } from "@playwright/test";

// GUI end-to-end tests against the mock backend (src/lib/mock.ts) — no Tauri, no database.
// Not 1420: that's the dev server `pnpm tauri dev` may already be running. E2E_PORT picks another free port (vite has strictPort).
const port = Number(process.env.E2E_PORT ?? 1425);
const viewport = { width: 1400, height: 900 };

export default defineConfig({
  testDir: "e2e",
  forbidOnly: !!process.env.CI,
  // One worker by default (laptops run other things); E2E_WORKERS or --workers for more (CI sets 2)
  workers: Number(process.env.E2E_WORKERS ?? 1),
  retries: 0,
  reporter: process.env.CI ? "github" : "list",
  use: {
    baseURL: `http://localhost:${port}/?mock`,
    trace: "retain-on-failure",
  },
  projects: [
    // Grid copy reads the clipboard back; WebKit only knows clipboard-read (writing needs no permission there)
    { name: "chromium", use: { ...devices["Desktop Chrome"], viewport, permissions: ["clipboard-read", "clipboard-write"] } },
    { name: "webkit", use: { ...devices["Desktop Safari"], viewport, permissions: ["clipboard-read"] } },
  ],
  webServer: {
    command: `pnpm dev --port ${port}`,
    url: `http://localhost:${port}/`,
    reuseExistingServer: false,
    timeout: 60_000,
  },
});
