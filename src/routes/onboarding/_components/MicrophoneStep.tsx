/**
 * SOURCE OF TRUTH KEYWORDS: MicrophoneStep, onboarding microphone, privacy consent check, microphone privacy deep link, live level meter, input device choice
 * WHAT:  Onboarding step 1: whether Windows lets desktop apps use the microphone (with the privacy settings link when
 *        it does not), the microphone choice from the registry, and a live microphone test.
 * WHY:   05 W13: a blocked consent makes the microphone deliver silence, so it is checked first and explained with the
 *        same copy and action a refused take would give (lib/app-error.ts). Windows changes the consent without
 *        telling Echo; coming back from the Settings app focuses the window, which reads it again (no polling). The
 *        test listens to the microphone the device setting names (found by its kind, never its key), so the level
 *        that moves is exactly what a take would hear.
 * WHERE: onboarding-steps.ts (the `microphone` entry).
 */
import { MicOffIcon } from "lucide-react";
import type { AudioDeviceId, SettingKey } from "@/bindings";
import { useAppErrorAction } from "@/app/shell/use-app-error-action";
import { InlineNotice, MicCheckPanel } from "@/components/global";
import { Button } from "@/components/ui";
import { ONBOARDING_QUERY, useRefreshOnWindowFocus, useRegistryView, useSettingValues } from "@/hooks";
import { describeAppError } from "@/lib/app-error";
import type { OnboardingStepProps } from "./step-props";
import { StepSettings } from "./StepSettings";

const BLOCKED = describeAppError({ code: "PermissionDenied", permission: "microphone" });

/** The microphone the step's device setting names now; null follows the Windows default. */
function useChosenDevice(keys: readonly SettingKey[]): AudioDeviceId | null {
  const { settings } = useRegistryView();
  const values = useSettingValues();
  const spec = settings.find((entry) => keys.includes(entry.key) && entry.kind.kind === "device");
  const value = spec === undefined ? undefined : values.data?.get(spec.key);
  return value?.kind === "device" ? value.value : null;
}

export function MicrophoneStep({ view, step }: OnboardingStepProps) {
  useRefreshOnWindowFocus(ONBOARDING_QUERY);
  const performAction = useAppErrorAction();
  const device = useChosenDevice(step.settings);
  const blockedAction = BLOCKED.action;

  return (
    <div className="flex flex-col gap-4">
      {view.microphone === "denied" ? (
        <InlineNotice
          icon={<MicOffIcon />}
          title={BLOCKED.title}
          body={BLOCKED.body}
          action={
            blockedAction === null ? undefined : (
              <Button
                size="sm"
                onClick={() => {
                  performAction(blockedAction.id);
                }}
              >
                {blockedAction.label}
              </Button>
            )
          }
        />
      ) : null}
      {view.microphone === null ? (
        <InlineNotice
          icon={<MicOffIcon />}
          title="Couldn't check microphone access"
          body="Run the test below. If it hears nothing, check Windows privacy settings."
        />
      ) : null}
      <StepSettings settings={step.settings} />
      <MicCheckPanel device={device} />
    </div>
  );
}
