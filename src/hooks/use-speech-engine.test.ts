/**
 * SOURCE OF TRUTH KEYWORDS: speech engine query test, SPEECH_ENGINE_QUERY test, accelerator status refresh test
 * WHAT:  Verifies the speech engine status refreshes from the Rust events that make it stale and that each named
 *        event exists in the generated bindings.
 * WHY:   The status is never polled (root CLAUDE.md §7); a misspelt event would leave About showing the old
 *        accelerator after a CPU fallback or a new preference.
 * WHERE: Runs in the `web` Vitest project against the real generated bindings (no Tauri call is made).
 */
import { describe, expect, it } from "vitest";
import { ECHO_EVENT_NAMES } from "@/lib/echo-events";
import { SPEECH_ENGINE_QUERY } from "./use-speech-engine";

describe("speech engine query", () => {
  it("refreshes when the engine or its settings change", () => {
    expect(SPEECH_ENGINE_QUERY.invalidatedBy).toEqual(["modelsChanged", "settingsChanged"]);
    for (const name of SPEECH_ENGINE_QUERY.invalidatedBy ?? []) {
      expect(ECHO_EVENT_NAMES).toContain(name);
    }
  });
});
