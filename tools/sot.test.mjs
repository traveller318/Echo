/**
 * SOURCE OF TRUTH KEYWORDS: sot test, SOT header parser test, keyword search test, sot CLI test
 * WHAT:  Unit tests for tools/sot.mjs: header parsing in every comment style, term matching, the directory
 *        walk's skip rules, output formatting and the CLI exit codes.
 * WHY:   Every agent relies on `pnpm sot` to find owners before creating code; a parser regression would
 *        silently hide files and cause duplicates.
 * WHERE: Runs in the `tools` Vitest project (node environment).
 */
import { mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { formatMatches, main, matchesTerms, parseSotHeaders, searchSot, stripCommentMarkers } from "./sot.mjs";

const RUST_FILE = [
  "/*!",
  " * SOURCE OF TRUTH KEYWORDS: AppError, error codes, IPC error",
  " * WHAT:  The only error that crosses IPC.",
  " * WHY:   One error surface for the UI.",
  " * WHERE: Returned by every command.",
  " */",
  "",
  "/**",
  " * SOURCE OF TRUTH KEYWORDS: map_error, internal error",
  " * WHAT:  Maps internal errors.",
  " * @param error the internal error",
  " */",
  "pub fn map_error() {}",
  'const TEXT: &str = "SOURCE OF TRUTH KEYWORDS: not a header";',
].join("\n");

describe("stripCommentMarkers", () => {
  it.each([
    [" * WHAT:  text", "WHAT:  text"],
    ["/** SOURCE", "SOURCE"],
    ["/*! SOURCE", "SOURCE"],
    ["//! SOURCE", "SOURCE"],
    ["# SOURCE", "SOURCE"],
    ["-- SOURCE", "SOURCE"],
    ["<!-- SOURCE -->", "SOURCE"],
    [" */", ""],
  ])("strips %j to %j", (input, expected) => {
    expect(stripCommentMarkers(input)).toBe(expected);
  });
});

describe("parseSotHeaders", () => {
  it("finds file and block headers and stops at the comment end or a JSDoc tag", () => {
    const headers = parseSotHeaders(RUST_FILE);

    expect(headers).toHaveLength(2);
    expect(headers[0]).toEqual({
      line: 2,
      keywords: "AppError, error codes, IPC error",
      block: [
        "SOURCE OF TRUTH KEYWORDS: AppError, error codes, IPC error",
        "WHAT:  The only error that crosses IPC.",
        "WHY:   One error surface for the UI.",
        "WHERE: Returned by every command.",
      ],
    });
    expect(headers[1]?.line).toBe(9);
    expect(headers[1]?.block).toEqual(["SOURCE OF TRUTH KEYWORDS: map_error, internal error", "WHAT:  Maps internal errors."]);
  });

  it("ignores the marker inside string literals", () => {
    const keywords = parseSotHeaders(RUST_FILE).map((header) => header.keywords);
    expect(keywords).not.toContain("not a header");
  });

  it("reads line-comment headers until the prefix changes", () => {
    const toml = ["# SOURCE OF TRUTH KEYWORDS: clippy config", "# WHAT:  Lint settings.", "[lints]", "# unrelated"].join("\n");
    expect(parseSotHeaders(toml)).toEqual([
      { line: 1, keywords: "clippy config", block: ["SOURCE OF TRUTH KEYWORDS: clippy config", "WHAT:  Lint settings."] },
    ]);
  });

  it("reads HTML comment headers whose lines carry no prefix", () => {
    const html = ["<!--", "  SOURCE OF TRUTH KEYWORDS: pill entry", "  WHAT:  Pill HTML.", "-->", "<!doctype html>"].join("\n");
    expect(parseSotHeaders(html)[0]?.block).toEqual(["SOURCE OF TRUTH KEYWORDS: pill entry", "WHAT:  Pill HTML."]);
  });

  it("handles CRLF line endings", () => {
    expect(parseSotHeaders("/**\r\n * SOURCE OF TRUTH KEYWORDS: crlf\r\n */")[0]?.keywords).toBe("crlf");
  });
});

describe("matchesTerms", () => {
  const header = { line: 1, keywords: "AppError, error codes, IPC error", block: [] };

  it("matches case-insensitively", () => {
    expect(matchesTerms(header, ["apperror"])).toBe(true);
  });

  it("requires every term", () => {
    expect(matchesTerms(header, ["ipc", "codes"])).toBe(true);
    expect(matchesTerms(header, ["ipc", "session"])).toBe(false);
  });
});

describe("searchSot and main", () => {
  /** @type {string} */
  let repo = "";

  /**
   * @param {string} path repo-relative
   * @param {string} text
   */
  const put = async (path, text) => {
    const target = join(repo, path);
    await mkdir(dirname(target), { recursive: true });
    await writeFile(target, text, "utf8");
  };

  beforeEach(async () => {
    repo = await mkdtemp(join(tmpdir(), "echo-sot-"));
    await put("src-tauri/src/types/error.rs", RUST_FILE);
    await put("src/lib/format.ts", "/**\n * SOURCE OF TRUTH KEYWORDS: formatDuration, IPC display\n */\n");
    await put("src/bindings.ts", "/**\n * SOURCE OF TRUTH KEYWORDS: generated IPC\n */\n");
    await put("src/node_modules/pkg/index.ts", "/**\n * SOURCE OF TRUTH KEYWORDS: vendored IPC\n */\n");
    await put("src/readme.md", "SOURCE OF TRUTH KEYWORDS: markdown IPC\n");
    await put("docs/notes.ts", "/**\n * SOURCE OF TRUTH KEYWORDS: outside roots IPC\n */\n");
  });

  afterEach(async () => {
    vi.restoreAllMocks();
    await rm(repo, { recursive: true, force: true });
  });

  it("searches only the scan roots and skips generated, vendored and non-source files", async () => {
    const matches = await searchSot(repo, ["ipc"]);
    expect(matches.map(({ file, header }) => `${file}:${String(header.line)}`)).toEqual([
      "src-tauri/src/types/error.rs:2",
      "src/lib/format.ts:2",
    ]);
  });

  it("formats one line per match, or full blocks with --show", async () => {
    const matches = await searchSot(repo, ["AppError"]);
    expect(formatMatches(matches, false)).toBe("src-tauri/src/types/error.rs:2  AppError, error codes, IPC error");
    expect(formatMatches(matches, true)).toContain("src-tauri/src/types/error.rs:2\n  SOURCE OF TRUTH KEYWORDS:");
  });

  it("returns 2 without a term and 0 when nothing matches", async () => {
    const stderr = vi.spyOn(process.stderr, "write").mockReturnValue(true);
    const stdout = vi.spyOn(process.stdout, "write").mockReturnValue(true);

    await expect(main([], repo)).resolves.toBe(2);
    expect(stderr).toHaveBeenCalled();
    await expect(main(["no-such-keyword"], repo)).resolves.toBe(0);
    expect(stdout).toHaveBeenCalledWith(expect.stringContaining("No SOURCE OF TRUTH header mentions: no-such-keyword"));
  });

  it("returns 0 when the scan roots do not exist yet", async () => {
    const empty = await mkdtemp(join(tmpdir(), "echo-sot-empty-"));
    try {
      await expect(searchSot(empty, ["anything"])).resolves.toEqual([]);
    } finally {
      await rm(empty, { recursive: true, force: true });
    }
  });
});
