/// <reference types="vitest/config" />
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
// @ts-expect-error type error without @types/node package
import process from "node:process";
import tauriConf from "./src-tauri/tauri.conf.json" with { type: "json" };
const host = process.env.TAURI_DEV_HOST;

// https://vite.dev/config/
export default defineConfig(() => ({
  plugins: [react()],
  // e2e/ holds Playwright specs (npx playwright test), not vitest ones
  test: { exclude: ["**/node_modules/**", "e2e/**"] },

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },
  // `vite preview` serves the build under the app's production CSP, to check it the way the app loads it
  preview: { headers: { "Content-Security-Policy": tauriConf.app.security.csp } },
  test: { setupFiles: ["vitest.setup.ts"] },
}));
