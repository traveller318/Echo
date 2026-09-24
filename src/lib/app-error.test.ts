/**
 * SOURCE OF TRUTH KEYWORDS: app-error test, describeAppError test, toAppError test, calm copy test
 * WHAT:  Checks every AppError code has calm copy, field-dependent copy picks the right text and action, and
 *        toAppError normalizes unknown rejections.
 * WHY:   tsc proves the copy table is exhaustive; these tests prove the copy itself follows 04 §1 and that the
 *        one error surface never receives a non-AppError value.
 * WHERE: Runs in the `web` Vitest project.
 */
import { describe, expect, it } from "vitest";
import type { AppError } from "@/bindings";
import { describeAppError, isAppError, isAppErrorCode, toAppError } from "./app-error";

/** One sample per code; the completeness check below fails if a code is missing. */
const SAMPLES: readonly AppError[] = [
  { code: "Validation", field: "hotkeys.record", message: "Use at least one modifier key." },
  { code: "PermissionDenied", permission: "microphone" },
  { code: "PermissionDenied", permission: "network" },
  { code: "PermissionDenied", permission: "clipboard" },
  { code: "PermissionDenied", permission: "input_injection" },
  { code: "Busy" },
  { code: "NotFound", resource: "transcript" },
  { code: "NotFound", resource: "model" },
  { code: "NotFound", resource: "engine" },
  { code: "NotFound", resource: "setting" },
  { code: "NotFound", resource: "audio_device" },
  { code: "NotFound", resource: "update" },
  { code: "ModelMissing", model_id: "parakeet-tdt-0.6b-v3" },
  { code: "ModelCorrupt", model_id: "parakeet-tdt-0.6b-v3" },
  { code: "AudioDevice" },
  { code: "Asr" },
  { code: "Polish" },
  { code: "Storage" },
  { code: "Offline" },
  { code: "Network" },
  { code: "Hotkey", reason: "conflict" },
  { code: "Hotkey", reason: "invalid" },
  { code: "Internal" },
];

const ALL_CODES = [
  "Validation",
  "PermissionDenied",
  "Busy",
  "NotFound",
  "ModelMissing",
  "ModelCorrupt",
  "AudioDevice",
  "Asr",
  "Polish",
  "Storage",
  "Offline",
  "Network",
  "Hotkey",
  "Internal",
] as const satisfies readonly AppError["code"][];

describe("describeAppError", () => {
  it("covers every code", () => {
    expect(new Set(SAMPLES.map((error) => error.code))).toEqual(new Set(ALL_CODES));
    for (const code of ALL_CODES) {
      expect(isAppErrorCode(code)).toBe(true);
    }
  });

  it.each(SAMPLES)("gives calm copy for $code", (error) => {
    const copy = describeAppError(error);
    for (const text of [copy.title, copy.body, copy.action?.label ?? "Ok"]) {
      expect(text.length).toBeGreaterThan(0);
      expect(text).not.toContain("!");
      expect(text.charAt(0)).toBe(text.charAt(0).toUpperCase());
    }
    expect(copy.title.endsWith(".")).toBe(false);
    expect(copy.body.endsWith(".")).toBe(true);
  });

  it("uses the validation message from Rust as the body", () => {
    expect(describeAppError({ code: "Validation", field: "polish.dictionary", message: "Keep it under 500 entries." }).body).toBe(
      "Keep it under 500 entries.",
    );
  });

  it("points a blocked microphone at the Windows privacy page", () => {
    expect(describeAppError({ code: "PermissionDenied", permission: "microphone" }).action?.id).toBe("open_mic_privacy");
  });

  it("tells a hotkey conflict apart from an invalid combination", () => {
    const conflict = describeAppError({ code: "Hotkey", reason: "conflict" });
    const invalid = describeAppError({ code: "Hotkey", reason: "invalid" });
    expect(conflict.title).not.toBe(invalid.title);
  });
});

describe("toAppError", () => {
  it("keeps a real AppError", () => {
    const error: AppError = { code: "ModelMissing", model_id: "parakeet-tdt-0.6b-v3" };
    expect(toAppError(error)).toBe(error);
    expect(isAppError(error)).toBe(true);
  });

  it.each([new Error("boom"), "invalid args", null, undefined, 42, {}, { code: "Nope" }, { code: 7 }])(
    "turns %s into Internal",
    (value) => {
      expect(isAppError(value)).toBe(false);
      expect(toAppError(value)).toEqual({ code: "Internal" });
    },
  );
});
