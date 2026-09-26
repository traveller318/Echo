/**
 * SOURCE OF TRUTH KEYWORDS: useOnboardingGate, onboarding at launch, OnboardingRequested, open onboarding, first run redirect, missing model redirect
 * WHAT:  `useOnboardingGate()` opens onboarding once per window launch when Rust says it is due (first run, or the
 *        speech model is missing), and whenever OnboardingRequested arrives (the pill's "Model not installed · Set up").
 * WHY:   01 §7 and step 24: onboarding is shown on first run and whenever the active speech model is missing. The
 *        decision is Rust's (`onboarding_get`); the window acts on the first answer only, so removing a model from the
 *        Models page later never yanks the user away mid-task (the pill's "Set up" and the next launch bring it back).
 *        A failed read leaves the window where it is: dictation still explains a missing model through the pill.
 * WHERE: app/shell/AppRoot.tsx (the root route, so it runs once per main window, inside the router).
 */
import { useEffect, useRef } from "react";
import { useLocation, useNavigate } from "react-router";
import { useEchoEvent } from "@/hooks/use-echo-event";
import { useOnboarding } from "@/hooks/use-onboarding";
import { ONBOARDING_ROUTE } from "../screen-pages";

export function useOnboardingGate(): void {
  const navigate = useNavigate();
  const { pathname } = useLocation();
  const onboarding = useOnboarding();
  const decided = useRef(false);
  const required = onboarding.data?.required;

  useEffect(() => {
    if (decided.current || required === undefined) {
      return;
    }
    decided.current = true;
    if (required && pathname !== ONBOARDING_ROUTE) {
      void navigate(ONBOARDING_ROUTE, { replace: true });
    }
  }, [required, pathname, navigate]);

  useEchoEvent("onboardingRequested", () => {
    void navigate(ONBOARDING_ROUTE);
  });
}
