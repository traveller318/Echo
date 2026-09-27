/**
 * SOURCE OF TRUTH KEYWORDS: pill components barrel, CountdownRing, StatusGlyph, PillAction
 * WHAT:  Barrel for the pill's own components (used only by src/pill).
 * WHY:   One import path for the pill; they are pill-only, so they live under pill/_components (03 §3), not
 *        components/global. The waveform moved to components/global/pill-face once Settings drew pill previews.
 * WHERE: src/pill/Pill.tsx.
 */
export { CountdownRing, type CountdownRingProps } from "./CountdownRing";
export { PillAction, type PillActionProps } from "./PillAction";
export { StatusGlyph, type StatusGlyphProps } from "./StatusGlyph";
