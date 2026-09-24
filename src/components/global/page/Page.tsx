/**
 * SOURCE OF TRUTH KEYWORDS: Page, page layout, page title, content area, content-pad, content-max, actions slot
 * WHAT:  The content area of a main-window screen: a padded, width-capped column with the page title as its h1,
 *        an optional `actions` slot beside the title, and the page body as children.
 * WHY:   04 §5 fixes the content area (--content-pad, max --content-max, title --text-title1) for every screen;
 *        one component keeps Dashboard, History, Models, Settings and Onboarding aligned. The title is the one
 *        h1 of the window, so EmptyState and cards use h2 and below. Slots keep copy and behaviour out of it.
 * WHERE: Every route page in src/routes (index.tsx). Exported through components/global/index.ts.
 */
import type { ComponentProps, ReactNode } from "react";
import { cn } from "@/lib/cn";

export type PageProps = Omit<ComponentProps<"section">, "title"> & {
  readonly title: ReactNode;
  /** Controls beside the title (e.g. "Import from folder"). */
  readonly actions?: ReactNode;
};

export function Page({ title, actions, className, children, ...props }: PageProps) {
  return (
    <section
      data-slot="page"
      className={cn("mx-auto flex w-full max-w-content flex-col gap-6 p-content-pad", className)}
      {...props}
    >
      <header data-slot="page-header" className="flex items-center justify-between gap-4">
        <h1 className="text-title1 text-fg">{title}</h1>
        {actions === undefined ? null : (
          <div data-slot="page-actions" className="flex items-center gap-2">
            {actions}
          </div>
        )}
      </header>
      {children}
    </section>
  );
}
