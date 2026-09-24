/**
 * SOURCE OF TRUTH KEYWORDS: badgeVariants, badge classes, status badge variant
 * WHAT:  Badge's class variants: neutral / accent / success / warning / record.
 * WHY:   Kept apart from Badge.tsx because a component file may export components alone (fast refresh). The
 *        status colour tints only the glyph so text keeps its contrast (see Badge.tsx).
 * WHERE: Badge.tsx; any part that needs badge styling on another element.
 */
import { cva } from "class-variance-authority";

export const badgeVariants = cva(
  [
    "inline-flex h-kbd shrink-0 items-center gap-1 rounded-xs px-2 text-caption whitespace-nowrap",
    "[&_svg]:pointer-events-none [&_svg]:size-3 [&_svg]:shrink-0",
  ],
  {
    variants: {
      variant: {
        neutral: "bg-fill text-fg-secondary",
        accent: "bg-accent-soft text-fg [&_svg]:text-accent",
        success: "bg-fill text-fg [&_svg]:text-success",
        warning: "bg-fill text-fg [&_svg]:text-warning",
        record: "bg-fill text-fg [&_svg]:text-record",
      },
    },
    defaultVariants: {
      variant: "neutral",
    },
  },
);

