/**
 * SOURCE OF TRUTH KEYWORDS: useSessionRehearsal, session_rehearse, rehearsal while mounted, hotkey test, practice take, ordered rehearsal calls
 * WHAT:  `useSessionRehearsal(rehearsal)` sets what the session rehearses while the calling component is mounted
 *        (`hotkey`: presses are reported as HotkeyRehearsed, `take`: a take's text stays in Echo) and turns it off
 *        when the component unmounts or asks for another one.
 * WHY:   The rehearsal lives in the session actor (pipeline/session/rehearsal.rs); a step only declares the one it
 *        needs. Every call goes through one ordered queue (lib/command-queue.ts), because Tauri may run two quick
 *        invokes concurrently and a StrictMode remount (on, off, on) must never end with "off" landing last. A failed
 *        call is logged, never thrown: Rust applies a rehearsal only while an Echo window has focus, so a lost "off"
 *        cannot break dictation elsewhere.
 * WHERE: routes/onboarding (the hotkey and practice steps).
 */
import { useEffect } from "react";
import { commands, type SessionRehearsal } from "@/bindings";
import { createCommandQueue } from "@/lib/command-queue";

const send = createCommandQueue("set the session rehearsal");

export function useSessionRehearsal(rehearsal: SessionRehearsal): void {
  useEffect(() => {
    send(() => commands.sessionRehearse(rehearsal));
    return () => {
      send(() => commands.sessionRehearse("off"));
    };
  }, [rehearsal]);
}
