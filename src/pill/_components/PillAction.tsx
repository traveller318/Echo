/**
 * SOURCE OF TRUTH KEYWORDS: PillAction, pill button, pill stop button, pill text button, clickable pill area
 * WHAT:  A pill button: the Button primitive (ghost, pill-rounded) that also registers itself as one of the pill's
 *        clickable areas while it is mounted.
 * WHY:   The pill window lets clicks through everywhere except on its buttons (04 §4), so every button must report its
 *        rectangle (usePillHitArea); wrapping it once means no pill button can forget. Its size is --size-hit, the
 *        minimum hit target (04 §7); an icon-only button needs `label` for its accessible name.
 * WHERE: src/pill/Pill.tsx (cancel, stop, "Undo", "Open", "Set up").
 */
import type { ReactNode } from "react";
import { Button, type ButtonProps } from "@/components/ui";
import { cn } from "@/lib/cn";
import { usePillHitArea } from "../hit-areas";

export interface PillActionProps {
  readonly onPress: () => void;
  readonly children: ReactNode;
  /** Accessible name of an icon-only button. */
  readonly label?: string;
  readonly size?: Extract<ButtonProps["size"], "sm" | "icon-sm">;
  readonly className?: string;
}

export function PillAction({ onPress, children, label, size = "sm", className }: PillActionProps) {
  const hitArea = usePillHitArea();
  return (
    <Button
      ref={hitArea}
      variant="ghost"
      size={size}
      aria-label={label}
      onClick={onPress}
      className={cn("rounded-pill", className)}
    >
      {children}
    </Button>
  );
}
