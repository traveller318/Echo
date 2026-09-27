/**
 * SOURCE OF TRUTH KEYWORDS: PillPreview, pill style preview, Settings pill style card, static pill
 * WHAT:  A still copy of the pill at rest in a given style: the glass pill surface at the style's resting width
 *        holding PillRest.
 * WHY:   The `pill.style` choices are pictures (EnumDisplay::Cards), and the picture is the real resting pill, built
 *        from the same surface, width token and content as the pill window. It is decorative: the card's label
 *        names the choice.
 * WHERE: setting-field/enum-previews.tsx (the `pill_style` preview).
 */
import { useReducedMotion } from "motion/react";
import type { PillStyle } from "@/bindings";
import { GlassSurface } from "../glass-surface";
import { PillRest } from "./PillRest";
import { PILL_STYLES } from "./pill-styles";

export interface PillPreviewProps {
  readonly pillStyle: PillStyle;
}

export function PillPreview({ pillStyle }: PillPreviewProps) {
  const reducedMotion = useReducedMotion() ?? false;
  return (
    <GlassSurface
      variant="pill"
      aria-hidden
      data-slot="pill-preview"
      className="flex h-(--pill-height) items-center justify-center"
      style={{ width: `var(${PILL_STYLES[pillStyle].restWidth})` }}
    >
      <PillRest pillStyle={pillStyle} reducedMotion={reducedMotion} />
    </GlassSurface>
  );
}
