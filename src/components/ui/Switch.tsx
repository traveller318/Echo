/**
 * SOURCE OF TRUTH KEYWORDS: Switch, toggle, boolean setting control, Radix Switch, checked state
 * WHAT:  The shadcn Switch restyled to Echo tokens: a --switch-width × --switch-height pill track in
 *        --color-fill-pressed that turns --color-accent when checked, with a --switch-thumb knob.
 * WHY:   Bool settings render as this control (04 §5 Settings, generated from the registry in step 18). The thumb
 *        inset and travel are computed from the size tokens, so changing a token keeps the knob centred. Radix
 *        supplies role="switch", keyboard toggling and aria-checked.
 * WHERE: SettingField for Bool settings, onboarding. Exported through components/ui/index.ts.
 */
import { Switch as SwitchPrimitive } from "radix-ui";
import type { ComponentProps } from "react";
import { cn } from "@/lib/cn";

export function Switch({ className, ...props }: ComponentProps<typeof SwitchPrimitive.Root>) {
  return (
    <SwitchPrimitive.Root
      data-slot="switch"
      className={cn(
        "inline-flex h-switch-height w-switch-width shrink-0 items-center rounded-pill",
        "px-[calc((var(--switch-height)-var(--switch-thumb))/2)]",
        "bg-fill-pressed transition-colors duration-(--duration-fast) ease-standard data-[state=checked]:bg-accent",
        "disabled:pointer-events-none disabled:opacity-(--opacity-disabled)",
        className,
      )}
      {...props}
    >
      <SwitchPrimitive.Thumb
        data-slot="switch-thumb"
        className={cn(
          "pointer-events-none block size-switch-thumb rounded-pill bg-accent-fg shadow-e1",
          "transition-transform duration-(--duration-fast) ease-standard",
          "data-[state=checked]:translate-x-[calc(var(--switch-width)-var(--switch-height))]",
        )}
      />
    </SwitchPrimitive.Root>
  );
}
