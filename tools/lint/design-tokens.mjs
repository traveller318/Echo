/**
 * SOURCE OF TRUTH KEYWORDS: design token lint, eslint-plugin-echo, no-raw-design-values, no-raw-css-values, hex colour ban, text-white ban, arbitrary value ban, raw size radius duration
 * WHAT:  A local ESLint plugin (`echo`) with two rules that fail on raw visual values anywhere in src/ except
 *        tokens.css: `no-raw-design-values` for TS/TSX (class strings in className / cn / cva / clsx, `style`
 *        objects, and colour literals in any string) and `no-raw-css-values` for CSS declarations. Also exports
 *        `findRawValues`, the one matcher both rules share.
 * WHY:   Root CLAUDE.md §7 bans hex values, `text-white`, `bg-[#…]`, forced `dark:` classes and improvised
 *        size, radius or duration values; docs/04 is the only source of visual values. globals.css already makes
 *        non-token Tailwind classes generate nothing, and this rule turns that silent no-op into a lint error at
 *        the line that wrote it. The spacing scale is read from tokens.css (the `--space-N` names), so the lint
 *        and the tokens cannot disagree. Unit-less numbers inside calc() and percentages are layout arithmetic,
 *        not design values, so they pass.
 * WHERE: Registered in eslint.config.js for src/ TS/TSX and CSS (tokens.css excluded); tested in
 *        design-tokens.test.mjs (tools Vitest project). Part of `pnpm lint` (local gate, 02 §11).
 */
import { readFileSync } from "node:fs";

const TOKENS_CSS = new URL("../../src/styles/tokens.css", import.meta.url);

/** Spacing steps that exist as `--space-N` tokens (and so as Tailwind `p-N`, `gap-N`, …). */
export const SPACING_SCALE = new Set(
  [...readFileSync(TOKENS_CSS, "utf8").matchAll(/--space-(\d+)\s*:/g)].map((match) => match[1]),
);

/** Functions whose string arguments are class lists. */
export const CLASS_FUNCTIONS = new Set(["cn", "cva", "clsx"]);

const MESSAGES = {
  hex: "Raw colour {{value}}. Use a colour token from docs/04 (e.g. text-fg, bg-accent).",
  colorFunction: "Raw colour function {{value}}. Use a colour token from docs/04.",
  blackWhite: "{{value}} is not a design token. Use a colour token from docs/04 (e.g. text-accent-fg).",
  darkVariant: "Forced `dark:` class {{value}}. Tokens already switch with data-theme and prefers-color-scheme.",
  arbitrary: "Raw value in {{value}}. Use a token from docs/04 or a var(--token) reference.",
  rawScale: "{{value}} is not a docs/04 token. Use the token scale (spacing {{scale}}, named radius/text/shadow).",
  rawNumber: "Raw number {{value}} in a style object. Use a token from docs/04.",
};

/** @typedef {keyof typeof MESSAGES} MessageId */
/** @typedef {{ messageId: MessageId, value: string }} Finding */
/** @typedef {"class" | "css" | "text"} ValueContext */

