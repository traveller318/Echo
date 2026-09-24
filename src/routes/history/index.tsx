/**
 * SOURCE OF TRUTH KEYWORDS: HistoryPage, history route, history page, page being set up, EmptyState
 * WHAT:  The history page: its registry title and an EmptyState saying the page is being set up.
 * WHY:   The router needs a page for every registry nav entry (app/routes.tsx), and this is the one interim
 *        content the build allows (step 08). Step 16 replaces it with the searchable, virtualized history
 *        list and its actions (04 §5).
 * WHERE: Lazy-loaded by app/routes.tsx for the `history` nav entry.
 */
import { EmptyState, NavIcon, Page } from "@/components/global";
import type { NavPageProps } from "@/app/nav-page";

export default function HistoryPage({ nav }: NavPageProps) {
  return (
    <Page title={nav.label}>
      <EmptyState
        icon={<NavIcon icon={nav.icon} />}
        title="This page is being set up"
        body="Every take you dictate will be listed here, with search."
      />
    </Page>
  );
}
