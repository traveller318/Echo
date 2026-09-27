/**
 * SOURCE OF TRUTH KEYWORDS: PillRest, idle pill, resting pill, pill between takes, always visible pill look
 * WHAT:  The content of the pill at rest (no take running) in a given style: the full style shows the logo, a
 *        hairline divider and a flat waveform; the compact styles show the logo alone, centred in a round badge.
 * WHY:   With `pill.visibility` = always the pill stays on screen between takes and must say "ready" without words;
 *        the flat waveform previews the recording layout it grows into. The Settings style cards render this same
 *        content, so a preview can never drift from the real pill.
 * WHERE: src/pill/Pill.tsx (idle layout); pill-face/PillPreview.tsx (Settings style cards).
 */
import type { PillStyle } from "@/bindings";
import { PillMark } from "./PillMark";
import { PILL_STYLES } from "./pill-styles";
import { Waveform } from "./Waveform";

export interface PillRestProps {
  readonly pillStyle: PillStyle;
  readonly reducedMotion: boolean;
}

export function PillRest({ pillStyle, reducedMotion }: PillRestProps) {
  const spec = PILL_STYLES[pillStyle];
  if (spec.compact) {
    return <PillMark pillStyle={pillStyle} />;
  }
  return (
    <div className="flex items-center gap-2">
      <PillMark pillStyle={pillStyle} />
      <span aria-hidden className="h-(--pill-divider-height) w-hairline shrink-0 bg-fg-tertiary" />
      <Waveform listening={false} reducedMotion={reducedMotion} tone={spec.tone} />
    </div>
  );
}
