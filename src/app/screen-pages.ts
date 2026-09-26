/**
 * SOURCE OF TRUTH KEYWORDS: ONBOARDING_ROUTE, ONBOARDING_PAGE, screen pages, routes outside the sidebar, lazy onboarding page
 * WHAT:  The main-window screens that are not sidebar pages: their route and their lazily loaded page (onboarding).
 * WHY:   Sidebar pages come from registry nav entries (app/nav-page.ts); onboarding is reached on purpose (first run,
 *        a missing model, the pill's "Set up"), never from the sidebar (registry/nav.rs), so its route is named once
 *        here and every navigation to it uses this constant. The page loads lazily like every other page, so the
 *        first paint carries only the shell. It lives apart from the route builder because a file that defines lazy
 *        components may export nothing else (fast refresh).
 * WHERE: app/routes.tsx (the onboarding route), app/shell/use-onboarding-gate.ts (navigates to it), the onboarding
 *        page itself (src/routes/onboarding).
 */
import { lazy } from "react";

/** Where onboarding lives in the main window's router. */
export const ONBOARDING_ROUTE = "/onboarding";

export const ONBOARDING_PAGE = lazy(() => import("@/routes/onboarding"));
