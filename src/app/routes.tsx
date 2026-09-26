/**
 * SOURCE OF TRUTH KEYWORDS: buildAppRoutes, route table, registry routes, page error boundary, fallback redirect, RouteObject, onboarding route
 * WHAT:  `buildAppRoutes(nav, pages, onboarding)` turns the registry nav into the router's route objects: the root
 *        AppRoot (toasts, page requests, the onboarding gate) holding the ShellLayout around one route per nav entry
 *        (its registry `route` → its page from NAV_PAGES, given its NavItem), a page-level error boundary and a
 *        catch-all that redirects to the first page in sidebar order; and, beside it, onboarding at ONBOARDING_ROUTE
 *        in its own sidebar-less layout.
 * WHY:   Routes are built from the registry (02 §3.3), never listed by hand, so a nav entry and its route cannot
 *        disagree. Onboarding is not a nav entry (registry/nav.rs), so its one route comes from app/screen-pages.ts.
 *        Pages load lazily, so each waits in a Suspense boundary that shows nothing: a local chunk loads in
 *        milliseconds (04 §1 "Nothing waits"). The error boundaries sit below the layouts, so a failing page keeps
 *        the titlebar (and the sidebar). Pure (no router instance), so tests use it with a memory router and their
 *        own page table.
 * WHERE: app/router.tsx (hash router for the window); app tests (memory router).
 */
import { Suspense, type ComponentType } from "react";
import { Navigate, type RouteObject } from "react-router";
import type { NavItem } from "@/bindings";
import { sortNavItems } from "@/hooks/use-registry";
import { NAV_PAGES, type NavPages } from "./nav-page";
import { ONBOARDING_PAGE, ONBOARDING_ROUTE } from "./screen-pages";
import { AppRoot, OnboardingLayout, RouteError, ShellLayout } from "./shell";

export function buildAppRoutes(
  nav: readonly NavItem[],
  pages: NavPages = NAV_PAGES,
  Onboarding: ComponentType = ONBOARDING_PAGE,
): RouteObject[] {
  const items = sortNavItems(nav);
  const home = items[0];
  const pageRoutes: RouteObject[] = items.map((item) => {
    const Page = pages[item.id];
    return {
      path: item.route,
      element: (
        <Suspense fallback={null}>
          <Page nav={item} />
        </Suspense>
      ),
    };
  });
  const fallback: RouteObject[] =
    home === undefined ? [] : [{ path: "*", element: <Navigate to={home.route} replace /> }];
  return [
    {
      element: <AppRoot />,
      children: [
        {
          element: <ShellLayout />,
          children: [{ errorElement: <RouteError />, children: [...pageRoutes, ...fallback] }],
        },
        {
          path: ONBOARDING_ROUTE,
          element: <OnboardingLayout />,
          children: [
            {
              errorElement: <RouteError />,
              children: [
                {
                  index: true,
                  element: (
                    <Suspense fallback={null}>
                      <Onboarding />
                    </Suspense>
                  ),
                },
              ],
            },
          ],
        },
      ],
    },
  ];
}
