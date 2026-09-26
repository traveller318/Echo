/**
 * SOURCE OF TRUTH KEYWORDS: audio level, levelFromRms, smoothLevel, AudioLevel display, dBFS mapping, level meter scale, level smoothing
 * WHAT:  How an AudioLevel RMS is drawn: `levelFromRms` maps it onto a 0–1 decibel scale, `smoothLevel` blends a new
 *        level into the previous one.
 * WHY:   Speech RMS spans about -50 to -10 dBFS, so a linear RMS scale would leave every meter near the floor; a -60 dB
 *        floor maps a quiet room to the bottom and normal speech to the middle. Blending each level with the previous
 *        one keeps a meter from jumping on single noisy frames without lagging behind speech (AudioLevel arrives at up
 *        to 30 Hz). The pill's waveform and onboarding's microphone meter draw the same event, so they share one
 *        scale and one smoothing.
 * WHERE: pill/_components/waveform-levels.ts (the waveform bars); hooks/use-audio-level.ts (level meters).
 */

/** The level (dBFS) drawn as the lowest mark; anything quieter is silence. */
const FLOOR_DB = -60;

/** How much of the previous level a new one keeps. */
const SMOOTHING = 0.35;

/** The quietest RMS measured, so log10 never sees 0. */
const MIN_RMS = 1e-6;

/** An RMS (0–1) as a display level (0–1) on a decibel scale. */
export function levelFromRms(rms: number): number {
  if (!Number.isFinite(rms) || rms <= 0) {
    return 0;
  }
  const decibels = 20 * Math.log10(Math.max(rms, MIN_RMS));
  return Math.min(1, Math.max(0, (decibels - FLOOR_DB) / -FLOOR_DB));
}

/** `level` blended into `previous`. */
export function smoothLevel(previous: number, level: number): number {
  return previous * SMOOTHING + level * (1 - SMOOTHING);
}