const HEX = /(?<![\w&#-])#(?:[0-9a-f]{8}|[0-9a-f]{6}|[0-9a-f]{3,4})(?![\w-])/gi;
const COLOR_FUNCTION = /\b(?:rgba?|hsla?|hwb|lab|lch|oklab|oklch|color-mix)\(/gi;
const UNIT_VALUE = /(?<![\w-])-?\d*\.?\d+(?:px|rem|em|pt|ms|s)\b/gi;
const BARE_NUMBER = /^-?\d*\.?\d+$/;
const CSS_NAMED_BLACK_WHITE = /(?<![\w-])(?:white|black)(?![\w-])/gi;
const EASING_FUNCTION = /\bcubic-bezier\(/gi;
const COLOR_CLASS_BLACK_WHITE = /^(?:[a-z]+-)+(?:white|black)(?:\/\d+)?$/;
const SPACING_UTILITY =
  /^-?(?:p[xytrblse]?|m[xytrblse]?|gap(?:-[xy])?|space-[xy]|w|h|size|min-[wh]|max-[wh]|inset(?:-[xy])?|top|right|bottom|left|start|end|translate-[xy]|scroll-[mp][xytrblse]?)-(\d+(?:\.\d+)?)$/;
const NUMERIC_UTILITY = /^-?(?:duration|delay|opacity|z|scale(?:-[xy])?|rotate|blur|brightness|saturate)-\d+$/;
const DEFAULT_SCALE_UTILITY =
  /^(?:text-(?:xs|sm|base|lg|[2-9]?xl)|rounded(?:-[trblse]{1,2})?-(?:md|lg|[2-4]?xl|full)|shadow-(?:xs|sm|md|lg|2?xl)|font-(?:thin|extralight|light|normal|extrabold|black)|leading-\d+|tracking-(?:tighter|tight|wide|wider|widest))$/;

/**
 * Splits one class into its variant prefix and utility, ignoring colons inside [] and ().
 * @param {string} cls
 * @returns {{ variants: string, utility: string }}
 */
export function splitVariants(cls) {
  let depth = 0;
  let cut = 0;
  for (let index = 0; index < cls.length; index += 1) {
    const char = cls[index];
    if (char === "[" || char === "(") depth += 1;
    else if (char === "]" || char === ")") depth -= 1;
    else if (char === ":" && depth === 0) cut = index + 1;
  }
  return { variants: cls.slice(0, cut), utility: cls.slice(cut).replace(/^!/, "") };
}

/**
 * Every top-level `[...]` group in a class.
 * @param {string} cls
 * @returns {string[]}
 */
function bracketGroups(cls) {
  /** @type {string[]} */
  const groups = [];
  let depth = 0;
  let start = -1;
  for (let index = 0; index < cls.length; index += 1) {
    const char = cls[index];
    if (char === "[") {
      if (depth === 0) start = index + 1;
      depth += 1;
    } else if (char === "]") {
      depth -= 1;
      if (depth === 0 && start >= 0) groups.push(cls.slice(start, index));
    }
  }
  return groups;
}

/**
 * Colour literals that are never allowed in src/, whatever the string is for.
 * @param {string} text
 * @returns {Finding[]}
 */
function colorFindings(text) {
  return [
    ...[...text.matchAll(HEX)].map((match) => ({ messageId: /** @type {const} */ ("hex"), value: match[0] })),
    ...[...text.matchAll(COLOR_FUNCTION)].map((match) => ({
      messageId: /** @type {const} */ ("colorFunction"),
      value: match[0],
    })),
  ];
}

/**
 * Raw design values in one Tailwind class.
 * @param {string} cls
 * @returns {Finding[]}
 */
function classFindings(cls) {
  /** @type {Finding[]} */
  const findings = [];
  const { variants, utility } = splitVariants(cls);
  if (/(?:^|:)dark:$/.test(variants) || variants.includes("dark:")) {
    findings.push({ messageId: "darkVariant", value: cls });
  }
  for (const group of bracketGroups(cls)) {
    const content = group.trim();
    if (BARE_NUMBER.test(content) || /\d*\.?\d+(?:px|rem|em|pt|ms|s)\b/i.test(content)) {
      findings.push({ messageId: "arbitrary", value: cls });
    }
  }
  if (COLOR_CLASS_BLACK_WHITE.test(utility)) {
    findings.push({ messageId: "blackWhite", value: cls });
  }
  const spacing = SPACING_UTILITY.exec(utility);
  const step = spacing?.[1];
  if ((step !== undefined && !SPACING_SCALE.has(step)) || NUMERIC_UTILITY.test(utility) || DEFAULT_SCALE_UTILITY.test(utility)) {
    findings.push({ messageId: "rawScale", value: cls });
  }
  return findings;
}

/**
 * Raw design values in `text`, read as a class list, a CSS value or plain text.
 * @param {string} text
 * @param {ValueContext} context
 * @returns {Finding[]}
 */
export function findRawValues(text, context) {
  const findings = colorFindings(text);
  if (context === "class") {
    for (const cls of text.split(/\s+/)) {
      if (cls !== "") findings.push(...classFindings(cls));
    }
  } else if (context === "css") {
    for (const match of text.matchAll(UNIT_VALUE)) {
      findings.push({ messageId: "arbitrary", value: match[0] });
    }
    for (const match of text.matchAll(CSS_NAMED_BLACK_WHITE)) {
      findings.push({ messageId: "blackWhite", value: match[0] });
    }
    for (const match of text.matchAll(EASING_FUNCTION)) {
      findings.push({ messageId: "arbitrary", value: match[0] });
    }
  }
  return findings;
}

/**
 * @param {unknown} value
 * @returns {value is Record<string, unknown>}
 */
function isObject(value) {
  return typeof value === "object" && value !== null;
}

/**
 * The JSX attribute name (`className`, `style`) of an ESTree-shaped node, if it is one.
 * @param {unknown} node
 * @returns {string | undefined}
 */
function jsxAttributeName(node) {
  if (!isObject(node) || node["type"] !== "JSXAttribute") return undefined;
  const name = node["name"];
  return isObject(name) && typeof name["name"] === "string" ? name["name"] : undefined;
}

/**
 * The called function's name when `node` is a call like `cn(…)`.
 * @param {unknown} node
 * @returns {string | undefined}
 */
function calleeName(node) {
  if (!isObject(node) || node["type"] !== "CallExpression") return undefined;
  const callee = node["callee"];
  if (!isObject(callee) || callee["type"] !== "Identifier") return undefined;
  return typeof callee["name"] === "string" ? callee["name"] : undefined;
}

/**
 * How a string at this position is read, from its ancestors (nearest first wins).
 * @param {readonly unknown[]} ancestors
 * @returns {ValueContext}
 */
function valueContext(ancestors) {
  for (let index = ancestors.length - 1; index >= 0; index -= 1) {
    const ancestor = ancestors[index];
    const attribute = jsxAttributeName(ancestor);
    if (attribute === "className") return "class";
    if (attribute === "style") return "css";
    const callee = calleeName(ancestor);
    if (callee !== undefined && CLASS_FUNCTIONS.has(callee)) return "class";
  }
  return "text";
}

/** @type {import("eslint").Rule.RuleModule} */
const noRawDesignValues = {
  meta: {
    type: "problem",
    docs: { description: "Disallow raw visual values outside tokens.css (docs/04)." },
    messages: MESSAGES,
    schema: [],
  },
  create(context) {
    /**
     * @param {import("eslint").Rule.Node} node
     * @param {string} text
     */
    const check = (node, text) => {
      const ancestors = context.sourceCode.getAncestors(node);
      for (const finding of findRawValues(text, valueContext(ancestors))) {
        context.report({ node, messageId: finding.messageId, data: { value: finding.value, scale: [...SPACING_SCALE].join(" ") } });
      }
    };
    return {
      Literal(node) {
        if (typeof node.value === "string") check(node, node.value);
        if (typeof node.value === "number" && node.value !== 0 && valueContext(context.sourceCode.getAncestors(node)) === "css") {
          context.report({ node, messageId: "rawNumber", data: { value: String(node.value) } });
        }
      },
      TemplateElement(node) {
        check(node, node.value.cooked ?? node.value.raw);
      },
    };
  },
};

/** @type {import("@eslint/css").CSSRuleDefinition} */
const noRawCssValues = {
  meta: {
    type: "problem",
    docs: { description: "Disallow raw visual values in CSS outside tokens.css (docs/04)." },
    messages: MESSAGES,
    schema: [],
  },
  create(context) {
    return {
      Declaration(node) {
        const text = context.sourceCode.getText(node.value);
        for (const finding of findRawValues(text, "css")) {
          context.report({ node, messageId: finding.messageId, data: { value: finding.value, scale: "" } });
        }
      },
    };
  },
};

export default {
  meta: { name: "eslint-plugin-echo" },
  rules: {
    "no-raw-design-values": noRawDesignValues,
    "no-raw-css-values": noRawCssValues,
  },
};
