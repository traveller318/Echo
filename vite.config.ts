/**
 * SOURCE OF TRUTH KEYWORDS: vite config, window entries, main window, pill window, dev server port, vitest projects, path alias
 * WHAT:  Vite build for the two webview entries (index.html → main window, pill.html → pill window) plus the Vitest setup.
 * WHY:   Each Tauri window loads its own HTML entry so the pill ships a tiny bundle and paints fast (02 §6.2).
 *        Port 1420 is strict because tauri.conf.json `build.devUrl` points at it. `src-tauri/` is not watched:
 *        cargo writes gigabytes into `target/` and every write would wake the watcher.
 *        `@/` resolves from tsconfig `paths`, so the alias is declared once.
 * WHERE: Used by `pnpm dev` / `pnpm build` (called by `tauri dev` / `tauri build`) and by `pnpm test`.
 */
import { fileURLToPath } from "node:url";
import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vitest/config";

const entry = (file: string): string => fileURLToPath(new URL(file, import.meta.url));

export default defineConfig({
  plugins: [react(), tailwindcss()],
  clearScreen: false,
  envPrefix: ["VITE_", "TAURI_ENV_"],
  resolve: {
    tsconfigPaths: true,
  },
  server: {
    port: 1420,
    strictPort: true,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
  build: {
    // WebView2 is evergreen Chromium on every supported Windows build.
    target: "chrome120",
    rolldownOptions: {
      input: {
        main: entry("./index.html"),
        pill: entry("./pill.html"),
      },
    },
  },
  test: {
    // One worker per logical core oversubscribes the CPU during jsdom setup, and interaction-heavy UI tests then
    // hit their 5 s timeout at random (05 W33); half the cores finishes sooner and never times out.
    maxWorkers: "50%",
    projects: [
      {
        extends: true,
        test: {
          name: "web",
          environment: "jsdom",
          include: ["src/**/*.test.{ts,tsx}"],
          setupFiles: ["./src/test/setup.ts"],
        },
      },
      {
        extends: true,
        test: {
          name: "tools",
          environment: "node",
          include: ["tools/**/*.test.mjs"],
        },
      },
    ],
  },
});
