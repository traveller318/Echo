/**
 * SOURCE OF TRUTH KEYWORDS: sot search, pnpm sot, sot:show, SOURCE OF TRUTH header parser, keyword search, codebase navigation
 * WHAT:  Read-only CLI that lists files whose `SOURCE OF TRUTH KEYWORDS:` line contains every given term
 *        (case-insensitive); `--show` also prints each matching header block (KEYWORDS/WHAT/WHY/WHERE) with file:line.
 * WHY:   Lets an agent find the owner of a symbol without reading the codebase (root CLAUDE.md §2). It is the one
 *        tooling script the rulebook allows, so it must stay dependency-free and never write anything (03 §5).
 *        Headers come in several comment styles (Rust `/*!`, TS `/**`, CSS `/*`, HTML `<!--`, SQL `--`, TOML `#`);
 *        a marker only counts when it opens a comment line, so string literals that mention it are ignored.
 * WHERE: `pnpm sot <terms…>` and `pnpm sot:show <terms…>` (package.json). Exported helpers are tested in sot.test.mjs.
 */
import { readdir, readFile } from "node:fs/promises";
import { extname, join, relative, resolve, sep } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

export const MARKER = "SOURCE OF TRUTH KEYWORDS:";
export const SCAN_ROOTS = ["src", "src-tauri/src"];
export const SKIP_DIRS = new Set(["node_modules", "target"]);
export const SKIP_FILES = new Set(["bindings.ts"]);
export const SOURCE_EXTENSIONS = new Set([
  ".rs",
  ".ts",
  ".tsx",
  ".js",
  ".jsx",
  ".mjs",
  ".cjs",
  ".css",
  ".html",
  ".sql",
  ".toml",
]);

