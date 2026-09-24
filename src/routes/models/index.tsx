/**
 * SOURCE OF TRUTH KEYWORDS: ModelsPage, models route, models page, page being set up, EmptyState
 * WHAT:  The models page: its registry title and an EmptyState saying the page is being set up.
 * WHY:   The router needs a page for every registry nav entry (app/routes.tsx), and this is the one interim
 *        content the build allows (step 08). Step 21 replaces it with one card per registry engine and the
 *        model manager actions (04 §5).
 * WHERE: Lazy-loaded by app/routes.tsx for the `models` nav entry.
 */
import { EmptyState, NavIcon, Page } from "@/components/global";
import type { NavPageProps } from "@/app/nav-page";

export default function ModelsPage({ nav }: NavPageProps) {
  return (
    <Page title={nav.label}>
      <EmptyState
        icon={<NavIcon icon={nav.icon} />}
        title="This page is being set up"
        body="Speech models to download, import and switch between will show here."
      />
    </Page>
  );
}
