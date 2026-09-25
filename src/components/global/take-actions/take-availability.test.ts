/**
 * SOURCE OF TRUTH KEYWORDS: takeAvailability test, take action rules test
 * WHAT:  Verifies which take actions each take offers: nothing while a take is still in progress, no copy for
 *        an empty take, no retry without audio.
 * WHY:   Buttons that cannot work would only raise error toasts; the rule is shared by the rows and the sheet.
 * WHERE: Runs in the `web` Vitest project.
 */
import { describe, expect, it } from "vitest";
import { takeAvailability } from "./take-availability";

describe("takeAvailability", () => {
  it("offers everything for a settled take with audio", () => {
    expect(takeAvailability({ status: "done", has_audio: true })).toEqual({ copy: true, retry: true, remove: true });
    expect(takeAvailability({ status: "recoverable", has_audio: true })).toEqual({
      copy: true,
      retry: true,
      remove: true,
    });
  });

  it("offers nothing while a take is still recording or transcribing", () => {
    for (const status of ["recording", "transcribing"] as const) {
      expect(takeAvailability({ status, has_audio: true })).toEqual({ copy: false, retry: false, remove: false });
    }
  });

  it("offers no copy for an empty take and no retry without audio", () => {
    expect(takeAvailability({ status: "empty", has_audio: true }).copy).toBe(false);
    expect(takeAvailability({ status: "failed", has_audio: false })).toEqual({
      copy: true,
      retry: false,
      remove: true,
    });
  });
});
