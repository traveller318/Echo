/**
 * SOURCE OF TRUTH KEYWORDS: pillKind test, pill state table test, pill width tokens test, idle pill test, pill style width test
 * WHAT:  Table test of every session status (and outcome / error) against the pill layout it shows, with the pill
 *        kept on screen (idle layout) or not, and of the width token each layout takes in each style.
 * WHY:   The pill must follow docs/04 §4 exactly; a status mapped to the wrong layout (or a discarded take left on
 *        screen) is visible to every user on every take.
 * WHERE: Runs in the `web` Vitest project.
 */
import { describe, expect, it } from "vitest";
import type { PillLook, SessionView } from "@/bindings";
import { PILL_WIDTH_TOKENS, pillKind, pillWidthToken, type PillKind } from "./pill-layout";

const ONLY_RECORDING: PillLook = { visibility: "recording", style: "full", movable: false };
const ALWAYS: PillLook = { ...ONLY_RECORDING, visibility: "always" };

const IDLE: SessionView = {
  status: "idle",
  transcript_id: null,
  elapsed_ms: 0,
  countdown_remaining_ms: null,
  outcome: null,
  error: null,
};

function view(patch: Partial<SessionView>): SessionView {
  return { ...IDLE, ...patch };
}

describe("pillKind", () => {
  it("maps every status to its 04 §4 layout", () => {
    expect(pillKind(null, ONLY_RECORDING)).toBeNull();
    expect(pillKind(IDLE, ONLY_RECORDING)).toBeNull();
    expect(pillKind(view({ status: "discarded" }), ONLY_RECORDING)).toBeNull();
    expect(pillKind(view({ status: "arming" }), ONLY_RECORDING)).toBe("recording");
    expect(pillKind(view({ status: "recording" }), ONLY_RECORDING)).toBe("recording");
    expect(pillKind(view({ status: "cancel_pending", countdown_remaining_ms: 3000 }), ONLY_RECORDING)).toBe("cancel");
    expect(pillKind(view({ status: "done", outcome: "pasted" }), ONLY_RECORDING)).toBeNull();
    expect(pillKind(view({ status: "done", outcome: "copied" }), ONLY_RECORDING)).toBe("copied");
    expect(pillKind(view({ status: "done", outcome: "no_speech" }), ONLY_RECORDING)).toBe("no_speech");
    expect(pillKind(view({ status: "done", outcome: "shown" }), ONLY_RECORDING)).toBeNull();
    expect(pillKind(view({ status: "done" }), ONLY_RECORDING)).toBeNull();
    expect(pillKind(view({ status: "failed", error: { code: "Asr" } }), ONLY_RECORDING)).toBe("error");
    expect(
      pillKind(view({ status: "failed", error: { code: "ModelMissing", model_id: "parakeet-tdt-0.6b-v3" } }), ONLY_RECORDING),
    ).toBe("model_missing");
  });

  it("leaves as soon as the take stops, with no transcribing layout", () => {
    for (const status of ["finalizing", "delivering"] as const) {
      expect(pillKind(view({ status, transcript_id: "take" }), ONLY_RECORDING)).toBeNull();
    }
  });

  it("rests instead of leaving whenever a take shows nothing and the pill stays on screen", () => {
    expect(pillKind(null, ALWAYS)).toBeNull();
    for (const status of ["idle", "discarded", "finalizing", "delivering"] as const) {
      expect(pillKind(view({ status }), ALWAYS)).toBe("idle");
    }
    expect(pillKind(view({ status: "done", outcome: "pasted" }), ALWAYS)).toBe("idle");
    expect(pillKind(view({ status: "recording" }), ALWAYS)).toBe("recording");
    expect(pillKind(view({ status: "done", outcome: "copied" }), ALWAYS)).toBe("copied");
    expect(pillKind(IDLE, null)).toBeNull();
  });

  it("names a distinct width token for every layout, per style for the styled ones", () => {
    const tokens = Object.values(PILL_WIDTH_TOKENS);
    expect(new Set(tokens).size).toBe(tokens.length);
    expect(tokens.every((token) => token.startsWith("--pill-width-"))).toBe(true);
    expect(pillWidthToken("recording", "full")).toBe("--pill-width-recording");
    expect(pillWidthToken("idle", "full")).toBe("--pill-width-idle");
    for (const style of ["icon", "mono"] as const) {
      expect(pillWidthToken("recording", style)).toBe("--pill-width-recording-compact");
      expect(pillWidthToken("idle", style)).toBe("--pill-width-mark");
    }
    const shared: PillKind[] = ["cancel", "copied", "no_speech", "error", "model_missing"];
    for (const kind of shared) {
      expect(pillWidthToken(kind, "mono")).toBe(pillWidthToken(kind, "full"));
    }
  });
});
