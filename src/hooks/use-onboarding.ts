/**
 * SOURCE OF TRUTH KEYWORDS: useOnboarding, useCompleteOnboarding, useRefreshOnWindowFocus, ONBOARDING_QUERY, ONBOARDING_EVENTS, onboarding_get, onboarding_complete, first-run onboarding data
 * WHAT:  The onboarding data layer: `useOnboarding()` reads `onboarding_get` (whether onboarding is due, the steps to
 *        walk, the speech engine and its model's state, the microphone consent); `useCompleteOnboarding()` finishes
 *        it (`onboarding_complete`); `useRefreshOnWindowFocus(query, enabled)` reads a query again whenever the window
 *        regains focus.
 * WHY:   Rust decides whether onboarding is due and which steps apply (root CLAUDE.md §7: the UI keeps no copy): the
 *        view refetches on ModelsChanged (a download finished) and SettingsChanged (completion, a new engine), never
 *        by polling. Windows changes the microphone consent without telling Echo, and the user changes it in the
 *        Settings app the privacy link opened; coming back to Echo focuses the window, so that event (not a timer) is
 *        when the consent is read again.
 * WHERE: app/onboarding-gate.tsx (whether to open onboarding at launch); routes/onboarding (the flow and its steps).
 */
import { useQueryClient, type UseQueryResult } from "@tanstack/react-query";
import { useEffect } from "react";
import { commands, type OnboardingView } from "@/bindings";
import type { EchoEventName } from "@/lib/echo-events";
import { useEchoMutation } from "./use-echo-mutation";
import { useEchoQuery, type EchoQuery } from "./use-echo-query";

/** Rust events after which the onboarding view is stale. */
export const ONBOARDING_EVENTS: readonly EchoEventName[] = ["modelsChanged", "settingsChanged"];

export const ONBOARDING_QUERY: EchoQuery<OnboardingView> = {
  queryKey: ["onboarding", "view"],
  command: commands.onboardingGet,
  invalidatedBy: ONBOARDING_EVENTS,
};

export function useOnboarding(): UseQueryResult<OnboardingView> {
  return useEchoQuery(ONBOARDING_QUERY);
}

export function useCompleteOnboarding() {
  return useEchoMutation<undefined, OnboardingView>(commands.onboardingComplete);
}

/** Reads `query` again each time the window regains focus, while `enabled`. */
export function useRefreshOnWindowFocus<T>(query: EchoQuery<T>, enabled = true): void {
  const client = useQueryClient();
  const { queryKey } = query;
  useEffect(() => {
    if (!enabled) {
      return undefined;
    }
    const refresh = () => {
      void client.invalidateQueries({ queryKey });
    };
    window.addEventListener("focus", refresh);
    return () => {
      window.removeEventListener("focus", refresh);
    };
  }, [client, queryKey, enabled]);
}
