/**
 * SOURCE OF TRUTH KEYWORDS: tokens test, dark theme parity, theme mapping test, THEME_SCALE parity, undefined token reference
 * WHAT:  Guards the token system: the two dark blocks in tokens.css are identical, every dark and reduced token
 *        overrides a light one, every `var(--…)` in globals.css and every token reference in src/ TS/TSX exists
 *        in tokens.css, and cn's THEME_SCALE lists exactly the names globals.css maps into Tailwind.
 * WHY:   CSS fails silently: a typo in a token name or a dark value edited in one block but not the other renders
 *        wrong with no error anywhere. These checks turn each of those into a failing test.
 * WHERE: Runs in the `web` Vitest project; reads the stylesheets from disk and the sources as raw text through Vite.
 */
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { THEME_SCALE } from "@/lib/cn";

// Read from disk: Vitest does not process CSS, so a `?raw` CSS import would come back empty.
const tokensCss = readFileSync(join(import.meta.dirname, "tokens.css"), "utf8");
const globalsCss = readFileSync(join(import.meta.dirname, "globals.css"), "utf8");

const SOURCES = import.meta.glob<string>(["/src/**/*.{ts,tsx}", "!/src/bindings.ts", "!/src/**/*.test.{ts,tsx}"], {
  query: "?raw",
  import: "default",
  eager: true,
});

/** Custom properties that components set themselves or that libraries provide. */
const LOCAL_PREFIXES = ["--radix-", "--tw-", "--echo-"];

function block(css: string, opener: string): string {
  const start = css.indexOf(opener);
  if (start < 0) {
    throw new Error(`tokens.css has no block ${opener}`);
  }
  let depth = 0;
  for (let index = css.indexOf("{", start); index < css.length; index += 1) {
    if (css[index] === "{") depth += 1;
    if (css[index] === "}") depth -= 1;
    if (depth === 0) return css.slice(css.indexOf("{", start) + 1, index);
  }
  throw new Error(`unterminated block ${opener}`);
}

function declarations(body: string): Map<string, string> {
  const withoutComments = body.replace(/\/\*[\s\S]*?\*\//g, "");
  return new Map(
    [...withoutComments.matchAll(/(--[\w-]+|color-scheme)\s*:\s*([^;]+);/g)].map((match) => [
      match[1] ?? "",
      (match[2] ?? "").trim(),
    ]),
  );
}

const light = declarations(block(tokensCss, ":root {"));
const dark = declarations(block(tokensCss, ':root[data-theme="dark"] {'));
const systemDark = declarations(block(block(tokensCss, "@media (prefers-color-scheme: dark) {"), "{"));
const reduced = declarations(block(tokensCss, ':root[data-transparency="reduced"] {'));
const theme = declarations(block(globalsCss, "@theme inline reference {"));

function references(text: string): string[] {
  return [...text.matchAll(/(?:var\(|\()(--[a-z][\w-]*)/g)].map((match) => match[1] ?? "");
}

describe("tokens.css", () => {
  it("keeps the explicit and the follow-Windows dark blocks identical", () => {
    expect(systemDark).toEqual(dark);
  });

  it("only overrides tokens the light theme defines", () => {
    for (const name of [...dark.keys(), ...reduced.keys()]) {
      expect(light.has(name), name).toBe(true);
    }
  });

  it("switches every glass tint to its solid under reduced transparency", () => {
    expect(reduced.get("--glass-tint")).toBe("var(--glass-tint-solid)");
    expect(reduced.get("--glass-tint-strong")).toBe("var(--glass-tint-solid)");
  });
});

describe("token references", () => {
  it("globals.css references only tokens that exist", () => {
    for (const name of references(globalsCss)) {
      expect(light.has(name), `globals.css uses undefined ${name}`).toBe(true);
    }
  });

  it("components reference only tokens that exist", () => {
    expect(Object.keys(SOURCES).length).toBeGreaterThan(10);
    for (const [file, source] of Object.entries(SOURCES)) {
      for (const name of references(source)) {
        if (LOCAL_PREFIXES.some((prefix) => name.startsWith(prefix))) continue;
        expect(light.has(name), `${file} uses undefined ${name}`).toBe(true);
      }
    }
  });
});

describe("THEME_SCALE", () => {
  it("lists exactly the names globals.css maps into each Tailwind namespace", () => {
    const mapped = [...theme.keys()];
    for (const [namespace, names] of Object.entries(THEME_SCALE)) {
      const prefix = `--${namespace}-`;
      const expected = mapped
        .filter((name) => name.startsWith(prefix) && !name.slice(prefix.length).includes("--"))
        .map((name) => name.slice(prefix.length));
      expect([...names].sort(), namespace).toEqual(expected.sort());
    }
  });
});
