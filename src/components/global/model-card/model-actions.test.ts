/**
 * SOURCE OF TRUTH KEYWORDS: modelActionPlan offered test, onboarding model actions test, secondary action filter test
 * WHAT:  Verifies that a surface offering only some secondary actions (onboarding: import) gets exactly those, in card
 *        order, while the primary action is unchanged; the full table is covered by the Models page test.
 * WHY:   Onboarding must never offer Remove or Check files; a filter that dropped the primary action would leave the
 *        model step without its Download.
 * WHERE: Runs in the `web` Vitest project.
 */
import { describe, expect, it } from "vitest";
import type { ModelEntry } from "@/bindings";
import { ALL_SECONDARY_MODEL_ACTIONS, modelActionPlan } from "./model-actions";

function entry(status: ModelEntry["status"]): ModelEntry {
  return {
    engine: {
      id: "parakeet-tdt-0.6b-v3",
      label: "Parakeet",
      model_id: "parakeet-tdt-0.6b-v3",
      caps: { kind: "vad", frame_ms: 32 },
    },
    model: {
      id: "parakeet-tdt-0.6b-v3",
      label: "Parakeet",
      kind: "model",
      license: "MIT",
      attribution: null,
      revision: "abc",
      files: [],
      archive: null,
      requires: [],
      bundled: false,
    },
    requires: [],
    download_bytes: 1,
    status,
    selection: { kind: "selectable", active: true },
    runtime: null,
    transfer: null,
  };
}

describe("modelActionPlan with offered actions", () => {
  it("keeps the primary action and only the offered secondary ones", () => {
    expect(modelActionPlan(entry({ kind: "partial", bytes: 10 }), null, ["import"])).toEqual({
      primary: "resume",
      secondary: ["import"],
    });
    expect(modelActionPlan(entry({ kind: "installed" }), null, ["import"])).toEqual({
      primary: null,
      secondary: [],
    });
    expect(modelActionPlan(entry({ kind: "corrupt" }), null)).toEqual({
      primary: "redownload",
      secondary: ["import", "remove"],
    });
    expect(ALL_SECONDARY_MODEL_ACTIONS).toEqual(["import", "verify", "remove"]);
  });
});
