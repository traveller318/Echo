/**
 * SOURCE OF TRUTH KEYWORDS: Badge, status badge, badgeVariants, neutral, accent, success, warning, record, status glyph
 * WHAT:  The shadcn Badge restyled to Echo tokens: a small --radius-xs chip in --text-caption with variants
 *        neutral / accent / success / warning / record.
 * WHY:   Status is never conveyed by colour alone (04 §7), so the status colour tints only the leading glyph
 *        (an icon child) and the text stays --color-fg / --color-fg-secondary, which keeps ≥ 4.5:1 contrast on
 *        glass; the green, orange and red tokens would fail that as small text. Accent uses the soft accent fill
 *        for selected or "in use" markers.
 * WHERE: History row status, Models card status, Settings "restart required". Exported through
 *        components/ui/index.ts.
 */
import type { VariantProps } from "class-variance-authority";
import type { ComponentProps } from "react";
import { cn } from "@/lib/cn";
import { badgeVariants } from "./badge-variants";

export type BadgeProps = ComponentProps<"span"> & VariantProps<typeof badgeVariants>;

export function Badge({ className, variant, ...props }: BadgeProps) {
  return <span data-slot="badge" className={cn(badgeVariants({ variant }), className)} {...props} />;
}
