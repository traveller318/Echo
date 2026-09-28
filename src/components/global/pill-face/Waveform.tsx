/**
 * SOURCE OF TRUTH KEYWORDS: Waveform, WaveformTone, pill waveform, AudioLevel bars, static level bar, reduced motion waveform, neutral waveform
 * WHAT:  The recording pill's live input level: WAVEFORM_BARS bars (the last AudioLevel readings, newest on the right), or
 *        under reduced motion a single static bar whose fill is the current level; `tone` draws it in the accent
 *        colour or in neutral grey (the monochrome pill style).
 * WHY:   04 §4 and §3.7: bars `--waveform-bar-width` wide with `--waveform-bar-gap` gaps, `--radius-pill`, height
 *        between `--waveform-min` and `--waveform-max`, `--color-accent` at `--opacity-waveform`. Heights are computed
 *        in CSS from those tokens and a per-bar level variable, so no size lives in code. It listens to AudioLevel
 *        only while `listening` (the microphone is open), and keeps its last levels while the take finishes, so the
 *        bars freeze instead of dropping. Not listening from the start, it rests as a flat line of bars (the idle
 *        pill and the Settings previews). The levels are display state, never a copy of domain state. The tone
 *        classes are literal so Tailwind generates them.
 * WHERE: src/pill/Pill.tsx (recording layouts); pill-face/PillRest.tsx (idle look, Settings previews).
 */
import { useState, type CSSProperties } from "react";
import { useEchoEvent } from "@/hooks/use-echo-event";
import { levelFromRms, pushLevel, silentLevels } from "./waveform-levels";

export type WaveformTone = "accent" | "neutral";

/** Fill of the bars per tone (--color-accent, or --color-fg-secondary for the monochrome style). */
const TONE_FILL: Readonly<Record<WaveformTone, string>> = {
  accent: "bg-accent",
  neutral: "bg-fg-secondary",
};

export interface WaveformProps {
  /** The microphone is open: follow AudioLevel. */
  readonly listening: boolean;
  /** Show the static level bar instead of moving bars. */
  readonly reducedMotion: boolean;
  readonly tone?: WaveformTone;
}

/** A bar's level (0–1) for the CSS height and width formulas below. */
type LevelStyle = CSSProperties & Record<"--echo-level", number>;

function levelStyle(level: number): LevelStyle {
  return { "--echo-level": level };
}

/**
 * The bars' own width (WAVEFORM_BARS bars and one gap fewer), so the static bar takes exactly the waveform's place.
 * Literal because Tailwind only generates classes it finds verbatim in source; keep it in step with WAVEFORM_BARS.
 */
const WAVEFORM_WIDTH = "w-[calc(8*var(--waveform-bar-width)+7*var(--waveform-bar-gap))]";

const BAR_HEIGHT =
  "h-[calc(var(--waveform-min)+var(--echo-level)*(var(--waveform-max)-var(--waveform-min)))]";

export function Waveform({ listening, reducedMotion, tone = "accent" }: WaveformProps) {
  const [levels, setLevels] = useState(silentLevels);
  useEchoEvent(
    "audioLevel",
    ({ rms }) => {
      // A non-finite reading arrives as null: treat it as silence.
      setLevels((previous) => pushLevel(previous, levelFromRms(rms ?? 0)));
    },
    { enabled: listening },
  );

  if (reducedMotion) {
    const current = levels.at(-1) ?? 0;
    return (
      <div
        data-slot="waveform"
        data-mode="static"
        aria-hidden
        className={`h-(--waveform-bar-width) overflow-hidden rounded-pill bg-fill ${WAVEFORM_WIDTH}`}
      >
        <div
          className={`h-full w-[calc(var(--echo-level)*100%)] rounded-pill opacity-(--opacity-waveform) ${TONE_FILL[tone]}`}
          style={levelStyle(current)}
        />
      </div>
    );
  }
  return (
    <div data-slot="waveform" data-mode="bars" aria-hidden className="flex items-center gap-(--waveform-bar-gap)">
      {levels.map((level, index) => (
        <span
          // Bars are positions in a fixed-length window, so the index is their identity.
          key={index}
          className={`w-(--waveform-bar-width) shrink-0 rounded-pill opacity-(--opacity-waveform) ${TONE_FILL[tone]} ${BAR_HEIGHT}`}
          style={levelStyle(level)}
        />
      ))}
    </div>
  );
}
