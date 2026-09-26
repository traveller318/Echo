/**
 * SOURCE OF TRUTH KEYWORDS: AppRoot, root route, root layout, toaster, navigation requests, onboarding gate
 * WHAT:  The root route of the main window: renders the current layout (the sidebar shell or onboarding) and the
 *        Toaster, follows NavigationRequested (the pill's "Open") and opens onboarding when it is due or requested.
 * WHY:   Toasts, page requests from the pill and the onboarding gate must work on every screen, including onboarding,
 *        which has no sidebar; one root keeps them mounted once per window. The Toaster sits inside the router
 *        because its actions navigate; its viewport is fixed-positioned, so it lives outside any <main>.
 * WHERE: Root route element built by app/routes.tsx.
 */
import { Outlet } from "react-router";
import { Toaster } from "./Toaster";
import { useNavigationRequests } from "./use-navigation-requests";
import { useOnboardingGate } from "./use-onboarding-gate";

export function AppRoot() {
  useNavigationRequests();
  useOnboardingGate();
  return (
    <>
      <Outlet />
      <Toaster />
    </>
  );
}
