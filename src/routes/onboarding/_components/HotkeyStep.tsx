/**
 * SOURCE OF TRUTH KEYWORDS: HotkeyStep, onboarding hotkey test, press hotkey to test, HotkeyRehearsed, rebind hotkey on conflict, hold to talk test
 * WHAT:  Onboarding step 3: asks the user to press (or hold and release) the dictation hotkey and confirms each press
 *        Echo received, then offers the hotkey and its mode from the registry so a clashing combination can be
 *        rebound on the spot.
 * WHY:   The session rehearses hotkeys while this step shows (`session_rehearse("hotkey")`): a press in Echo's own
 *        window is reported as HotkeyRehearsed and starts no take, so testing never records or pastes. A combination
 *        another Echo hotkey uses is refused inline by its setting row (Hotkey{conflict}); a press that never arrives
 *        means another app takes the keys, which the hint says to fix by picking another combination. The test resets
 *        whenever the hotkey or its mode changes (keyed), so a new binding is tested from scratch.
 * WHERE: onboarding-steps.ts (the `hotkey` entry).
 */
import { CircleAlertIcon, CircleCheckIcon } from "lucide-react";
import { useState } from "react";
import type { KeyState } from "@/bindings";
import { ShortcutKeys } from "@/components/global";
import { useEchoEvent, useSessionRehearsal } from "@/hooks";
import { cn } from "@/lib/cn";
import type { OnboardingStepProps } from "./step-props";
import { StepSettings } from "./StepSettings";

interface HotkeyTestProps {
  readonly shortcut: string | null;
  readonly hold: boolean;
}

interface TestStatus {
  readonly text: string;
  readonly tone: "waiting" | "passed" | "warning";
}

function testStatus(heard: KeyState | null, hold: boolean): TestStatus {
  switch (heard) {
    case null:
      return { text: "Waiting for the hotkey.", tone: "waiting" };
    case "pressed":
      return hold
        ? { text: "Echo feels it. Now let go.", tone: "waiting" }
        : { text: "Echo heard it. Your hotkey works.", tone: "passed" };
    case "released":
      return { text: "Echo heard it. Your hotkey works.", tone: "passed" };
    case "interrupted":
      return { text: "Another key joined in. Press only the hotkey.", tone: "warning" };
  }
}

function HotkeyTest({ shortcut, hold }: HotkeyTestProps) {
  const [heard, setHeard] = useState<KeyState | null>(null);
  useEchoEvent("hotkeyRehearsed", (rehearsed) => {
    if (rehearsed.action === "record") {
      setHeard(rehearsed.state);
    }
  });
  const status = testStatus(heard, hold);
  const StatusIcon = status.tone === "warning" ? CircleAlertIcon : CircleCheckIcon;

  if (shortcut === null) {
    return <p className="text-body text-fg">No dictation hotkey is set. Choose one below.</p>;
  }
  return (
    <div className="flex flex-col gap-3 rounded-sm bg-fill p-4">
      <p className="flex flex-wrap items-center gap-2 text-body text-fg">
        {hold ? "Hold" : "Press"}
        <ShortcutKeys shortcut={shortcut} />
        {hold ? "for a moment, then let go." : "once."}
      </p>
      <p role="status" aria-live="polite" className="flex items-center gap-2 text-footnote text-fg-secondary">
        {status.tone === "waiting" ? null : (
          <StatusIcon
            aria-hidden="true"
            className={cn("size-icon-sm shrink-0", status.tone === "passed" ? "text-success" : "text-warning")}
          />
        )}
        {status.text}
      </p>
    </div>
  );
}

export function HotkeyStep({ view, step }: OnboardingStepProps) {
  useSessionRehearsal("hotkey");
  return (
    <div className="flex flex-col gap-4">
      <HotkeyTest
        key={`${view.record_hotkey ?? ""}:${String(view.hold_to_talk)}`}
        shortcut={view.record_hotkey}
        hold={view.hold_to_talk}
      />
      <StepSettings settings={step.settings} />
      <p className="text-footnote text-fg-secondary">
        Nothing happens when you press it? Another app may use the same keys. Pick a different hotkey above.
      </p>
    </div>
  );
}
