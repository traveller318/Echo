/**
 * SOURCE OF TRUTH KEYWORDS: ShellLayout, routed shell, layout route, Outlet, sidebar
 * WHAT:  The layout route of the main window's sidebar pages: ShellFrame with the registry Sidebar and the current
 *        page in the content area (Outlet).
 * WHY:   Every sidebar page shares one chrome, so pages render only their content (04 §5). Toasts and page requests
 *        from the pill are the root route's (AppRoot), so they also work on onboarding, which has no sidebar.
 * WHERE: Layout route built by app/routes.tsx under AppRoot.
 */
import { Outlet } from "react-router";
import { ShellFrame } from "./ShellFrame";
import { Sidebar } from "./Sidebar";

export function ShellLayout() {
  return (
    <ShellFrame sidebar={<Sidebar />}>
      <Outlet />
    </ShellFrame>
  );
}
