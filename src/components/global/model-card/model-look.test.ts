/**
 * SOURCE OF TRUTH KEYWORDS: model-look test, modelStatusLook table, transferSummary test, languagesSummary test
 * WHAT:  Table tests for how a model card reads: the badge for every status, selection, runtime and transfer phase,
 *        the transfer summary line, and the language phrase for each engine kind.
 * WHY:   These strings are what a user sees while waiting on a 670 MB download; a wrong mapping (a "Downloading"
 *        badge while checking) would read as a hang.
 * WHERE: Runs in the `web` Vitest project.
 */
import { describe, expect, it } from "vitest";
import type { EngineCaps, ModelEntry, ModelProgress } from "@/bindings";
import { isDeterminate, languagesSummary, modelStatusLook, transferSummary } from "./model-look";

const ENTRY: ModelEntry = {
  engine: {
    id: "e",
    label: "Engine",
    model_id: "m",
    caps: {
      kind: "asr",
      languages: ["en"],
      auto_language: true,
      punctuation: true,
      casing: true,
      accelerators: ["cpu"],
      max_segment_s: 30,
    },
  },
  model: {
    id: "m",
    label: "Model",
    kind: "model",
    license: "MIT",
    attribution: null,
    revision: "r",
    files: [],
    archive: null,
    requires: [],
    bundled: false,
  },
  requires: [],
  download_bytes: 0,
  status: { kind: "installed" },
  selection: { kind: "selectable", active: true },
  runtime: null,
  transfer: null,
};

const MB = 1_048_576;

function progress(phase: ModelProgress["phase"], bytes = 300 * MB): ModelProgress {
  return { model_id: "m", bytes, total: 600 * MB, phase };
}

describe("modelStatusLook", () => {
  it.each([
    [{ status: { kind: "not_installed" } }, "Not installed"],
    [{ status: { kind: "partial", bytes: 1 } }, "Paused"],
    [{ status: { kind: "corrupt" } }, "Damaged"],
    [{}, "In use"],
    [{ runtime: { kind: "loading" } }, "Loading"],
    [{ runtime: { kind: "failed", error: { code: "ModelCorrupt", model_id: "m" } } }, "Couldn't load"],
    [{ selection: { kind: "selectable", active: false } }, "Installed"],
    [{ selection: { kind: "built_in" } }, "Built in"],
  ] as const)("%j reads %s", (overrides, label) => {
    expect(modelStatusLook({ ...ENTRY, ...overrides }, null).label).toBe(label);
  });

  it("a running transfer wins over the stored status", () => {
    expect(modelStatusLook(ENTRY, progress("transferring")).label).toBe("Downloading");
    expect(modelStatusLook(ENTRY, progress("waiting")).label).toBe("Waiting for connection");
    expect(modelStatusLook(ENTRY, progress("verifying")).label).toBe("Checking");
  });
});

describe("transferSummary", () => {
  it("says how far while bytes move and what happens after", () => {
    expect(transferSummary(progress("transferring"), "en-US")).toBe("300 MB of 600 MB");
    expect(transferSummary(progress("waiting"), "en-US")).toMatch(/^300 MB of 600 MB\. The connection dropped/);
    expect(transferSummary(progress("verifying"), "en-US")).toBe("Checking every file");
    expect(transferSummary(progress("cancelled"), "en-US")).toBe("Paused at 300 MB of 600 MB");
    expect(isDeterminate(progress("transferring"))).toBe(true);
    expect(isDeterminate(progress("installing"))).toBe(false);
    expect(isDeterminate({ ...progress("transferring"), total: 0 })).toBe(false);
  });
});

describe("languagesSummary", () => {
  const asr = (languages: string[]): EngineCaps => {
    const caps = ENTRY.engine.caps;
    return caps.kind === "asr" ? { ...caps, languages } : caps;
  };

  it("names a few languages and counts the rest", () => {
    expect(languagesSummary(asr(["en", "de"]), "en")).toBe("English, German");
    expect(languagesSummary(asr(["en", "de", "fr", "es", "it"]), "en")).toBe("English, German, French and 2 more");
    expect(languagesSummary({ kind: "vad", frame_ms: 32 }, "en")).toBeNull();
    expect(
      languagesSummary({ kind: "polisher", latency_class: "slow", languages: { kind: "any" }, needs_model: true }, "en"),
    ).toBe("Any language");
  });
});
