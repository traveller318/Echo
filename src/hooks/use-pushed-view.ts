/**
 * SOURCE OF TRUTH KEYWORDS: usePushedView, pushed view, read once then follow event, full view event, Rust owned view, no QueryClient
 * WHAT:  `usePushedView(event, read, onError)` returns the view Rust last published, or null until the first answer:
 *        one `read` command on mount, then the payload of every `event`.
 * WHY:   Views whose event carries the whole view (SessionStateChanged, PillLookChanged) need no cache and no merge:
 *        the latest one Rust sent is the truth (root CLAUDE.md §7). An event can overtake the initial read (a take
 *        starting while the window mounts), so the read only fills a view no event has set yet. A failed read leaves
 *        the view unknown (null) until the next event instead of guessing. It is plain React state, not TanStack
 *        Query, so the pill's bundle stays minimal (02 §6.2) and it works in a window without a QueryClient. `read`
 *        and `onError` go through useEffectEvent, so a new identity never triggers a second read.
 * WHERE: hooks/use-session-view.ts (useSessionView), hooks/use-pill-look.ts (usePillLook).
 */
import { useEffect, useEffectEvent, useState } from "react";
import type { AppError } from "@/bindings";
import { toAppError } from "@/lib/app-error";
import { runCommand, type CommandResult } from "@/lib/command";
import type { EchoEventName, EchoEventPayload } from "@/lib/echo-events";
import { useEchoEvent } from "./use-echo-event";

export function usePushedView<N extends EchoEventName>(
  event: N,
  read: () => Promise<CommandResult<EchoEventPayload<N>>>,
  onError: (error: AppError) => void,
): EchoEventPayload<N> | null {
  const [view, setView] = useState<EchoEventPayload<N> | null>(null);
  const readView = useEffectEvent(read);
  const reportError = useEffectEvent(onError);

  useEchoEvent(event, setView);

  useEffect(() => {
    let active = true;
    runCommand(readView).then(
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
