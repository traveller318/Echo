/**
 * SOURCE OF TRUTH KEYWORDS: setting schema test, Zod builder test, every SettingKind, Rust message parity, wrap unwrap setting value
 * WHAT:  Proves the Zod schema built for each SettingKind accepts and refuses what Rust's validation does, with the
 *        same messages, and that values round-trip through wrapSettingValue / unwrapSettingValue.
 * WHY:   The form and `settings_set` must agree (root CLAUDE.md §5); these cases mirror the Rust tests of
 *        `SettingSpec::validate` (types/settings.rs) and the registry's runtime membership check.
 * WHERE: Runs in the `web` Vitest project.
 */
import { describe, expect, it, vi } from "vitest";
import type { EnumOption } from "@/bindings";
import { settingFormSchema, settingValueSchema, unwrapSettingValue, wrapSettingValue } from "./setting-schema";

vi.mock("@/bindings", () => ({ SETTING_TOKEN_MAX_CHARS: 128 }));

function firstMessage(result: { success: boolean; error?: { issues: readonly { message: string }[] } }): string {
  return result.error?.issues[0]?.message ?? "";
}

const THEMES: EnumOption[] = [
  { value: "system", label: "Match Windows", requires: null },
  { value: "dark", label: "Dark", requires: null },
];

describe("settingValueSchema", () => {
  it("bool accepts booleans only", () => {
    const schema = settingValueSchema("bool", { kind: "bool" });
    expect(schema.safeParse(true).success).toBe(true);
    expect(schema.safeParse("true").success).toBe(false);
  });

  it("int checks bounds inclusively with Rust's message", () => {
    const schema = settingValueSchema("int", { kind: "int", min: 1000, max: 10000, unit: "milliseconds" });
    expect(schema.safeParse(1000).success).toBe(true);
    expect(schema.safeParse(10000).success).toBe(true);
    expect(firstMessage(schema.safeParse(999))).toBe("Choose a number from 1000 to 10000.");
    expect(firstMessage(schema.safeParse(10001))).toBe("Choose a number from 1000 to 10000.");
    expect(schema.safeParse(1500.5).success).toBe(false);
    expect(schema.safeParse(Number.NaN).success).toBe(false);
  });

  it("enum accepts only the options offered now", () => {
    const kind = { kind: "enum", options: { from: "fixed", list: THEMES } } as const;
    const schema = settingValueSchema("enum", kind, { options: THEMES });
    expect(schema.safeParse("dark").success).toBe(true);
    expect(firstMessage(schema.safeParse("sepia"))).toBe("Choose one of the listed options.");
    expect(firstMessage(schema.safeParse(" "))).toBe("Choose an option.");
    const runtime = settingValueSchema(
      "enum",
      { kind: "enum", options: { from: "runtime", source: "asr_engines" } },
      { options: [] },
    );
    expect(firstMessage(runtime.safeParse("parakeet"))).toBe("Choose one of the listed options.");
  });

  it("hotkey needs a non-blank, well-formed combination", () => {
    const schema = settingValueSchema("hotkey", { kind: "hotkey" });
    expect(schema.safeParse("Ctrl+Alt+Space").success).toBe(true);
    expect(firstMessage(schema.safeParse(""))).toBe("Press a key combination.");
    expect(firstMessage(schema.safeParse("x".repeat(129)))).toBe("This value is not valid.");
    expect(firstMessage(schema.safeParse("Ctrl+\u0000"))).toBe("This value is not valid.");
  });

  it("device is the system default (null) or a device id", () => {
    const schema = settingValueSchema("device", { kind: "device" });
    expect(schema.safeParse(null).success).toBe(true);
    expect(schema.safeParse("wasapi:{0.0.1}").success).toBe(true);
    expect(firstMessage(schema.safeParse(""))).toBe("Choose a microphone.");
  });

  it("text counts code points and allows only line breaks and tabs as control characters", () => {
    const schema = settingValueSchema("text", { kind: "text", max_len: 5 });
    expect(schema.safeParse("héllo").success).toBe(true);
    expect(schema.safeParse("😀😀😀😀😀").success).toBe(true);
    expect(schema.safeParse("a\nb\tc").success).toBe(true);
    expect(firstMessage(schema.safeParse("toolong"))).toBe("Use at most 5 characters.");
    expect(firstMessage(schema.safeParse("a\u0007"))).toBe("Remove unsupported characters.");
  });

  it("pairs enforce count, words, lengths, characters and case-insensitive uniqueness", () => {
    const schema = settingValueSchema("pairs", { kind: "pairs", max_pairs: 2, max_len: 10 });
    const pair = (from: string, to: string) => ({ from, to });
    expect(schema.safeParse([pair("echo", "Echo"), pair("um", "")]).success).toBe(true);
    expect(firstMessage(schema.safeParse([pair("a", "b"), pair("c", "d"), pair("e", "f")]))).toBe(
      "Use at most 2 entries.",
    );
    expect(firstMessage(schema.safeParse([pair("  ", "x")]))).toBe("Every entry needs a word to replace.");
    const duplicate = schema.safeParse([pair("echo", "Echo"), pair("ECHO ", "E")]);
    expect(firstMessage(duplicate)).toBe('"ECHO" is listed more than once.');
    expect(duplicate.error?.issues[0]?.path).toEqual([1, "from"]);
    const long = schema.safeParse([pair("echo", "a very long text")]);
    expect(firstMessage(long)).toBe("Keep each entry to 10 characters or fewer.");
    expect(long.error?.issues[0]?.path).toEqual([0, "to"]);
    expect(firstMessage(schema.safeParse([pair("ec\tho", "Echo")]))).toBe("Remove unsupported characters.");
  });
});

describe("settingFormSchema", () => {
  it("wraps the value schema in the form shape", () => {
    const schema = settingFormSchema("int", { kind: "int", min: 10, max: 200, unit: "words_per_minute" });
    expect(schema.safeParse({ value: 40 }).success).toBe(true);
    const refused = schema.safeParse({ value: 5 });
    expect(refused.error?.issues[0]?.path).toEqual(["value"]);
  });
});

describe("wrapSettingValue / unwrapSettingValue", () => {
  it("round-trips every kind and refuses another kind", () => {
    expect(wrapSettingValue("bool", true)).toEqual({ kind: "bool", value: true });
    expect(wrapSettingValue("device", null)).toEqual({ kind: "device", value: null });
    expect(wrapSettingValue("pairs", [{ from: "a", to: "b" }])).toEqual({
      kind: "pairs",
      value: [{ from: "a", to: "b" }],
    });
    expect(unwrapSettingValue("int", { kind: "int", value: 3000 })).toBe(3000);
    expect(unwrapSettingValue("hotkey", { kind: "hotkey", value: "Ctrl+Alt" })).toBe("Ctrl+Alt");
    expect(unwrapSettingValue("text", { kind: "text", value: "note" })).toBe("note");
    expect(unwrapSettingValue("enum", { kind: "enum", value: "dark" })).toBe("dark");
    expect(unwrapSettingValue("int", { kind: "bool", value: true })).toBeUndefined();
  });
});
