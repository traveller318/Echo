/**
 * SOURCE OF TRUTH KEYWORDS: OnboardingFlow, onboarding screen, step navigation, continue back finish, set up later, onboarding_complete, onboarding done
 * WHAT:  The full-window onboarding screen (04 §5): the dot indicator, the current step's title, explanation and
 *        component in a scrolling body, and a footer pinned to the bottom with "Set up later" and Back / Continue /
 *        Finish. Finish stores completion (`onboarding_complete`) and opens the first page in sidebar order.
 * WHY:   Onboarding owns the whole content area (no card inside the window), so a step like the model's ModelCard
 *        or the practice pad gets the room a page does, and only the body scrolls: the buttons stay in reach
 *        whatever the window height. The steps to walk come from Rust (registry/onboarding.rs via
 *        `onboarding_get`) and are fixed when the screen opens, so a model that finishes installing does not
 *        reshuffle the dots mid-flow; everything else each step shows stays live. Continue waits only where a step
 *        says so (the model). "Set up later" leaves without completing, so onboarding returns at the next launch
 *        while it is still due. Each step is keyed by its id, so its state (and its session rehearsal) starts
 *        fresh; focus moves to the step's heading so a screen reader hears the new step (04 §7). With nothing left
 *        to walk, the screen says Echo is ready.
 * WHERE: routes/onboarding/index.tsx.
 */
import { SparklesIcon } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { useNavigate } from "react-router";
import type { OnboardingView } from "@/bindings";
import { EmptyState } from "@/components/global";
import { Button } from "@/components/ui";
import { useCompleteOnboarding, useNavItems } from "@/hooks";
import { showToast } from "@/stores/toast-store";
import { ONBOARDING_STEP_PAGES } from "./onboarding-steps";
import { StepDots } from "./StepDots";

export interface OnboardingFlowProps {
  readonly view: OnboardingView;
}

export function OnboardingFlow({ view }: OnboardingFlowProps) {
  const [steps] = useState(() => view.steps);
  const [index, setIndex] = useState(0);
  const navigate = useNavigate();
  const home = useNavItems()[0]?.route ?? "/";
  const complete = useCompleteOnboarding();
  const heading = useRef<HTMLHeadingElement>(null);

  useEffect(() => {
    heading.current?.focus();
  }, [index]);

  const leave = () => {
    void navigate(home, { replace: true });
  };
  const finish = () => {
    complete.mutate(undefined, {
      onSuccess: () => {
        showToast({ title: "You're all set", body: "Use your hotkey in any app to dictate." });
        leave();
      },
    });
  };

  const step = steps[Math.min(index, steps.length - 1)];
  if (step === undefined) {
    return (
      <div className="flex h-full items-center justify-center p-content-pad">
        <EmptyState
          icon={<SparklesIcon />}
          titleAs="h1"
          title="Echo is ready"
          body="Everything is set up. Use your hotkey in any app to dictate."
          action={
            <Button variant="primary" onClick={leave}>
              Open Echo
            </Button>
          }
        />
      </div>
    );
  }

  const page = ONBOARDING_STEP_PAGES[step.id];
  const Step = page.Component;
  const last = index === steps.length - 1;
  const ready = page.canContinue(view);

  return (
    <section aria-labelledby={`onboarding-${step.id}`} data-slot="onboarding-flow" className="flex h-full flex-col">
      <div className="min-h-0 flex-1 overflow-y-auto">
        <div className="mx-auto flex w-full max-w-content flex-col gap-6 p-content-pad">
          <header className="flex flex-col gap-3">
            <StepDots steps={steps} current={index} />
            <h1 id={`onboarding-${step.id}`} ref={heading} tabIndex={-1} className="text-title1 text-fg">
              {page.title}
            </h1>
            <p className="max-w-measure text-body text-fg-secondary">{page.body}</p>
          </header>
          <Step key={step.id} view={view} step={step} />
        </div>
      </div>
      <footer className="border-t-(length:--border-hairline) border-separator">
        <div className="mx-auto flex w-full max-w-content items-center justify-between gap-3 px-content-pad py-4">
          <Button variant="ghost" onClick={leave}>
            Set up later
          </Button>
          <div className="flex items-center gap-2">
            {index === 0 ? null : (
              <Button
                variant="ghost"
                onClick={() => {
                  setIndex(index - 1);
                }}
              >
                Back
              </Button>
            )}
            {last ? (
              <Button variant="primary" disabled={!ready || complete.isPending} onClick={finish}>
                Finish
              </Button>
            ) : (
              <Button
                variant="primary"
                disabled={!ready}
                onClick={() => {
                  setIndex(index + 1);
                }}
              >
                Continue
              </Button>
            )}
          </div>
        </div>
      </footer>
    </section>
  );
}