/** Longest header block we will read before giving up on finding its end. */
const MAX_BLOCK_LINES = 40;
const LEADING_MARKER = /^\s*(?:\/\*[*!]?|\*(?!\/)|\/\/[/!]?|<!--|--(?!>)|#)?\s*/;
const TRAILING_MARKER = /\s*(?:\*\/|-->)\s*$/;
const LINE_COMMENT = /^\s*(\/\/[/!]?|--|#)/;
const BLOCK_CLOSE = /\*\/|-->/;

/**
 * @typedef {object} SotHeader
 * @property {number} line      1-based line of the KEYWORDS marker.
 * @property {string} keywords  Text after the marker.
 * @property {string[]} block   Header lines with comment markers stripped, starting at the KEYWORDS line.
 */

/**
 * @typedef {object} SotMatch
 * @property {string} file      Repo-relative path with forward slashes.
 * @property {SotHeader} header
 */

/**
 * SOURCE OF TRUTH KEYWORDS: stripCommentMarkers, comment prefix, comment suffix
 * WHAT:  Removes a leading comment marker (`/**`, `/*!`, `*`, `//`, `//!`, `<!--`, `--`, `#`) and a trailing `*\/`/`-->`.
 * WHY:   One normaliser for every comment style keeps parsing and `--show` output identical across languages.
 * WHERE: Used by parseSotHeaders.
 * @param {string} line
 * @returns {string}
 */
export function stripCommentMarkers(line) {
  return line.replace(LEADING_MARKER, "").replace(TRAILING_MARKER, "").trimEnd();
}

/**
 * SOURCE OF TRUTH KEYWORDS: parseSotHeaders, header block, header extraction
 * WHAT:  Returns every SOT header in a file's text with its line number, keywords and stripped block lines.
 * WHY:   Line-comment headers end at the first line without the same prefix; block headers end at their close
 *        delimiter. A cap stops a missing delimiter from swallowing the rest of the file.
 * WHERE: Used by searchSot.
 * @param {string} text
 * @returns {SotHeader[]}
 */
export function parseSotHeaders(text) {
  const lines = text.split(/\r?\n/);
  /** @type {SotHeader[]} */
  const headers = [];

  lines.forEach((rawLine, index) => {
    const stripped = stripCommentMarkers(rawLine);
    if (!stripped.startsWith(MARKER)) {
      return;
    }
    const lineStyle = LINE_COMMENT.exec(rawLine)?.[1];
    const block = [stripped];
    const last = Math.min(lines.length, index + MAX_BLOCK_LINES);

    for (let next = index + 1; !BLOCK_CLOSE.test(rawLine) && next < last; next += 1) {
      const candidate = lines[next] ?? "";
      if (lineStyle !== undefined && !candidate.trimStart().startsWith(lineStyle)) {
        break;
      }
      const content = stripCommentMarkers(candidate);
      // JSDoc tags (`@param`, `@returns`) follow the header inside the same comment; they are not part of it.
      if (content.startsWith("@")) {
        break;
      }
      if (content.length > 0) {
        block.push(content);
      }
      if (lineStyle === undefined && BLOCK_CLOSE.test(candidate)) {
        break;
      }
    }

    headers.push({ line: index + 1, keywords: stripped.slice(MARKER.length).trim(), block });
  });

  return headers;
}

/**
 * @param {SotHeader} header
 * @param {string[]} terms
 * @returns {boolean} true when every term appears in the header's keywords, ignoring case.
 */
export function matchesTerms(header, terms) {
  const haystack = header.keywords.toLowerCase();
  return terms.every((term) => haystack.includes(term.toLowerCase()));
}

/**
 * SOURCE OF TRUTH KEYWORDS: collectSourceFiles, directory walk, skip node_modules, skip target
 * WHAT:  Recursively lists source files under a directory, skipping build/dependency folders and generated files.
 * WHY:   A missing root (e.g. before src-tauri exists) is not an error; it simply contributes no files.
 * WHERE: Used by searchSot for each entry in SCAN_ROOTS.
 * @param {string} directory absolute path
 * @returns {Promise<string[]>} absolute file paths
 */
export async function collectSourceFiles(directory) {
  /** @type {import("node:fs").Dirent[]} */
  let entries;
  try {
    entries = await readdir(directory, { withFileTypes: true });
  } catch (error) {
    if (error instanceof Error && "code" in error && error.code === "ENOENT") {
      return [];
    }
    throw error;
  }

  const nested = await Promise.all(
    entries.map(async (entry) => {
      const path = join(directory, entry.name);
      if (entry.isDirectory()) {
        return SKIP_DIRS.has(entry.name) ? [] : collectSourceFiles(path);
      }
      const wanted = entry.isFile() && SOURCE_EXTENSIONS.has(extname(entry.name)) && !SKIP_FILES.has(entry.name);
      return wanted ? [path] : [];
    }),
  );
  return nested.flat();
}

/**
 * SOURCE OF TRUTH KEYWORDS: searchSot, keyword match, sot results
 * WHAT:  Scans SCAN_ROOTS under repoRoot and returns every header whose keywords contain all terms, sorted by file and line.
 * WHY:   Sorting keeps output stable between runs so agents can diff or cite it.
 * WHERE: Used by main (CLI) and tests.
 * @param {string} repoRoot absolute path
 * @param {string[]} terms
 * @returns {Promise<SotMatch[]>}
 */
export async function searchSot(repoRoot, terms) {
  const files = (await Promise.all(SCAN_ROOTS.map((root) => collectSourceFiles(resolve(repoRoot, root))))).flat();
  /** @type {SotMatch[]} */
  const matches = [];

  for (const path of files.sort()) {
    const text = await readFile(path, "utf8");
    const file = relative(repoRoot, path).split(sep).join("/");
    for (const header of parseSotHeaders(text)) {
      if (matchesTerms(header, terms)) {
        matches.push({ file, header });
      }
    }
  }
  return matches;
}

/**
 * @param {SotMatch[]} matches
 * @param {boolean} show print full header blocks instead of one line per match
 * @returns {string}
 */
export function formatMatches(matches, show) {
  if (show) {
    return matches
      .map(({ file, header }) => [`${file}:${String(header.line)}`, ...header.block.map((line) => `  ${line}`)].join("\n"))
      .join("\n\n");
  }
  return matches.map(({ file, header }) => `${file}:${String(header.line)}  ${header.keywords}`).join("\n");
}

const USAGE = [
  "Usage: pnpm sot <term> [more terms…]       files whose SOURCE OF TRUTH KEYWORDS line contains every term",
  "       pnpm sot:show <term> [more terms…]  the same, printing each matching header block",
].join("\n");

/**
 * SOURCE OF TRUTH KEYWORDS: sot main, sot CLI, exit code
 * WHAT:  CLI entry: parses argv, runs the search from the repo root and prints results; returns the exit code.
 * WHY:   Returns a code instead of calling process.exit so pending output always flushes. No match is a normal
 *        answer (exit 0 with a message); only a missing term is a usage error (exit 2).
 * WHERE: Invoked at the bottom of this file when run directly by node.
 * @param {string[]} argv arguments after the script path
 * @param {string} repoRoot absolute path
 * @returns {Promise<number>}
 */
export async function main(argv, repoRoot) {
  const show = argv.includes("--show");
  const terms = argv.filter((arg) => !arg.startsWith("--") && arg.trim().length > 0);
  if (argv.includes("--help") || terms.length === 0) {
    process.stderr.write(`${USAGE}\n`);
    return terms.length === 0 && !argv.includes("--help") ? 2 : 0;
  }

  const matches = await searchSot(repoRoot, terms);
  const output =
    matches.length === 0
      ? `No SOURCE OF TRUTH header mentions: ${terms.join(" ")}\nSearch the codebase with rg before creating anything new.`
      : formatMatches(matches, show);
  process.stdout.write(`${output}\n`);
  return 0;
}

const invokedPath = process.argv[1];
if (invokedPath !== undefined && import.meta.url === pathToFileURL(resolve(invokedPath)).href) {
  const repoRoot = fileURLToPath(new URL("..", import.meta.url));
  process.exitCode = await main(process.argv.slice(2), repoRoot);
}
