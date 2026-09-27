/**
 * SOURCE OF TRUTH KEYWORDS: PillMark, pill logo, wave logo, grey wave logo, pill style logo
 * WHAT:  Echo's wave logo as the given pill style draws it (colour or grey, full or compact width), decorative.
 * WHY:   Every pill layout that shows the logo and the Settings previews take it from PILL_STYLES, so the logo of a
 *        style is chosen in one place. It is hidden from assistive tech: the pill surface names its state.
 * WHERE: pill-face/PillRest.tsx; src/pill/Pill.tsx (recording layouts).
 */
import type { PillStyle } from "@/bindings";
import { cn } from "@/lib/cn";
import { PILL_STYLES } from "./pill-styles";

export interface PillMarkProps {
  readonly pillStyle: PillStyle;
  readonly className?: string;
}

export function PillMark({ pillStyle, className }: PillMarkProps) {
  const spec = PILL_STYLES[pillStyle];
  return (
    <img
      src={spec.mark}
      alt=""
      aria-hidden
      draggable={false}
      data-slot="pill-mark"
      className={cn("shrink-0", spec.markClass, className)}
    />
  );
}
