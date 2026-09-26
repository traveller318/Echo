/**
 * SOURCE OF TRUTH KEYWORDS: ShellFrame, window layout, main window chrome, sidebar slot, content card, sidebar collapse, scroll container
 * WHAT:  The main window's frame: the full-width Titlebar on top, then a row of two cards — an optional `sidebar`
 *        slot on the left (collapsible from the titlebar toggle) and a scrolling <main> card holding `children`.
 *        Fills the window and never scrolls itself.
 * WHY:   The titlebar must exist in every state, including before the registry has loaded and when it failed,
 *        otherwise a frameless window could not be moved or closed. Keeping the frame free of routing and
 *        registry reads lets the app gate (app/router.tsx) use it for those states and ShellLayout for the
 *        routed app, so the layout is written once. Only <main> scrolls, so the titlebar and sidebar stay put.
 *        Collapse animates the slot's width to 0 and makes it `inert`, so hidden links leave the tab order and the
 *        accessibility tree while the sidebar stays mounted (no refetch, no lost scroll). The open flag is UI
 *        state in stores/shell-store.ts; the toggle only appears when there is a sidebar to toggle.
 * WHERE: app/router.tsx (loading and error states), app/shell/ShellLayout.tsx (routed pages),
 *        app/shell/OnboardingLayout.tsx (no sidebar).
 */
import { useId, type ReactNode } from "react";
import { GlassSurface } from "@/components/global";
import { cn } from "@/lib/cn";
import { toggleSidebar, useShellStore } from "@/stores/shell-store";
import { Titlebar } from "./Titlebar";

export interface ShellFrameProps {
  readonly sidebar?: ReactNode;
  readonly children?: ReactNode;
}

export function ShellFrame({ sidebar, children }: ShellFrameProps) {
  const sidebarId = useId();
  const sidebarOpen = useShellStore((state) => state.sidebarOpen);
  const hasSidebar = sidebar !== undefined;
  return (
    <div data-slot="shell" className="flex h-dvh flex-col overflow-hidden text-fg">
      <Titlebar
        sidebar={hasSidebar ? { open: sidebarOpen, controls: sidebarId, onToggle: toggleSidebar } : undefined}
      />
      <div className="flex min-h-0 flex-1 px-3 pb-3">
        {hasSidebar ? (
          <div
            id={sidebarId}
            data-slot="sidebar-slot"
            data-state={sidebarOpen ? "open" : "closed"}
            inert={!sidebarOpen}
            className={cn(
              "flex shrink-0 overflow-hidden",
              "transition-[width,margin] duration-(--duration-base) ease-standard motion-reduce:transition-none",
              sidebarOpen ? "me-3 w-sidebar" : "me-0 w-0",
            )}
          >
            {sidebar}
          </div>
        ) : null}
        <GlassSurface variant="card" asChild>
          <main className="min-h-0 min-w-0 flex-1 overflow-y-auto rounded-window">{children}</main>
        </GlassSurface>
      </div>
    </div>
  );
}
