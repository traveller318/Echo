/**
 * SOURCE OF TRUTH KEYWORDS: Kbd, keyboard key, hotkey chip, shortcut display, KbdGroup
 * WHAT:  Kbd renders one key as a --size-kbd chip; KbdGroup lays several keys out as one shortcut.
 * WHY:   Hotkeys are shown as key chips everywhere (HotkeyInput in step 18, onboarding, tray tips), so one
 *        component owns the look. Uses the semantic <kbd> element so assistive tech announces keys.
 * WHERE: HotkeyInput (step 18), onboarding hotkey step, empty-state hints. Exported through
 *        components/ui/index.ts.
 */
import type { ComponentProps } from "react";
import { cn } from "@/lib/cn";

export function Kbd({ className, ...props }: ComponentProps<"kbd">) {
  return (
    <kbd
      data-slot="kbd"
      className={cn(
        "inline-flex h-kbd min-w-kbd items-center justify-center rounded-xs px-1",
        "border-(length:--border-hairline) border-separator bg-fill font-sans text-caption text-fg-secondary tabular-nums",
        className,
      )}
      {...props}
    />
  );
}

export function KbdGroup({ className, ...props }: ComponentProps<"span">) {
  return <span data-slot="kbd-group" className={cn("inline-flex items-center gap-1", className)} {...props} />;
}
