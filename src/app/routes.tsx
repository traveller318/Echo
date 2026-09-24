/**
 * SOURCE OF TRUTH KEYWORDS: buildAppRoutes, route table, registry routes, page error boundary, fallback redirect, RouteObject
 * WHAT:  `buildAppRoutes(nav, pages)` turns the registry nav into the router's route objects: the ShellLayout around
 *        one route per nav entry (its registry `route` → its page from NAV_PAGES, given its NavItem), a page-level
 *        error boundary, and a catch-all that redirects to the first page in sidebar order.
 * WHY:   Routes are built from the registry (02 §3.3), never listed by hand, so a nav entry and its route cannot
 *        disagree. Pages load lazily (app/nav-page.ts), so each waits in a Suspense boundary that shows nothing:
 *        a local chunk loads in milliseconds (04 §1 "Nothing waits"). The error boundary sits below the layout,
 *        so a failing page keeps the sidebar. Pure (no router instance), so tests use it with a memory router
 *        and their own page table.
 * WHERE: app/router.tsx (hash router for the window); app tests (memory router). Onboarding (a route without a
 *        nav entry) joins as one more child here in step 24.
 */
import { Suspense } from "react";
import { Navigate, type RouteObject } from "react-router";
import type { NavItem } from "@/bindings";
import { sortNavItems } from "@/hooks/use-registry";
import { NAV_PAGES, type NavPages } from "./nav-page";
import { RouteError, ShellLayout } from "./shell";

export function buildAppRoutes(nav: readonly NavItem[], pages: NavPages = NAV_PAGES): RouteObject[] {
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
      element: <ShellLayout />,
      children: [{ errorElement: <RouteError />, children: [...pageRoutes, ...fallback] }],
    },
  ];
}
