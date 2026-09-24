/**
 * SOURCE OF TRUTH KEYWORDS: useNavigationRequests, NavigationRequested, app_open_page, navigate on request, pill Set up
 * WHAT:  `useNavigationRequests()` navigates the main window to the page every NavigationRequested event names.
 * WHY:   Surfaces without the router (the pill's "Set up" and "Open") call `app_open_page`; Rust brings this window
 *        forward and sends the event, and the router here is the one place that can act on it.
 * WHERE: app/shell/ShellLayout.tsx (the routed shell, so it runs once per main window).
 */
import { useEchoEvent } from "@/hooks/use-echo-event";
import { useOpenPage } from "./use-open-page";

export function useNavigationRequests(): void {
  const openPage = useOpenPage();
  useEchoEvent("navigationRequested", ({ page }) => {
    openPage(page);
  });
}
