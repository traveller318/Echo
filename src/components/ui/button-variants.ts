/**
 * SOURCE OF TRUTH KEYWORDS: buttonVariants, button classes, button variant, button size, press scale
 * WHAT:  Button's class variants: primary / secondary / ghost / destructive and sm / md / icon-sm / icon-md.
 * WHY:   Kept apart from Button.tsx so other parts can style a link or a Radix trigger like a button, and because
 *        a component file may export components alone (fast refresh). Values are tokens only (see Button.tsx).
 * WHERE: Button.tsx; any part that needs button styling on a non-button element.
 */
import { cva } from "class-variance-authority";

export const buttonVariants = cva(
  [
    "inline-flex shrink-0 items-center justify-center gap-2 whitespace-nowrap rounded-control text-callout select-none",
    "transition-[background-color,color,scale] duration-(--duration-fast) ease-standard",
    "motion-safe:active:scale-(--press-scale)",
    "disabled:pointer-events-none disabled:opacity-(--opacity-disabled)",
    "[&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*=size-])]:size-icon-sm",
  ],
  {
    variants: {
      variant: {
        primary: "bg-accent text-accent-fg hover:bg-accent-hover",
        secondary: "bg-fill text-fg hover:bg-fill-hover active:bg-fill-pressed",
        ghost: "bg-transparent text-fg hover:bg-fill-hover active:bg-fill-pressed",
        destructive: "bg-record text-accent-fg hover:bg-record-hover",
      },
      size: {
        sm: "h-hit px-3",
        md: "h-control px-4",
        "icon-sm": "size-hit",
        "icon-md": "size-control",
      },
    },
    defaultVariants: {
      variant: "secondary",
      size: "md",
    },
  },
);

