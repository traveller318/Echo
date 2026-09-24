/**
 * SOURCE OF TRUTH KEYWORDS: glassSurfaceVariants, GlassSurfaceVariant, GLASS_SURFACE_VARIANTS, glass recipe classes, glass variants
 * WHAT:  The glass recipe as class variants (card, sidebar, popover, pill, modal) and the list of variant names.
 * WHY:   docs/04 §3.2 makes GlassSurface the only place the recipe is assembled; the classes live in this sibling
 *        file only because a component file may export components alone (fast refresh). Every value is a token;
 *        under data-transparency="reduced" the tints are already solid (tokens.css) and the backdrop filter is
 *        dropped, since blurring behind an opaque surface only costs GPU time.
 * WHERE: GlassSurface.tsx (the only consumer of the classes); GLASS_SURFACE_VARIANTS by its tests.
 */
import { cva, type VariantProps } from "class-variance-authority";

export const glassSurfaceVariants = cva(
  [
    "bg-(--glass-tint) backdrop-saturate-(--glass-saturate)",
    "[:root[data-transparency=reduced]_&]:backdrop-filter-none",
  ],
  {
    variants: {
      variant: {
        card: [
          "rounded-card border-(length:--border-hairline) border-(--glass-border) backdrop-blur-md",
          "shadow-[inset_0_var(--border-hairline)_0_var(--glass-highlight),var(--shadow-e1)]",
        ],
        sidebar: "backdrop-blur-md",
        popover: [
          "rounded-control border-(length:--border-hairline) border-(--glass-border) bg-(--glass-tint-strong) backdrop-blur-sm",
          "shadow-[inset_0_var(--border-hairline)_0_var(--glass-highlight),var(--shadow-e2)]",
        ],
        pill: [
          "rounded-pill border-(length:--border-hairline) border-(--glass-border) bg-(--glass-tint-strong) backdrop-blur-md",
          "shadow-[inset_0_var(--border-hairline)_0_var(--glass-highlight),var(--shadow-e3)]",
        ],
        modal: [
          "rounded-card border-(length:--border-hairline) border-(--glass-border) bg-(--glass-tint-strong) backdrop-blur-lg",
          "shadow-[inset_0_var(--border-hairline)_0_var(--glass-highlight),var(--shadow-e2)]",
        ],
      },
    },
    defaultVariants: {
      variant: "card",
    },
  },
);

export type GlassSurfaceVariant = NonNullable<VariantProps<typeof glassSurfaceVariants>["variant"]>;

export const GLASS_SURFACE_VARIANTS = [
  "card",
  "sidebar",
  "popover",
  "pill",
  "modal",
] as const satisfies readonly GlassSurfaceVariant[];

