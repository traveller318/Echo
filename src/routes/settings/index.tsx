/**
 * SOURCE OF TRUTH KEYWORDS: SettingsPage, settings route, settings page, page being set up, EmptyState
 * WHAT:  The settings page: its registry title and an EmptyState saying the page is being set up.
 * WHY:   The router needs a page for every registry nav entry (app/routes.tsx), and this is the one interim
 *        content the build allows (step 08). Step 18 replaces it with the sections generated from the
 *        registry settings (04 §5).
 * WHERE: Lazy-loaded by app/routes.tsx for the `settings` nav entry.
 */
import { EmptyState, NavIcon, Page } from "@/components/global";
import type { NavPageProps } from "@/app/nav-page";

export default function SettingsPage({ nav }: NavPageProps) {
  return (
    <Page title={nav.label}>
      <EmptyState
        icon={<NavIcon icon={nav.icon} />}
        title="This page is being set up"
        body="Hotkeys, audio, output and privacy options will show here."
      />
    </Page>
  );
}
