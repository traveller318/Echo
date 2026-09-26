/**
 * SOURCE OF TRUTH KEYWORDS: PracticePad, practice pad, onboarding text box, dictated text shown, rehearsed take text, try it pad
 * WHAT:  The box onboarding's "Try it" step dictates into: the text of every practice take so far (read from
 *        History by id), oldest first, or a placeholder until the first take lands.
 * WHY:   A practice take is shown in Echo instead of pasted (`session_rehearse("take")`), so the user needs a place
 *        in this window where the words visibly land, the way they would in Notepad, without leaving setup. The
 *        text is History's (`history_get` via useTranscript); the pad holds only which takes to show. It is a log
 *        region, so each new take is read out once it arrives.
 * WHERE: PracticeStep (this folder).
 */
import type { TranscriptId } from "@/bindings";
import { useTranscript } from "@/hooks";

export interface PracticePadProps {
  /** The shown practice takes, oldest first. */
  readonly takes: readonly TranscriptId[];
}

function PadLine({ id }: { readonly id: TranscriptId }) {
  const transcript = useTranscript(id);
  const text = transcript.data?.final_text ?? transcript.data?.raw_text ?? null;
  return <p className={text === null ? "text-fg-tertiary" : undefined}>{text ?? "Loading the text."}</p>;
}

export function PracticePad({ takes }: PracticePadProps) {
  return (
    <div
      role="log"
      aria-label="Practice pad"
      data-slot="practice-pad"
      className="flex min-h-practice-pad flex-col gap-2 rounded-control bg-fill p-4 text-body text-fg select-text"
    >
      {takes.length === 0 ? (
        <p className="text-fg-tertiary">Your words appear here.</p>
      ) : (
        takes.map((id) => <PadLine key={id} id={id} />)
      )}
    </div>
  );
}
