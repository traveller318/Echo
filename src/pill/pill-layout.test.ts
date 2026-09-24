/**
 * SOURCE OF TRUTH KEYWORDS: pillKind test, pill state table test, pill width tokens test
 * WHAT:  Table test of every session status (and outcome / error) against the pill layout it shows.
 * WHY:   The pill must follow docs/04 §4 exactly; a status mapped to the wrong layout (or a discarded take left on
 *        screen) is visible to every user on every take.
 * WHERE: Runs in the `web` Vitest project.
 */
import { describe, expect, it } from "vitest";
import type { SessionView } from "@/bindings";
import { isFinishing, PILL_WIDTH_TOKENS, pillKind } from "./pill-layout";

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
    expect(pillKind(null, false)).toBeNull();
    expect(pillKind(IDLE, false)).toBeNull();
    expect(pillKind(view({ status: "discarded" }), false)).toBeNull();
    expect(pillKind(view({ status: "arming" }), false)).toBe("recording");
    expect(pillKind(view({ status: "recording" }), false)).toBe("recording");
    expect(pillKind(view({ status: "cancel_pending", countdown_remaining_ms: 3000 }), false)).toBe("cancel");
    expect(pillKind(view({ status: "done", outcome: "pasted" }), false)).toBe("done");
    expect(pillKind(view({ status: "done", outcome: "copied" }), false)).toBe("copied");
    expect(pillKind(view({ status: "done", outcome: "no_speech" }), false)).toBe("no_speech");
    expect(pillKind(view({ status: "failed", error: { code: "Asr" } }), false)).toBe("error");
    expect(
      pillKind(view({ status: "failed", error: { code: "ModelMissing", model_id: "parakeet-tdt-0.6b-v3" } }), false),
    ).toBe("model_missing");
  });

  it("keeps the recording layout while finishing until the loading delay passed", () => {
    for (const status of ["finalizing", "delivering"] as const) {
      expect(isFinishing(view({ status }))).toBe(true);
      expect(pillKind(view({ status }), false)).toBe("recording");
      expect(pillKind(view({ status }), true)).toBe("processing");
    }
    expect(isFinishing(view({ status: "recording" }))).toBe(false);
    expect(isFinishing(null)).toBe(false);
  });

  it("names a distinct width token for every layout", () => {
    const tokens = Object.values(PILL_WIDTH_TOKENS);
    expect(new Set(tokens).size).toBe(tokens.length);
    expect(tokens.every((token) => token.startsWith("--pill-width-"))).toBe(true);
  });
});
