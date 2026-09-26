/**
 * SOURCE OF TRUTH KEYWORDS: OnboardingPage, onboarding route, first-run onboarding, setup screen, full-window onboarding
 * WHAT:  The onboarding screen (04 §5): fills the content area with the flow that walks the steps Rust says are due
 *        (microphone, speech model, practice take), or a centred calm error with "Try again" when they cannot be read.
 * WHY:   01 §7: onboarding gets a fresh install to a working first take with no manual steps. Whether it is due and
 *        which steps apply is Rust's answer (`onboarding_get`), read once and kept fresh by ModelsChanged and
 *        SettingsChanged; the page keeps no copy. Loading shows an indeterminate bar only after --delay-loading.
 * WHERE: Lazy-loaded by app/routes.tsx at ONBOARDING_ROUTE (app/screen-pages.ts), inside OnboardingLayout.
 */
import { TriangleAlertIcon } from "lucide-react";
import { EmptyState, ProgressBar } from "@/components/global";
import { Button } from "@/components/ui";
import { useOnboarding } from "@/hooks";
import { describeAppError, toAppError } from "@/lib/app-error";
import { OnboardingFlow } from "./_components/OnboardingFlow";

export default function OnboardingPage() {
  const onboarding = useOnboarding();

  if (!onboarding.isError && onboarding.data !== undefined) {
    return (
      <div data-slot="onboarding" className="h-full">
        <OnboardingFlow view={onboarding.data} />
      </div>
    );
  }

  const copy = onboarding.isError ? describeAppError(toAppError(onboarding.error)) : null;
  return (
    <div data-slot="onboarding" className="flex min-h-full flex-col items-center justify-center p-content-pad">
      {copy === null ? (
        <ProgressBar value={null} aria-label="Loading setup" className="max-w-measure" />
      ) : (
        <EmptyState
          icon={<TriangleAlertIcon />}
          titleAs="h1"
          title={copy.title}
          body={copy.body}
          action={
            <Button
              variant="primary"
              onClick={() => {
                void onboarding.refetch();
              }}
            >
              Try again
            </Button>
          }
        />
      )}
    </div>
  );
}
