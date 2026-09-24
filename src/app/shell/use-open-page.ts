/**
 * SOURCE OF TRUTH KEYWORDS: useOpenPage, open page by NavId, navigate to registry route, page navigation
 * WHAT:  `useOpenPage()` returns a function that navigates the main window to the page of a NavId, using the route
 *        the registry gives it.
 * WHY:   Pages are named by NavId everywhere outside the router (error actions, navigation requests from the pill);
 *        their paths live only in the registry, so the lookup is written once. A NavId the registry does not list is
 *        logged, never thrown, because a stale request must not take the window down.
 * WHERE: app/shell/use-app-error-action.ts and app/shell/use-navigation-requests.ts; needs the router and the
 *        registry context, so it is used only inside the routed shell.
 */
import { useCallback } from "react";
import { useNavigate } from "react-router";
import type { NavId } from "@/bindings";
import { useRegistryView } from "@/hooks/use-registry";

export function useOpenPage(): (page: NavId) => void {
  const navigate = useNavigate();
  const { nav } = useRegistryView();
  return useCallback(
    (page: NavId) => {
      const item = nav.find((entry) => entry.id === page);
      if (item === undefined) {
        console.error("Echo has no page for this id", page);
        return;
      }
      void navigate(item.route);
    },
    [navigate, nav],
  );
}
