/**
 * SOURCE OF TRUTH KEYWORDS: waveform levels, levelFromRms, pushLevel, silentLevels, WAVEFORM_BARS, dBFS mapping, level smoothing
 * WHAT:  The waveform's data: WAVEFORM_BARS levels (0–1, oldest first), `levelFromRms` (an AudioLevel RMS on a
 *        decibel scale), `pushLevel` (scroll in a new level with light smoothing) and `silentLevels`.
 * WHY:   04 §4: 20 bars mapped from AudioLevel.rms with light smoothing. Speech RMS spans about -50 to -10 dBFS, so a
 *        linear RMS scale would leave every bar near the floor; a -60 dB floor maps a quiet room to the minimum bar and
 *        normal speech to the middle of the range. Blending each new level with the previous one keeps bars from
 *        jumping on single noisy frames without lagging behind speech (AudioLevel arrives at up to 30 Hz).
 * WHERE: pill/_components/Waveform.tsx.
 */

/** Bars in the waveform (04 §4). */
export const WAVEFORM_BARS = 20;

/** The level (dBFS) drawn as the shortest bar; anything quieter is silence. */
const FLOOR_DB = -60;

/** How much of the previous level a new one keeps. */
const SMOOTHING = 0.35;

/** The quietest RMS measured, so log10 never sees 0. */
const MIN_RMS = 1e-6;

export function silentLevels(): readonly number[] {
  return Array.from({ length: WAVEFORM_BARS }, () => 0);
}

/** An RMS (0–1) as a bar level (0–1) on a decibel scale. */
export function levelFromRms(rms: number): number {
  if (!Number.isFinite(rms) || rms <= 0) {
    return 0;
  }
  const decibels = 20 * Math.log10(Math.max(rms, MIN_RMS));
  return Math.min(1, Math.max(0, (decibels - FLOOR_DB) / -FLOOR_DB));
}

/** `levels` scrolled by one, ending with `level` blended into the previous newest one. */
export function pushLevel(levels: readonly number[], level: number): readonly number[] {
  const previous = levels.at(-1) ?? 0;
  const smoothed = previous * SMOOTHING + level * (1 - SMOOTHING);
  return [...levels.slice(1 - WAVEFORM_BARS), smoothed];
}
