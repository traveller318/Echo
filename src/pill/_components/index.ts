/**
 * SOURCE OF TRUTH KEYWORDS: pill components barrel, Waveform, CountdownRing, StatusGlyph, PillAction
 * WHAT:  Barrel for the pill's own components (used only by src/pill).
 * WHY:   One import path for the pill; they are pill-only, so they live under pill/_components (03 §3), not
 *        components/global.
 * WHERE: src/pill/Pill.tsx.
 */
export { CountdownRing, type CountdownRingProps } from "./CountdownRing";
export { PillAction, type PillActionProps } from "./PillAction";
export { StatusGlyph, type StatusGlyphProps } from "./StatusGlyph";
export { Waveform, type WaveformProps } from "./Waveform";
export { levelFromRms, pushLevel, silentLevels, WAVEFORM_BARS } from "./waveform-levels";
