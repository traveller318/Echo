/**
 * SOURCE OF TRUTH KEYWORDS: EmptyState, empty state, placeholder content, icon slot, title slot, body slot, action slot, no results
 * WHAT:  A centered empty state built from four slots: `icon` (inside a soft circle), `title`, optional `body`
 *        and optional `action`. Slots that are not given render nothing.
 * WHY:   Every list and page shows "nothing here yet" the same way (04 §6); the content comes in as slots so the
 *        component carries no copy or behaviour of its own. The title renders as a heading (`titleAs`, default
 *        h2 under the page's h1) so screen readers can jump to it. Icons follow 04 §3.9 (--size-icon-md).
 * WHERE: Route index pages while they are being set up, app gate and page errors (app/router.tsx,
 *        app/shell/RouteError.tsx), History with no takes or no search results,
 *        Models with nothing installed. Exported through components/global/index.ts.
 */
import type { ComponentProps, ReactNode } from "react";
import { cn } from "@/lib/cn";

export type EmptyStateProps = Omit<ComponentProps<"div">, "title"> & {
  readonly icon?: ReactNode;
  readonly title: ReactNode;
  readonly body?: ReactNode;
  readonly action?: ReactNode;
  /** Heading element for the title; pick the level that fits under the surrounding headings. */
  readonly titleAs?: "h1" | "h2" | "h3" | "h4" | "p";
};

export function EmptyState({
  icon,
  title,
  body,
  action,
  titleAs: Title = "h2",
  className,
  ...props
}: EmptyStateProps) {
  return (
    <div
      data-slot="empty-state"
      className={cn("flex flex-col items-center justify-center gap-3 p-8 text-center", className)}
      {...props}
    >
      {icon === undefined ? null : (
        <div
          data-slot="empty-state-icon"
          aria-hidden="true"
          className="flex size-10 items-center justify-center rounded-pill bg-fill text-fg-secondary [&_svg]:size-icon-md"
        >
          {icon}
        </div>
      )}
      <Title data-slot="empty-state-title" className="text-title3 text-fg">
        {title}
      </Title>
      {body === undefined ? null : (
        <p data-slot="empty-state-body" className="max-w-measure text-body text-fg-secondary">
          {body}
        </p>
      )}
      {action === undefined ? null : (
        <div data-slot="empty-state-action" className="mt-2 flex items-center gap-2">
          {action}
        </div>
      )}
    </div>
  );
}
