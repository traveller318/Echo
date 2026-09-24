/**
 * SOURCE OF TRUTH KEYWORDS: useSessionView, SessionView, session_get_state, SessionStateChanged, pill state, current take, recording status
 * WHAT:  `useSessionView()` returns the SessionView of the current take as Rust last published it, or null until the
 *        first answer arrives: one `session_get_state` read on mount, then every SessionStateChanged.
 * WHY:   The session actor is the sole owner of recording state (02 §5); each SessionStateChanged carries the full
 *        view, so the hook only ever shows the latest one Rust sent and never derives or merges state of its own.
 *        An event can overtake the initial read (a take starting while the window mounts), so the read only fills
 *        a view no event has set yet. A failed read leaves the view unknown (null) until the next event instead
 *        of guessing Idle. It is plain React state, not TanStack Query, so the pill's bundle stays minimal
 *        (02 §6.2) and it works in a window without a QueryClient. `onError` goes through useEffectEvent, so a new
 *        callback identity never triggers a second read.
 * WHERE: The pill (step 15) and any main-window surface that shows whether a take is running.
 */
import { useEffect, useEffectEvent, useState } from "react";
import { commands, type AppError, type SessionView } from "@/bindings";
import { toAppError } from "@/lib/app-error";
import { runCommand } from "@/lib/command";
import { useEchoEvent } from "./use-echo-event";

function reportToConsole(error: AppError): void {
  console.error("Echo could not read the session state", error);
}

export interface UseSessionViewOptions {
  /** Called when the initial read fails (console by default); the next event still fills the view. */
  readonly onError?: (error: AppError) => void;
}

export function useSessionView({ onError = reportToConsole }: UseSessionViewOptions = {}): SessionView | null {
  const [view, setView] = useState<SessionView | null>(null);
  const reportError = useEffectEvent(onError);

  useEchoEvent("sessionStateChanged", setView);

  useEffect(() => {
    let active = true;
    runCommand(commands.sessionGetState).then(
      (current) => {
        if (active) {
          setView((pushed) => pushed ?? current);
        }
      },
      (error: unknown) => {
        if (active) {
          reportError(toAppError(error));
        }
      },
    );
    return () => {
      active = false;
    };
  }, []);

  return view;
}
