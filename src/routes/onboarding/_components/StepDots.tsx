/**
 * SOURCE OF TRUTH KEYWORDS: StepDots, step indicator, onboarding dots, setup progress, aria-current step
 * WHAT:  The onboarding dot indicator: one `--step-dot` dot per step, the current one in the accent, finished ones
 *        in the secondary text colour, the rest as a fill; each dot names its step for assistive tech.
 * WHY:   04 §5: a dot indicator, one dot per step. Colour alone never carries state (04 §7): every dot has a text label
 *        ("Step 2 of 3: Speech model, current"), and the current one is marked with aria-current.
 * WHERE: OnboardingFlow (this folder).
 */
import type { OnboardingStepSpec } from "@/bindings";
import { cn } from "@/lib/cn";

export interface StepDotsProps {
  readonly steps: readonly OnboardingStepSpec[];
  readonly current: number;
}

function dotLabel(step: OnboardingStepSpec, index: number, total: number, current: number): string {
  const state = index < current ? ", done" : index === current ? ", current" : "";
  return `Step ${String(index + 1)} of ${String(total)}: ${step.label}${state}`;
}

export function StepDots({ steps, current }: StepDotsProps) {
  return (
    <ol aria-label="Setup progress" className="flex items-center gap-2">
      {steps.map((step, index) => (
        <li
          key={step.id}
          aria-current={index === current ? "step" : undefined}
          className={cn(
            "size-step-dot rounded-pill transition-[background-color] duration-(--duration-base) ease-standard",
            index === current ? "bg-accent" : index < current ? "bg-fg-secondary" : "bg-fill-pressed",
          )}
        >
          <span className="sr-only">{dotLabel(step, index, steps.length, current)}</span>
        </li>
      ))}
    </ol>
  );
}
