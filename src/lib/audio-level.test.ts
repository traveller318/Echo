/**
 * SOURCE OF TRUTH KEYWORDS: audio level test, levelFromRms test, smoothLevel test, decibel scale test
 * WHAT:  Verifies the shared level scale: silence and garbage read as 0, full scale as 1, -20 dBFS two thirds up, and
 *        smoothing keeps part of the previous level.
 * WHY:   The pill's waveform and onboarding's meter draw every AudioLevel through these; a wrong floor would leave
 *        both flat for normal speech.
 * WHERE: Runs in the `web` Vitest project.
 */
import { describe, expect, it } from "vitest";
import { levelFromRms, smoothLevel } from "./audio-level";

describe("audio level", () => {
  it("maps RMS onto a -60 dB scale", () => {
    expect(levelFromRms(0)).toBe(0);
    expect(levelFromRms(Number.NaN)).toBe(0);
    expect(levelFromRms(-1)).toBe(0);
    expect(levelFromRms(0.001)).toBe(0);
    expect(levelFromRms(0.1)).toBeCloseTo(2 / 3);
    expect(levelFromRms(1)).toBe(1);
    expect(levelFromRms(4)).toBe(1);
  });

  it("smooths a new level towards the previous one", () => {
    expect(smoothLevel(0, 1)).toBeCloseTo(0.65);
    expect(smoothLevel(1, 0)).toBeCloseTo(0.35);
    expect(smoothLevel(0.5, 0.5)).toBeCloseTo(0.5);
  });
});
