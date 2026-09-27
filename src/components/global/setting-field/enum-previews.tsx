/**
 * SOURCE OF TRUTH KEYWORDS: EnumPreviewFor, ENUM_PREVIEWS, enum option preview, preview card picture, pill style preview
 * WHAT:  `EnumPreviewFor({ preview, value })`: the picture a card of an `EnumDisplay::Cards` setting shows for one
 *        option value, or nothing when the value is not one the preview can draw.
 * WHY:   The registry names what a card draws (EnumPreview), never a setting key, and the table is keyed by the
 *        generated union, so a new preview fails tsc here until it can be drawn (root CLAUDE.md §7: settings
 *        controls come from the registry). Values are narrowed with a guard, never cast.
 * WHERE: EnumControl (cards display).
 */
import type { ReactNode } from "react";
import type { EnumPreview } from "@/bindings";
import { isPillStyle, PillPreview } from "../pill-face";

const ENUM_PREVIEWS: Readonly<Record<EnumPreview, (value: string) => ReactNode>> = {
  pill_style: (value) => (isPillStyle(value) ? <PillPreview pillStyle={value} /> : null),
};

export interface EnumPreviewForProps {
  readonly preview: EnumPreview;
  readonly value: string;
}

export function EnumPreviewFor({ preview, value }: EnumPreviewForProps) {
  return ENUM_PREVIEWS[preview](value);
}
