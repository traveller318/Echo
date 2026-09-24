/**
 * SOURCE OF TRUTH KEYWORDS: Input, text field, search field, form input, aria-invalid, token input
 * WHAT:  The shadcn Input restyled to Echo tokens: a --size-control high filled field with --radius-control,
 *        placeholder in --color-fg-tertiary and a --color-record border when `aria-invalid` is set.
 * WHY:   Fields sit on glass cards, so they use the soft --color-fill instead of a hard border (04 §1 "glass, not
 *        chrome"). Invalid state keys off `aria-invalid`, which React Hook Form sets, so validation styling and
 *        the accessible state cannot disagree. Focus uses the global focus ring.
 * WHERE: History search (step 16), Settings text fields (step 18), dictionary editor. Exported through
 *        components/ui/index.ts.
 */
import type { ComponentProps } from "react";
import { cn } from "@/lib/cn";

export function Input({ className, type = "text", ...props }: ComponentProps<"input">) {
  return (
    <input
      data-slot="input"
      type={type}
      className={cn(
        "h-control w-full min-w-0 rounded-control border-(length:--border-hairline) border-transparent bg-fill px-3",
        "text-body text-fg placeholder:text-fg-tertiary",
        "transition-[background-color,border-color] duration-(--duration-fast) ease-standard hover:bg-fill-hover",
        "disabled:pointer-events-none disabled:opacity-(--opacity-disabled)",
        "aria-invalid:border-record",
        className,
      )}
      {...props}
    />
  );
}
