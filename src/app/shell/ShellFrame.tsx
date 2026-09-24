/**
 * SOURCE OF TRUTH KEYWORDS: ShellFrame, window layout, main window chrome, sidebar slot, content area, scroll container
 * WHAT:  The main window's frame: an optional `sidebar` slot on the left, then the Titlebar above a scrolling
 *        <main> that holds `children`. Fills the window and never scrolls itself.
 * WHY:   The titlebar must exist in every state, including before the registry has loaded and when it failed,
 *        otherwise a frameless window could not be moved or closed. Keeping the frame free of routing and
 *        registry reads lets the app gate (app/router.tsx) use it for those states and ShellLayout for the
 *        routed app, so the layout is written once. Only <main> scrolls, so the titlebar and sidebar stay put.
 * WHERE: app/router.tsx (loading and error states), app/shell/ShellLayout.tsx (routed pages).
 */
import type { ReactNode } from "react";
import { Titlebar } from "./Titlebar";

export interface ShellFrameProps {
  readonly sidebar?: ReactNode;
  readonly children?: ReactNode;
}

export function ShellFrame({ sidebar, children }: ShellFrameProps) {
  return (
    <div data-slot="shell" className="flex h-dvh overflow-hidden text-fg">
      {sidebar}
      <div className="flex min-w-0 flex-1 flex-col">
        <Titlebar />
        <main className="min-h-0 flex-1 overflow-y-auto">{children}</main>
      </div>
    </div>
  );
}
