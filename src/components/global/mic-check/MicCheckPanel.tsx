/**
 * SOURCE OF TRUTH KEYWORDS: MicCheckPanel, microphone test, live level meter, test microphone button, mic verdict, audio_test_level UI
 * WHAT:  A microphone test: a live level meter, a "Test microphone" button that listens to `device` (null: the
 *        Windows default) for a few seconds, and one status line (what to do now, the verdict, or why it failed).
 * WHY:   05 §4: the microphone is proven before a take depends on it. The meter draws the AudioLevel events the check
 *        sends while it listens (useMicCheck), so what moves is exactly what a take would hear; the verdict and the
 *        failure come from Rust and are worded once (mic-verdict.ts, lib/app-error.ts). Status is text with a glyph,
 *        never colour alone (04 §7), and a polite live region announces it. The device is a prop, so any surface
 *        (onboarding, a later Settings test) passes the one it shows.
 * WHERE: routes/onboarding (the microphone step). Exported through components/global.
 */
import { CircleAlertIcon, CircleCheckIcon, MicIcon } from "lucide-react";
import type { AudioDeviceId } from "@/bindings";
import { Button } from "@/components/ui";
import { useMicCheck } from "@/hooks/use-mic-check";
import { describeAppError } from "@/lib/app-error";
import { cn } from "@/lib/cn";
import { ProgressBar } from "../progress-bar";
import { MIC_VERDICT_LOOK } from "./mic-verdict";

export interface MicCheckPanelProps {
  /** The microphone to test; null tests the Windows default. */
  readonly device: AudioDeviceId | null;
  readonly className?: string;
}

const LEVEL_MAX = 1;

export function MicCheckPanel({ device, className }: MicCheckPanelProps) {
  const check = useMicCheck();
  const look = check.result === null ? null : MIC_VERDICT_LOOK[check.result.verdict];
  const failure = check.error === null ? null : describeAppError(check.error);

  const status = (() => {
    if (check.running) {
      return { icon: null, text: "Listening. Say a sentence as you normally would." };
    }
    if (look !== null) {
      return { icon: look.passed ? CircleCheckIcon : CircleAlertIcon, text: `${look.title}. ${look.body}` };
    }
    if (failure !== null) {
      return { icon: CircleAlertIcon, text: `${failure.title}. ${failure.body}` };
    }
    return { icon: null, text: "Start the test, then speak for a few seconds." };
  })();
  const StatusIcon = status.icon;

  return (
    <div data-slot="mic-check" className={cn("flex flex-col gap-3", className)}>
      <div className="flex items-center gap-3">
        <MicIcon aria-hidden="true" className="size-icon-md shrink-0 text-fg-secondary" />
        <ProgressBar value={check.level} max={LEVEL_MAX} aria-label="Microphone level" className="flex-1" />
        <Button
          variant={look?.passed === true ? "secondary" : "primary"}
          disabled={check.running}
          onClick={() => {
            check.run(device);
          }}
        >
          {check.running ? "Listening" : check.result === null ? "Test microphone" : "Test again"}
        </Button>
      </div>
      <p role="status" aria-live="polite" className="flex items-start gap-2 text-footnote text-fg-secondary">
        {StatusIcon === null ? null : (
          <StatusIcon
            aria-hidden="true"
            className={cn("size-icon-sm shrink-0", look?.passed === true ? "text-success" : "text-warning")}
          />
        )}
        <span>{status.text}</span>
      </p>
    </div>
  );
}
