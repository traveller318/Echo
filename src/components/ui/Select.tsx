/**
 * SOURCE OF TRUTH KEYWORDS: Select, SelectTrigger, SelectContent, SelectItem, SelectValue, SelectGroup, SelectLabel, SelectSeparator, Enum setting control, Radix Select
 * WHAT:  The shadcn Select parts restyled to Echo tokens: a filled --size-control trigger with a chevron, and a
 *        `popover` GlassSurface list (at most --popover-max-height tall) whose items show a check on the
 *        selected one.
 * WHY:   Enum settings (theme, engine, language, accelerator, hotkey mode) render as this control (step 18). Radix
 *        supplies typeahead, keyboard navigation, aria roles and collision-aware positioning; the list is
 *        position="popper" so it opens below the trigger like a macOS pop-up, matched to the trigger's width.
 *        The glass recipe comes from GlassSurface (04 §3.2). Scroll buttons are omitted: the list scrolls
 *        natively and Echo's option lists are short.
 * WHERE: SettingField for Enum settings, Models page accelerator picker. Exported through components/ui/index.ts.
 */
import { CheckIcon, ChevronDownIcon } from "lucide-react";
import { Select as SelectPrimitive } from "radix-ui";
import type { ComponentProps } from "react";
import { GlassSurface } from "@/components/global/glass-surface";
import { cn } from "@/lib/cn";
import { POPOVER_SIDE_OFFSET } from "@/styles/placement";

export const Select = SelectPrimitive.Root;
export const SelectGroup = SelectPrimitive.Group;
export const SelectValue = SelectPrimitive.Value;

export function SelectTrigger({ className, children, ...props }: ComponentProps<typeof SelectPrimitive.Trigger>) {
  return (
    <SelectPrimitive.Trigger
      data-slot="select-trigger"
      className={cn(
        "inline-flex h-control w-full min-w-0 items-center justify-between gap-2 rounded-control bg-fill px-3",
        "text-body text-fg data-[placeholder]:text-fg-tertiary",
        "transition-colors duration-(--duration-fast) ease-standard hover:bg-fill-hover",
        "disabled:pointer-events-none disabled:opacity-(--opacity-disabled)",
        "aria-invalid:border-(length:--border-hairline) aria-invalid:border-record",
        "[&>span]:truncate",
        className,
      )}
      {...props}
    >
      {children}
      <SelectPrimitive.Icon asChild>
        <ChevronDownIcon aria-hidden="true" className="size-icon-sm shrink-0 text-fg-secondary" />
      </SelectPrimitive.Icon>
    </SelectPrimitive.Trigger>
  );
}

export function SelectContent({
  className,
  children,
  position = "popper",
  sideOffset = POPOVER_SIDE_OFFSET,
  ...props
}: ComponentProps<typeof SelectPrimitive.Content>) {
  return (
    <SelectPrimitive.Portal>
      <GlassSurface variant="popover" asChild>
        <SelectPrimitive.Content
          data-slot="select-content"
          position={position}
          sideOffset={sideOffset}
          className={cn(
            "relative z-(--z-popover) max-h-(--radix-select-content-available-height) min-w-(--radix-select-trigger-width)",
            "overflow-x-hidden overflow-y-auto p-1",
            "origin-(--radix-select-content-transform-origin)",
            "animate-fade-in motion-safe:animate-scale-in data-[state=closed]:animate-fade-out",
            className,
          )}
          {...props}
        >
          <SelectPrimitive.Viewport data-slot="select-viewport" className="max-h-popover-max">
            {children}
          </SelectPrimitive.Viewport>
        </SelectPrimitive.Content>
      </GlassSurface>
    </SelectPrimitive.Portal>
  );
}

export function SelectLabel({ className, ...props }: ComponentProps<typeof SelectPrimitive.Label>) {
  return (
    <SelectPrimitive.Label
      data-slot="select-label"
      className={cn("px-2 py-1 text-caption text-fg-secondary", className)}
      {...props}
    />
  );
}

export function SelectItem({ className, children, ...props }: ComponentProps<typeof SelectPrimitive.Item>) {
  return (
    <SelectPrimitive.Item
      data-slot="select-item"
      className={cn(
        "relative flex h-hit w-full cursor-default items-center gap-2 rounded-sm pr-8 pl-2 text-body text-fg outline-none select-none",
        "data-[highlighted]:bg-accent data-[highlighted]:text-accent-fg",
        "data-[disabled]:pointer-events-none data-[disabled]:opacity-(--opacity-disabled)",
        className,
      )}
      {...props}
    >
      <SelectPrimitive.ItemText>{children}</SelectPrimitive.ItemText>
      <span className="absolute right-2 flex size-icon-sm items-center justify-center">
        <SelectPrimitive.ItemIndicator>
          <CheckIcon aria-hidden="true" className="size-icon-sm" />
        </SelectPrimitive.ItemIndicator>
      </span>
    </SelectPrimitive.Item>
  );
}

export function SelectSeparator({ className, ...props }: ComponentProps<typeof SelectPrimitive.Separator>) {
  return (
    <SelectPrimitive.Separator
      data-slot="select-separator"
      className={cn("-mx-1 my-1 h-hairline bg-separator", className)}
      {...props}
    />
  );
}
