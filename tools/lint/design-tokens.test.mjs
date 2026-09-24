/**
 * SOURCE OF TRUTH KEYWORDS: design token lint test, no-raw-design-values test, no-raw-css-values test, RuleTester, findRawValues test
 * WHAT:  Tests for tools/lint/design-tokens.mjs: the shared matcher, the TS/TSX rule through ESLint's RuleTester
 *        (class strings, style objects, plain strings) and the CSS rule with Tailwind v4 syntax.
 * WHY:   The rule guards docs/04 as the only source of visual values; a regex regression would either let raw
 *        values back in silently or fail the gate on valid token classes.
 * WHERE: Runs in the `tools` Vitest project (node environment).
 */
import css from "@eslint/css";
import { RuleTester } from "eslint";
import { tailwind4 } from "tailwind-csstree";
import tseslint from "typescript-eslint";
import { describe, expect, it } from "vitest";
import echo, { findRawValues, SPACING_SCALE, splitVariants } from "./design-tokens.mjs";

RuleTester.describe = describe;
RuleTester.it = it;
RuleTester.itOnly = it.only;

const tsx = new RuleTester({
  languageOptions: {
    parser: tseslint.parser,
    parserOptions: { ecmaFeatures: { jsx: true } },
  },
});

const cssTester = new RuleTester({
  plugins: { css },
  language: "css/css",
  languageOptions: { customSyntax: tailwind4 },
});

/** @param {string} text @param {"class" | "css" | "text"} context */
const ids = (text, context) => findRawValues(text, context).map((finding) => finding.messageId);

describe("findRawValues", () => {
  it("reads the spacing scale from tokens.css", () => {
    expect([...SPACING_SCALE]).toEqual(["0", "1", "2", "3", "4", "5", "6", "8", "10", "12"]);
  });

  it("splits variants without breaking arbitrary values", () => {
    expect(splitVariants("hover:data-[state=open]:bg-fill")).toEqual({
      variants: "hover:data-[state=open]:",
      utility: "bg-fill",
    });
    expect(splitVariants("[:root[data-transparency=reduced]_&]:backdrop-filter-none").utility).toBe(
      "backdrop-filter-none",
    );
  });

  it("accepts token classes, var references, fractions and calc arithmetic", () => {
    const valid = [
      "p-4 gap-3 size-10 -mx-1 top-1/2 -translate-x-1/2 w-1/3 translate-x-0",
      "bg-(--glass-tint) border-(length:--border-hairline) duration-(--duration-fast) z-(--z-modal)",
      "rounded-card text-callout shadow-e1 backdrop-blur-md font-medium ease-standard animate-fade-in",
      "px-[calc((var(--switch-height)-var(--switch-thumb))/2)] [&_svg:not([class*=size-])]:size-icon-sm",
      "shadow-[inset_0_var(--border-hairline)_0_var(--glass-highlight),var(--shadow-e1)] w-full h-full",
    ];
    for (const text of valid) {
      expect(ids(text, "class"), text).toEqual([]);
    }
  });

  it("flags raw colours, black and white, dark variants, arbitrary units and off-scale values", () => {
    expect(ids("bg-[#ff0000]", "class")).toEqual(["hex"]);
    expect(ids("text-white", "class")).toEqual(["blackWhite"]);
    expect(ids("bg-black/50", "class")).toEqual(["blackWhite"]);
    expect(ids("dark:bg-fill", "class")).toEqual(["darkVariant"]);
    expect(ids("w-[28px]", "class")).toEqual(["arbitrary"]);
    expect(ids("opacity-[0.5]", "class")).toEqual(["arbitrary"]);
    expect(ids("duration-[200ms]", "class")).toEqual(["arbitrary"]);
    expect(ids("p-7", "class")).toEqual(["rawScale"]);
    expect(ids("duration-200 z-50 opacity-40", "class")).toEqual(["rawScale", "rawScale", "rawScale"]);
    expect(ids("rounded-lg text-sm shadow-md rounded-full", "class")).toEqual([
      "rawScale",
      "rawScale",
      "rawScale",
      "rawScale",
    ]);
  });

  it("checks CSS values for units, named colours and easing curves", () => {
    expect(ids("var(--space-4) calc(var(--duration-slow) * 4) 250% 0", "css")).toEqual([]);
    expect(ids("12px solid rgba(0, 0, 0, 0.1)", "css")).toEqual(["colorFunction", "arbitrary"]);
    expect(ids("white", "css")).toEqual(["blackWhite"]);
    expect(ids("cubic-bezier(0, 0, 1, 1)", "css")).toEqual(["arbitrary"]);
  });

  it("only checks colours in plain text, so copy is free to say white or 12px", () => {
    expect(ids("Black and white 12px", "text")).toEqual([]);
    expect(ids("tint #1d1d1f", "text")).toEqual(["hex"]);
    expect(ids("#root and &#123;", "text")).toEqual([]);
  });
});

tsx.run("no-raw-design-values", echo.rules["no-raw-design-values"], {
  valid: [
    { code: 'const a = <div className="p-4 text-fg bg-fill" />;' },
    { code: 'const a = cn("rounded-card", flag && "shadow-e1");' },
    { code: 'const a = cva("h-control", { variants: { size: { sm: "h-hit px-3" } } });' },
    { code: 'const a = <div style={{ "--echo-progress": fraction, opacity: 0 }} />;' },
    { code: 'const copy = "Press Ctrl+Alt+Space, then speak for 12s";' },
    { code: 'const sel = document.querySelector("#root");' },
  ],
  invalid: [
    {
      code: 'const a = <div className="text-white" />;',
      errors: [{ messageId: "blackWhite" }],
    },
    {
      code: 'const a = cn("w-[28px]", "dark:bg-fill");',
      errors: [{ messageId: "arbitrary" }, { messageId: "darkVariant" }],
    },
    {
      code: "const a = cva(`p-7`);",
      errors: [{ messageId: "rawScale" }],
    },
    {
      code: 'const a = <div style={{ width: 28, color: "#fff", margin: "4px" }} />;',
      errors: [{ messageId: "rawNumber" }, { messageId: "hex" }, { messageId: "arbitrary" }],
    },
    {
      code: 'const accent = "#007AFF";',
      errors: [{ messageId: "hex" }],
    },
  ],
});

cssTester.run("no-raw-css-values", echo.rules["no-raw-css-values"], {
  valid: [
    {
      code: '@import "tailwindcss" source("../");\n@theme inline reference {\n  --*: initial;\n  --color-bg: var(--color-bg);\n}',
    },
    { code: "html { font-size: var(--text-body); outline: var(--focus-ring-width) solid var(--color-accent); }" },
    { code: "@keyframes a { from { translate: -100% 0; opacity: 0; } }" },
  ],
  invalid: [
    { code: "a { color: #fff; }", errors: [{ messageId: "hex" }] },
    { code: "a { padding: 12px; }", errors: [{ messageId: "arbitrary" }] },
    { code: "a { transition: color 200ms; }", errors: [{ messageId: "arbitrary" }] },
    { code: "a { background: rgb(0 0 0); }", errors: [{ messageId: "colorFunction" }] },
    { code: "a { color: black; }", errors: [{ messageId: "blackWhite" }] },
  ],
});
