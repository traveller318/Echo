/**
 * SOURCE OF TRUTH KEYWORDS: takeAvailability, can copy take, can retry take, can delete take, take action rules
 * WHAT:  Which take actions a take offers now: copy (it may hold text), retry (its audio is kept and the session
 *        is done with it) and delete (the session is done with it).
 * WHY:   Rust enforces every rule (a live take is `Busy`, no audio is `NotFound { recording }`, no text is
 *        `NotFound { transcript_text }`); disabling what cannot work only spares the user an error toast. A take in
 *        `recording`/`transcribing` is either the one in progress or one a crash left behind, which startup
 *        recovery turns into `recoverable`; both wait for that. Row buttons and the detail sheet read one rule.
 * WHERE: TakeRowActions and TranscriptSheet (this folder), wherever takes are listed (History, Dashboard).
 */
import type { TranscriptStatus } from "@/bindings";

export interface TakeAvailability {
  readonly copy: boolean;
  readonly retry: boolean;
  readonly remove: boolean;
}

const IN_PROGRESS: readonly TranscriptStatus[] = ["recording", "transcribing"];

export function takeAvailability(take: { readonly status: TranscriptStatus; readonly has_audio: boolean }): TakeAvailability {
  const settled = !IN_PROGRESS.includes(take.status);
  return {
    copy: settled && take.status !== "empty",
    retry: settled && take.has_audio,
    remove: settled,
  };
}
