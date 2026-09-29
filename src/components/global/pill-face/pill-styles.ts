/**
 * SOURCE OF TRUTH KEYWORDS: PILL_STYLES, PillStyleSpec, isPillStyle, pill style look, pill mark image, compact pill, monochrome pill, pill width per style
 * WHAT:  What each PillStyle (the `pill.style` setting) looks like: its logo image and width class, whether it is the
 *        compact pill, the waveform tone, and the width tokens of its resting and recording layouts; `isPillStyle`
 *        narrows a stored option value to a PillStyle.
 * WHY:   The pill and the Settings previews draw the same styles from one table keyed by the generated PillStyle
 *        union, so a style added in Rust fails tsc here until it has a look, and nothing branches on a style name.
 *        Logos are imported `?no-inline` because the CSP's img-src is 'self' only (a data: URI would be blocked).
 *        Class names are literal so Tailwind generates them; widths are token names because the pill springs its
 *        width in JS (readLengthToken).
 * WHERE: pill-face (PillMark, PillRest, PillPreview); src/pill (layout widths, recording layouts);
 *        setting-field/enum-previews.tsx (isPillStyle).
 */
import waveGrey from "@/assets/wave-grey.png?no-inline";
import waveLogo from "@/assets/wave-logo.png?no-inline";
import type { PillStyle } from "@/bindings";
import type { WaveformTone } from "./Waveform";

export interface PillStyleSpec {
  /** The logo image. */
  readonly mark: string;
  /** Width class of the logo (height follows the image). */
  readonly markClass: string;
  /** The compact pill: a round badge at rest, logo · waveform while recording. */
  readonly compact: boolean;
  readonly tone: WaveformTone;
  /** Width token of the resting (idle) layout. */
  readonly restWidth: string;
  /** Width token of the recording layout. */
  readonly recordingWidth: string;
}

export const PILL_STYLES: Readonly<Record<PillStyle, PillStyleSpec>> = {
  full: {
    mark: waveLogo,
    markClass: "w-(--pill-logo-width)",
    compact: false,
    tone: "accent",
    restWidth: "--pill-width-idle",
    recordingWidth: "--pill-width-recording",
  },
  icon: {
    mark: waveLogo,
    markClass: "w-(--pill-mark-width)",
    compact: true,
    tone: "accent",
    restWidth: "--pill-width-mark",
    recordingWidth: "--pill-width-recording-compact",
  },
  mono: {
    mark: waveGrey,
    markClass: "w-(--pill-mark-width)",
    compact: true,
    tone: "neutral",
    restWidth: "--pill-width-mark",
    recordingWidth: "--pill-width-recording-compact",
  },
};

export function isPillStyle(value: string): value is PillStyle {
  return Object.hasOwn(PILL_STYLES, value);
}
