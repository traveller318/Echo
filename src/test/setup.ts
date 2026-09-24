/**
 * SOURCE OF TRUTH KEYWORDS: vitest setup, test setup, jest-dom matchers, testing library cleanup, jsdom
 * WHAT:  Per-file setup for the `web` Vitest project: DOM matchers and DOM cleanup after each test.
 * WHY:   Vitest globals are off (explicit imports only), so Testing Library cannot register its own
 *        auto-cleanup; it is wired here once instead of in every test file.
 * WHERE: Listed in vite.config.ts `test.projects[web].setupFiles`.
 */
import "@testing-library/jest-dom/vitest";
import { cleanup } from "@testing-library/react";
import { afterEach } from "vitest";

afterEach(() => {
  cleanup();
});
