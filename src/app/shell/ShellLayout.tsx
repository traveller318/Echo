/**
 * SOURCE OF TRUTH KEYWORDS: ShellLayout, routed shell, layout route, Outlet, sidebar, toaster
 * WHAT:  The layout route of the main window: ShellFrame with the registry Sidebar, the current page in the
 *        content area (Outlet), and the Toaster; it also follows NavigationRequested (the pill's "Set up" / "Open").
 * WHY:   Every page shares one chrome, so pages render only their content (04 §5). The Toaster sits here, inside
 *        the router, because its actions navigate; its viewport is fixed-positioned, so it lives outside <main>.
 * WHERE: Root route element built by app/routes.tsx.
 */
import { Outlet } from "react-router";
import { ShellFrame } from "./ShellFrame";
import { Sidebar } from "./Sidebar";
import { Toaster } from "./Toaster";
import { useNavigationRequests } from "./use-navigation-requests";

export function ShellLayout() {
  useNavigationRequests();
  return (
    <>
      <ShellFrame sidebar={<Sidebar />}>
        <Outlet />
      </ShellFrame>
      <Toaster />
    </>
  );
}
