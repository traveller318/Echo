/**
 * SOURCE OF TRUTH KEYWORDS: NavPageProps, NAV_PAGES, NavPages, route page props, page table, lazy pages, NavId
 * WHAT:  The contract between the router and the sidebar pages: `NavPageProps` (every page receives its own
 *        registry NavItem) and NAV_PAGES, the lazily loaded page component for each NavId.
 * WHY:   Page title and icon come from the registry entry (02 §3.3) instead of being written again in each route,
 *        so renaming a page is one registry edit. NAV_PAGES is keyed by the generated NavId union, so a new nav id
 *        in Rust fails tsc until its route folder (src/routes/<id>/) exists. Pages load lazily so the first paint
 *        carries only the shell, and heavy pages (Recharts on the dashboard) cost nothing until opened. It lives
 *        apart from the route builder because a file that defines lazy components may export nothing else
 *        (fast refresh).
 * WHERE: app/routes.tsx (buildAppRoutes default page table); src/routes/<id>/index.tsx (default exports).
 */
import { lazy, type ComponentType, type LazyExoticComponent } from "react";
import type { NavId, NavItem } from "@/bindings";

export interface NavPageProps {
  readonly nav: NavItem;
}

export type NavPage = LazyExoticComponent<ComponentType<NavPageProps>> | ComponentType<NavPageProps>;

export type NavPages = Readonly<Record<NavId, NavPage>>;

export const NAV_PAGES: NavPages = {
  dashboard: lazy(() => import("@/routes/dashboard")),
  history: lazy(() => import("@/routes/history")),
  dictionary: lazy(() => import("@/routes/dictionary")),
  models: lazy(() => import("@/routes/models")),
  settings: lazy(() => import("@/routes/settings")),
};
