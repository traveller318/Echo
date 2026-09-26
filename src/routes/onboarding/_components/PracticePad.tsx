/**
 * SOURCE OF TRUTH KEYWORDS: PracticePad, practice pad, onboarding text box, dictated text shown, rehearsed take text, try it pad, pad ready state, click to focus
 * WHAT:  The small box onboarding's "Try it" step dictates into: the text of every practice take so far (read from
 *        History by id), oldest first. `ready` (Echo's window is in front) gives it the accent border and the "hold
 *        the hotkey" placeholder; otherwise the placeholder asks for a click, which focuses the pad and so Echo.
 * WHY:   A practice take is shown in Echo instead of pasted (`session_rehearse("take")`), but only when Echo's window
 *        is the foreground one when the hotkey is pressed (pipeline/session/rehearsal.rs); a visible but unfocused
 *        Echo sends the take to the app in front. The pad therefore looks like a focused text field only while that
 *        holds, and is itself a click target (tabIndex) so "click the box" is the one fix. It stays a log region,
 *        not an editable field (05 decision log 2026-09-26: no second delivery path, no form without a schema). The
 *        text is History's (`history_get` via useTranscript); the pad holds only which takes to show.
 * WHERE: PracticeStep (this folder).
 */
import type { TranscriptId } from "@/bindings";
import { useTranscript } from "@/hooks";

export interface PracticePadProps {
  /** The shown practice takes, oldest first. */
  readonly takes: readonly TranscriptId[];
  /** Echo's window is in front, so a take started now lands here. */
  readonly ready: boolean;
}

function PadLine({ id }: { readonly id: TranscriptId }) {
  const transcript = useTranscript(id);
  const text = transcript.data?.final_text ?? transcript.data?.raw_text ?? null;
  return <p className={text === null ? "text-fg-tertiary" : undefined}>{text ?? "Loading the text."}</p>;
}

export function PracticePad({ takes, ready }: PracticePadProps) {
  return (
    <div
      role="log"
      aria-label="Practice pad"
      tabIndex={0}
      data-slot="practice-pad"
      data-ready={ready}
      className="flex min-h-practice-pad cursor-text flex-col gap-2 rounded-control border-(length:--border-hairline) border-transparent bg-fill px-3 py-2 text-body text-fg transition-[border-color] duration-(--duration-fast) ease-standard select-text data-[ready=true]:border-accent"
    >
      {takes.length === 0 ? (
        <p className="text-fg-tertiary">{ready ? "Your words appear here." : "Click here first."}</p>
      ) : (
        takes.map((id) => <PadLine key={id} id={id} />)
      )}
    </div>
  );
}
