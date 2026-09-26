/**
 * SOURCE OF TRUTH KEYWORDS: InlineNotice, inline notice, page notice, banner, offline notice, callout, icon slot, action slot
 * WHAT:  A quiet in-page notice: a filled --radius-card panel with an `icon`, a `title`, an optional `body` and an
 *        optional `action`, announced politely to screen readers.
 * WHY:   Some states belong on the page rather than in a toast because they last (offline mode blocking downloads,
 *        later a missing microphone permission in onboarding): a toast would vanish while the state stays. The
 *        content comes in as slots, so the notice carries no copy or behaviour of its own; the glyph carries the
 *        tone (never colour alone, 04 §7), the text keeps --color-fg.
 * WHERE: routes/models (offline mode); routes/settings (a selected model that is missing or downloading); onboarding
 *        steps (step 24). Exported through components/global.
 */
import type { ComponentProps, ReactNode } from "react";
import { cn } from "@/lib/cn";

export type InlineNoticeProps = Omit<ComponentProps<"div">, "title"> & {
  readonly icon?: ReactNode;
  readonly title: ReactNode;
  readonly body?: ReactNode;
  readonly action?: ReactNode;
};

export function InlineNotice({ icon, title, body, action, className, ...props }: InlineNoticeProps) {
  return (
    <div
      role="status"
      data-slot="inline-notice"
      className={cn("flex items-center gap-3 rounded-card bg-fill px-4 py-3", className)}
      {...props}
    >
      {icon === undefined ? null : (
        <div aria-hidden="true" className="shrink-0 text-warning [&_svg]:size-icon-md">
          {icon}
        </div>
      )}
      <div className="flex min-w-0 flex-1 flex-col gap-1">
        <p className="text-callout text-fg">{title}</p>
        {body === undefined ? null : <p className="text-footnote text-fg-secondary">{body}</p>}
      </div>
      {action === undefined ? null : <div className="flex shrink-0 items-center gap-2">{action}</div>}
    </div>
  );
}
