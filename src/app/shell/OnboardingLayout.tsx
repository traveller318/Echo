/**
 * SOURCE OF TRUTH KEYWORDS: OnboardingLayout, onboarding layout, frame without sidebar, centred onboarding
 * WHAT:  The layout route of onboarding: ShellFrame (titlebar, scrolling content) without the sidebar, holding the
 *        onboarding page (Outlet).
 * WHY:   04 §5: onboarding is a centred single card; the sidebar would offer pages that cannot work yet (no model), and
 *        the titlebar must stay so the frameless window can be moved, minimized and closed.
 * WHERE: Layout route built by app/routes.tsx at ONBOARDING_ROUTE.
 */
import { Outlet } from "react-router";
import { ShellFrame } from "./ShellFrame";

export function OnboardingLayout() {
  return (
    <ShellFrame>
      <Outlet />
    </ShellFrame>
  );
}
