/**
 * SOURCE OF TRUTH KEYWORDS: applyDesignTokens, test design tokens, jsdom tokens, tokens.css in tests
 * WHAT:  `applyDesignTokens()` sets every light-theme token of tokens.css as an inline custom property on <html>,
 *        and `clearDesignTokens()` removes them again.
 * WHY:   Vitest does not process CSS, so components that read tokens at runtime (motion durations, pill widths through
 *        styles/motion.ts) would fail loudly in jsdom; reading the real tokens.css keeps tests on the shipped values
 *        instead of copies.
 * WHERE: Component tests that render token-reading components (src/pill/Pill.test.tsx).
 */
import { readFileSync } from "node:fs";
import { join } from "node:path";

const ROOT_BLOCK = /:root\s*\{([\s\S]*?)\n\}/;
const DECLARATION = /(--[\w-]+)\s*:\s*([^;]+);/g;

export function applyDesignTokens(root: HTMLElement = document.documentElement): void {
  const css = readFileSync(join(import.meta.dirname, "..", "styles", "tokens.css"), "utf8");
  const body = ROOT_BLOCK.exec(css)?.[1] ?? "";
  for (const match of body.replace(/\/\*[\s\S]*?\*\//g, "").matchAll(DECLARATION)) {
    const [, name, value] = match;
    if (name !== undefined && value !== undefined) {
      root.style.setProperty(name, value.trim());
    }
  }
}

export function clearDesignTokens(root: HTMLElement = document.documentElement): void {
  root.removeAttribute("style");
}
