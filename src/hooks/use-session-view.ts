/**
 * SOURCE OF TRUTH KEYWORDS: useSessionView, SessionView, session_get_state, SessionStateChanged, pill state, current take, recording status
 * WHAT:  `useSessionView()` returns the SessionView of the current take as Rust last published it, or null until the
 *        first answer arrives: one `session_get_state` read on mount, then every SessionStateChanged.
 * WHY:   The session actor is the sole owner of recording state (02 §5); each SessionStateChanged carries the full
 *        view, so the hook only ever shows the latest one Rust sent and never derives or merges state of its own
 *        (usePushedView holds the read/event race rules and stays QueryClient-free for the pill).
 * WHERE: The pill (step 15) and any main-window surface that shows whether a take is running.
 */
import { commands, type AppError, type SessionView } from "@/bindings";
import { usePushedView } from "./use-pushed-view";

function reportToConsole(error: AppError): void {
  console.error("Echo could not read the session state", error);
}

export interface UseSessionViewOptions {
  /** Called when the initial read fails (console by default); the next event still fills the view. */
  readonly onError?: (error: AppError) => void;
}

export function useSessionView({ onError = reportToConsole }: UseSessionViewOptions = {}): SessionView | null {
  return usePushedView("sessionStateChanged", commands.sessionGetState, onError);
}
