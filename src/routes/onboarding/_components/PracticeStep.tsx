/**
 * SOURCE OF TRUTH KEYWORDS: PracticeStep, onboarding practice take, try it, rehearsed take, text shown in card, DeliveryOutcome shown, first take
 * WHAT:  Onboarding step 4: one practice take. It says how to dictate with the hotkey as bound now, follows the take
 *        live (listening, transcribing), then shows its text in the card, or why nothing came out.
 * WHY:   The session rehearses a take while this step shows (`session_rehearse("take")`): a take started in Echo's own
 *        window runs the whole pipeline (microphone, speech engine, polish, History) and is delivered as `shown`, so
 *        its text is read back here instead of pasted into Echo (step 24). The take's state is the session's
 *        (SessionStateChanged) and its text is History's (`history_get`); the step keeps only which take to show.
 *        A take that reached another app (the user clicked elsewhere first) is explained rather than shown.
 * WHERE: onboarding-steps.ts (the `practice` entry).
 */
import { CircleAlertIcon, CircleCheckIcon } from "lucide-react";
import { useState } from "react";
import type { AppError, SessionView, TranscriptId } from "@/bindings";
import { ShortcutKeys } from "@/components/global";
import { useEchoEvent, useSessionRehearsal, useSessionView, useTranscript } from "@/hooks";
import { describeAppError } from "@/lib/app-error";
import type { OnboardingStepProps } from "./step-props";

/** How the latest practice take ended. */
type PracticeResult =
  | { readonly kind: "shown"; readonly id: TranscriptId }
  | { readonly kind: "elsewhere" }
  | { readonly kind: "no_speech" }
  | { readonly kind: "failed"; readonly error: AppError };

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

function ShownText({ id }: { readonly id: TranscriptId }) {
  const transcript = useTranscript(id);
  const text = transcript.data?.final_text ?? transcript.data?.raw_text ?? null;
  return (
    <div className="flex flex-col gap-2">
      <p className="min-h-control rounded-sm bg-fill p-4 text-body text-fg select-text">{text ?? "Loading the text."}</p>
      <p className="flex items-center gap-2 text-footnote text-fg-secondary">
        <CircleCheckIcon aria-hidden="true" className="size-icon-sm shrink-0 text-success" />
        Echo is ready. From now on, the text goes into the app you are typing in.
      </p>
    </div>
  );
}

function ResultNote({ title, body }: { readonly title: string; readonly body: string }) {
  return (
    <p className="flex items-start gap-2 text-footnote text-fg-secondary">
      <CircleAlertIcon aria-hidden="true" className="size-icon-sm shrink-0 text-warning" />
      <span>
        {title}. {body}
      </span>
    </p>
  );
}

function Result({ result }: { readonly result: PracticeResult }) {
  switch (result.kind) {
    case "shown":
      return <ShownText id={result.id} />;
    case "elsewhere":
      return (
        <ResultNote
          title="That take went to another app"
          body="Click into this window first, then dictate again to see the text here."
        />
      );
    case "no_speech":
      return <ResultNote title="No speech detected" body="Try again, a little closer to the microphone." />;
    case "failed": {
      const copy = describeAppError(result.error);
      return <ResultNote title={copy.title} body={copy.body} />;
    }
  }
}

export function PracticeStep({ view }: OnboardingStepProps) {
  useSessionRehearsal("take");
  const session = useSessionView();
  const [result, setResult] = useState<PracticeResult | null>(null);
  useEchoEvent("sessionStateChanged", (next) => {
    if (next.status === "arming") {
      setResult(null);
      return;
    }
    const ended = resultOf(next);
    if (ended !== null) {
      setResult(ended);
    }
  });
  const live = liveLine(session, view.hold_to_talk);
  const shortcut = view.record_hotkey;

  return (
    <div className="flex flex-col gap-4">
      {shortcut === null ? (
        <p className="text-body text-fg">Set a dictation hotkey first, then come back to try it.</p>
      ) : (
        <p className="flex flex-wrap items-center gap-2 text-body text-fg">
          {view.hold_to_talk ? "Hold" : "Press"}
          <ShortcutKeys shortcut={shortcut} />
          {view.hold_to_talk ? ", say a sentence, then let go." : ", say a sentence, then press it again."}
        </p>
      )}
      <p role="status" aria-live="polite" className="text-footnote text-fg-secondary">
        {live ?? "Keep this window in front: your words appear here instead of being pasted."}
      </p>
      {result === null ? null : <Result result={result} />}
    </div>
  );
}
