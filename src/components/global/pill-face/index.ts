/**
 * SOURCE OF TRUTH KEYWORDS: pill-face barrel, PillMark, PillRest, PillPreview, PILL_STYLES, Waveform, waveform levels
 * WHAT:  Barrel for the pill's drawing parts shared by the pill window and the main window: the style table, the
 *        logo, the resting content, the Settings preview and the waveform with its level helpers.
 * WHY:   The pill and the Settings style cards draw the same pill; one folder keeps the parts that must agree
 *        together (03 §3: used by 2+ surfaces → components/global).
 * WHERE: components/global/index.ts; setting-field (enum previews).
 */
export { PillMark, type PillMarkProps } from "./PillMark";
export { PillPreview, type PillPreviewProps } from "./PillPreview";
export { PillRest, type PillRestProps } from "./PillRest";
export { isPillStyle, PILL_STYLES, type PillStyleSpec } from "./pill-styles";
export { Waveform, type WaveformProps, type WaveformTone } from "./Waveform";
export { levelFromRms, pushLevel, silentLevels, WAVEFORM_BARS } from "./waveform-levels";
