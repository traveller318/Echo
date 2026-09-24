/**
 * SOURCE OF TRUTH KEYWORDS: DashboardPage, dashboard route, dashboard page, page being set up, EmptyState
 * WHAT:  The dashboard page: its registry title and an EmptyState saying the page is being set up.
 * WHY:   The router needs a page for every registry nav entry (app/routes.tsx), and this is the one interim
 *        content the build allows (step 08). Step 20 replaces it with the registry-driven dashboard
 *        (04 §5): hero stat, stat cards, activity chart, recent takes.
 * WHERE: Lazy-loaded by app/routes.tsx for the `dashboard` nav entry.
 */
import { EmptyState, NavIcon, Page } from "@/components/global";
import type { NavPageProps } from "@/app/nav-page";

export default function DashboardPage({ nav }: NavPageProps) {
  return (
    <Page title={nav.label}>
      <EmptyState
        icon={<NavIcon icon={nav.icon} />}
        title="This page is being set up"
        body="Time saved, words and speaking pace will show here."
      />
    </Page>
  );
}
