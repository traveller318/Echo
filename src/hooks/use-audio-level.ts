/**
 * SOURCE OF TRUTH KEYWORDS: useAudioLevel, live input level, level meter, AudioLevel subscription, microphone meter
 * WHAT:  `useAudioLevel(listening)` returns the live input level (0–1 on the decibel scale of lib/audio-level.ts,
 *        lightly smoothed) while `listening`, and 0 otherwise.
 * WHY:   Rust pushes AudioLevel at up to 30 Hz while a take records or a microphone check listens (02 §4.4); a meter
 *        only follows that event and never asks for levels. Subscribing only while listening means an idle window
 *        costs nothing. The value is display state, never a copy of domain state (root CLAUDE.md §7).
 * WHERE: hooks/use-mic-check.ts (onboarding's microphone step, any later microphone test).
 */
import { useState } from "react";
import { levelFromRms, smoothLevel } from "@/lib/audio-level";
import { useEchoEvent } from "./use-echo-event";

export function useAudioLevel(listening: boolean): number {
  const [level, setLevel] = useState(0);
  useEchoEvent(
    "audioLevel",
    ({ rms }) => {
      // A non-finite reading arrives as null: treat it as silence.
      setLevel((previous) => smoothLevel(previous, levelFromRms(rms ?? 0)));
    },
    { enabled: listening },
  );
  return listening ? level : 0;
}
