/**
 * SOURCE OF TRUTH KEYWORDS: useMicCheck, microphone check, audio_test_level, mic test, live meter, MicVerdict, micCheckWindowMs
 * WHAT:  `useMicCheck()` runs `audio_test_level` on a device (null = the Windows default) and returns the live level
 *        while it listens, the last check's result (peak, mean, verdict) and why it failed.
 * WHY:   Onboarding (and any later microphone test) must prove the microphone works before a take depends on it
 *        (05 §4). Rust listens and judges (pipeline/capture/mic_check.rs); the UI only draws the AudioLevel events the
 *        check sends meanwhile and its verdict. A failure is shown inline where the test runs (a blocked privacy
 *        consent, a busy microphone), so it does not toast. The window sits inside Rust's own bounds, exported as
 *        constants, so the two never disagree.
 * WHERE: routes/onboarding (the microphone step).
 */
import {
  commands,
  MIC_CHECK_MAX_WINDOW_MS,
  MIC_CHECK_MIN_WINDOW_MS,
  type AppError,
  type AudioDeviceId,
  type MicCheck,
} from "@/bindings";
import { toAppError } from "@/lib/app-error";
import { useAudioLevel } from "./use-audio-level";
import { useEchoMutation } from "./use-echo-mutation";

/** How long a check would like to listen: long enough to say a short sentence. */
const PREFERRED_WINDOW_MS = 4_000;

/** How long a check listens, inside Rust's bounds (read when a check starts, so the module loads without them). */
export function micCheckWindowMs(): number {
  return Math.min(MIC_CHECK_MAX_WINDOW_MS, Math.max(MIC_CHECK_MIN_WINDOW_MS, PREFERRED_WINDOW_MS));
}

export interface MicCheckRun {
  /** Listens to `device` (null: the Windows default). */
  readonly run: (device: AudioDeviceId | null) => void;
  /** The check is listening now. */
  readonly running: boolean;
  /** The live level (0–1) while listening. */
  readonly level: number;
  /** The last finished check; null before the first one and while one runs. */
  readonly result: MicCheck | null;
  /** Why the last check failed; null otherwise. */
  readonly error: AppError | null;
}

export function useMicCheck(): MicCheckRun {
  const check = useEchoMutation(commands.audioTestLevel, { toastOnError: false });
  const level = useAudioLevel(check.isPending);
  return {
    run: (device) => {
      check.mutate({ device, window_ms: micCheckWindowMs() });
    },
    running: check.isPending,
    level,
    result: check.isPending ? null : (check.data ?? null),
    error: check.error === null ? null : toAppError(check.error),
  };
}
