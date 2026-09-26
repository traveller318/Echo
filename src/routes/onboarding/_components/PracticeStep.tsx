/**
 * SOURCE OF TRUTH KEYWORDS: PracticeStep, onboarding try it, practice take, hotkey test, rehearsed take, practice pad, rebind hotkey on conflict, DeliveryOutcome shown, Echo not in front
 * WHAT:  Onboarding's last step, the hotkey test and the practice take in one: says how to dictate with the hotkey
 *        as bound now, follows each take live (listening, transcribing), writes its text into the practice pad, or
 *        says why nothing came out; below, the hotkey and its mode from the registry so a clash is rebound on the
 *        spot.
 * WHY:   A take that starts proves the hotkey, and its text proves the microphone, the speech model and the polish
 *        chain, so one screen tests everything without switching to another app. The session rehearses a take while
 *        this step shows (`session_rehearse("take")`): a take started in Echo's own window runs the whole pipeline
 *        and is delivered as `shown`, so its text lands in the pad instead of being pasted into Echo. The take's
 *        state is the session's (SessionStateChanged) and its text is History's; the step keeps only which takes to
 *        show. The take goes to whichever window is in front when the hotkey is pressed, and a visible Echo is not
 *        always the one in front (another monitor, a click elsewhere), so the step follows the window's focus
 *        (useWindowFocused): the pad and the line under it say "click here first" until Echo is in front. A take
 *        that still reached another app is explained rather than shown.
 *        A press that never starts a take means another app holds the keys, which the hint says to fix below.
 * WHERE: onboarding-steps.ts (the `practice` entry).
 */
import { CircleAlertIcon, CircleCheckIcon } from "lucide-react";
import { useState } from "react";
import type { AppError, SessionView, TranscriptId } from "@/bindings";
import { ShortcutKeys } from "@/components/global";
import { useEchoEvent, useSessionRehearsal, useSessionView, useWindowFocused } from "@/hooks";
import { describeAppError } from "@/lib/app-error";
import { PracticePad } from "./PracticePad";
import type { OnboardingStepProps } from "./step-props";
import { StepSettings } from "./StepSettings";

/** How the latest practice take ended. */
type PracticeResult =
  | { readonly kind: "shown"; readonly id: TranscriptId }
  | { readonly kind: "elsewhere" }
  | { readonly kind: "no_speech" }
  | { readonly kind: "failed"; readonly error: AppError };

/** A take that ended without text for the pad. */
type PracticeMiss = Exclude<PracticeResult, { readonly kind: "shown" }>;

/** The result a settled view means; null while a take runs or nothing ended. */
function resultOf(view: SessionView): PracticeResult | null {
  if (view.status === "failed" && view.error !== null) {
    return { kind: "failed", error: view.error };
  }
  if (view.status !== "done" || view.outcome === null) {
    return null;
  }
  switch (view.outcome) {
    case "shown":
      return view.transcript_id === null ? null : { kind: "shown", id: view.transcript_id };
    case "no_speech":
      return { kind: "no_speech" };
    case "pasted":
    case "copied":
      return { kind: "elsewhere" };
  }
}

/** What the running take is doing, for the live line; null when none runs. */
function liveLine(view: SessionView | null, hold: boolean): string | null {
  switch (view?.status) {
    case "arming":
    case "recording":
      return hold ? "Listening. Let go when you're done." : "Listening. Press the hotkey again when you're done.";
    case "cancel_pending":
      return "Cancelling. Press Esc to keep recording.";
    case "finalizing":
    case "delivering":
      return "Transcribing.";
    default:
      return null;
  }
}

function missCopy(miss: PracticeMiss): { readonly title: string; readonly body: string } {
  switch (miss.kind) {
    case "elsewhere":
      return {
        title: "That take went to the app in front",
        body: "Click the box above so Echo is in front, then dictate again.",
      };
    case "no_speech":
      return { title: "No speech detected", body: "Try again, a little closer to the microphone." };
    case "failed":
      return describeAppError(miss.error);
  }
}

function MissNote({ miss }: { readonly miss: PracticeMiss }) {
  const copy = missCopy(miss);
  return (
    <p className="flex items-start gap-2 text-footnote text-fg-secondary">
      <CircleAlertIcon aria-hidden="true" className="size-icon-sm shrink-0 text-warning" />
      <span>
        {copy.title}. {copy.body}
      </span>
    </p>
  );
}

export function PracticeStep({ view, step }: OnboardingStepProps) {
  useSessionRehearsal("take");
  const session = useSessionView();
  const focused = useWindowFocused();
  const [takes, setTakes] = useState<readonly TranscriptId[]>([]);
  const [miss, setMiss] = useState<PracticeMiss | null>(null);
  useEchoEvent("sessionStateChanged", (next) => {
    if (next.status === "arming") {
      setMiss(null);
      return;
    }
    const ended = resultOf(next);
    if (ended === null) {
      return;
    }
    if (ended.kind === "shown") {
      setMiss(null);
      setTakes((shown) => (shown.includes(ended.id) ? shown : [...shown, ended.id]));
    } else {
      setMiss(ended);
    }
  });
  const live = liveLine(session, view.hold_to_talk);
  const shortcut = view.record_hotkey;

  return (
    <div className="flex flex-col gap-4">
      {shortcut === null ? (
        <p className="text-body text-fg">No dictation hotkey is set. Choose one below.</p>
      ) : (
        <p className="flex flex-wrap items-center gap-2 text-body text-fg">
          {view.hold_to_talk ? "Hold" : "Press"}
          <ShortcutKeys shortcut={shortcut} />
          {view.hold_to_talk ? ", say a sentence, then let go." : ", say a sentence, then press it again."}
        </p>
      )}
      <PracticePad takes={takes} ready={focused} />
      <p role="status" aria-live="polite" className="text-footnote text-fg-secondary">
        {live ??
          (focused
            ? "Echo is in front, so your words appear in the box instead of being pasted."
            : "Another app is in front, so the hotkey types there. Click the box to try it here.")}
      </p>
      {miss === null ? null : <MissNote miss={miss} />}
      {takes.length === 0 ? null : (
        <p className="flex items-center gap-2 text-footnote text-fg-secondary">
          <CircleCheckIcon aria-hidden="true" className="size-icon-sm shrink-0 text-success" />
          Echo is ready. From now on, the text goes into the app you are typing in.
        </p>
      )}
      <StepSettings settings={step.settings} />
      <p className="text-footnote text-fg-secondary">
        Nothing happens when you press it? Another app may use the same keys. Pick a different hotkey above.
      </p>
    </div>
  );
}
