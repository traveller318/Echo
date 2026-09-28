/**
 * SOURCE OF TRUTH KEYWORDS: waveform levels, pushLevel, silentLevels, WAVEFORM_BARS, waveform bars, level smoothing
 * WHAT:  The waveform's data: WAVEFORM_BARS levels (0–1, oldest first), `pushLevel` (scroll in a new level with light
 *        smoothing) and `silentLevels`; `levelFromRms` is re-exported for the waveform.
 * WHY:   04 §4: 8 bars mapped from AudioLevel.rms with light smoothing. The decibel scale and the smoothing are
 *        lib/audio-level.ts, shared with onboarding's microphone meter so both draw the same event the same way.
 * WHERE: components/global/pill-face/Waveform.tsx; its tests in src/pill/pill-parts.test.ts.
 */
import { smoothLevel } from "@/lib/audio-level";

export { levelFromRms } from "@/lib/audio-level";

/** Bars in the waveform (04 §4). */
export const WAVEFORM_BARS = 8;

export function silentLevels(): readonly number[] {
  return Array.from({ length: WAVEFORM_BARS }, () => 0);
}

/** `levels` scrolled by one, ending with `level` blended into the previous newest one. */
export function pushLevel(levels: readonly number[], level: number): readonly number[] {
  return [...levels.slice(1 - WAVEFORM_BARS), smoothLevel(levels.at(-1) ?? 0, level)];
}
