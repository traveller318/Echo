/**
 * SOURCE OF TRUTH KEYWORDS: cn, class merge, tailwind-merge, clsx, THEME_SCALE, design token class names, className composition
 * WHAT:  `cn(...inputs)` joins class names (clsx) and resolves Tailwind conflicts (tailwind-merge) so the last
 *        class of a group wins; THEME_SCALE lists the token names globals.css exposes to Tailwind.
 * WHY:   Components take a `className` override; without merging, `cn("text-body", "text-fg")` would keep both
 *        but a naive merge would drop one. tailwind-merge only knows Tailwind's default scales, and globals.css
 *        replaces them all with docs/04 tokens (`text-callout` is a size, `text-fg` a colour), so it is told our
 *        names here. tokens.test.ts proves this list equals the @theme mapping, so the two cannot drift.
 * WHERE: Every component in src/components (ui primitives, global components) and route components.
 */
import { clsx, type ClassValue } from "clsx";
import { extendTailwindMerge } from "tailwind-merge";

/** Token names per Tailwind theme namespace, exactly as mapped in globals.css. */
export const THEME_SCALE = {
  color: [
    "bg",
    "fg",
    "fg-secondary",
    "fg-tertiary",
    "separator",
    "fill",
    "fill-hover",
    "fill-pressed",
    "accent",
    "accent-fg",
    "accent-soft",
    "record",
    "success",
    "warning",
    "chart-1",
    "chart-2",
    "scrim",
    "accent-hover",
    "record-hover",
    "transparent",
    "current",
  ],
  text: ["caption", "footnote", "body", "callout", "title3", "title2", "title1", "display"],
  radius: ["xs", "sm", "control", "card", "window", "pill"],
  shadow: ["e0", "e1", "e2", "e3"],
  blur: ["sm", "md", "lg"],
  spacing: [
    "0",
    "1",
    "2",
    "3",
    "4",
    "5",
    "6",
    "8",
    "10",
    "12",
    "sidebar",
    "titlebar",
    "content-pad",
    "hairline",
    "hit",
    "control",
    "icon-sm",
    "icon-md",
    "kbd",
    "switch-width",
    "switch-height",
    "switch-thumb",
    "slider-thumb",
    "track",
    "popover-max",
  ],
  container: ["content", "dialog", "measure", "sheet", "toast", "tooltip"],
  "font-weight": ["regular", "medium", "semibold", "bold"],
  ease: ["standard", "exit"],
  animate: [
    "fade-in",
    "fade-out",
    "scale-in",
    "scale-out",
    "slide-in-right",
    "slide-out-right",
    "slide-in-up",
    "indeterminate",
    "breathe",
    "shimmer",
  ],
} as const;

// Pure: a window that never calls cn (the pill) does not pay for building the merger.
const merge = /* @__PURE__ */ extendTailwindMerge({
  override: {
    theme: {
      color: [...THEME_SCALE.color],
      text: [...THEME_SCALE.text],
      radius: [...THEME_SCALE.radius],
      shadow: [...THEME_SCALE.shadow],
      blur: [...THEME_SCALE.blur],
      spacing: [...THEME_SCALE.spacing],
      container: [...THEME_SCALE.container],
      "font-weight": [...THEME_SCALE["font-weight"]],
      ease: [...THEME_SCALE.ease],
      animate: [...THEME_SCALE.animate],
    },
  },
});

export function cn(...inputs: ClassValue[]): string {
  return merge(clsx(inputs));
}
