/**
 * SOURCE OF TRUTH KEYWORDS: Pill, pill window UI, pill morph, pill states, recording pill, cancel pending pill, processing shimmer, done check, pill error, model missing Set up
 * WHAT:  The pill window's UI: follows the session (useSessionView) and renders its 04 §4 layout — recording (dot,
 *        waveform, timer, stop), cancel pending (draining ring, "Cancelling · Esc to undo"), processing (shimmer,
 *        after --delay-loading), done (✓), copied, no speech, error (message, "Open") and model missing ("Set up") —
 *        inside a glass pill that springs in, morphs its width between layouts and springs out.
 * WHY:   Rust owns the session; the pill keeps no copy of domain state, only display state (a running timer, audio
 *        levels, whether the loading delay has passed). The pill is anchored bottom-centre, --space-6 above the
 *        window's bottom edge, which Rust puts on the taskbar's edge (04 §4). Layouts cross-fade while the surface
 *        springs its width (`pillMorph`); under reduced motion every spring becomes the --duration-base fade and
 *        nothing moves or scales (04 §3.7). When the pill has left, the page tells Rust (`pill_exited`) so the window
 *        is hidden only after the animation. Buttons report their rectangles (PillHitAreaRegistry), because the
 *        window lets every other click through. Status is never colour alone: each layout has a glyph or words, and
 *        the surface names its state for assistive tech.
 * WHERE: Mounted by src/pill.tsx in the pill window. Layout choice in pill-layout.ts, Rust calls in
 *        pill-commands.ts, parts in pill/_components.
 */
import { SquareIcon } from "lucide-react";
import { AnimatePresence, motion, useReducedMotion } from "motion/react";
import { useEffect, useState } from "react";
import type { SessionView } from "@/bindings";
import { GlassSurface } from "@/components/global";
import { useDelayedFlag } from "@/hooks/use-delayed-flag";
import { useRunningClock } from "@/hooks/use-running-clock";
import { useSessionView } from "@/hooks/use-session-view";
import { describeAppError } from "@/lib/app-error";
import { formatClock, NUMERIC_CLASS } from "@/lib/format";
import {
  fadeTransition,
  PILL_ENTER_FROM,
  PILL_EXIT_TO,
  PILL_REST,
  readDurationToken,
  readLengthToken,
  stillPose,
  transitionFor,
} from "@/styles/motion";
import { CountdownRing, PillAction, StatusGlyph, Waveform } from "./_components";
import { PillHitAreaRegistry, PillHitAreasContext } from "./hit-areas";
import { openPage, performPillAction, reportExited, reportHitAreas, stopTake } from "./pill-commands";
import { isFinishing, PILL_WIDTH_TOKENS, pillKind, type PillKind } from "./pill-layout";

const MS_PER_SECOND = 1000;

/** The wait key of a finishing take that has no row (never expected, but the delay must still run). */
const UNKNOWN_TAKE = "take";

/** What the pill says it is doing, for assistive tech. */
const PILL_LABELS: Readonly<Record<PillKind, string>> = {
  recording: "Recording",
  cancel: "Cancelling",
  processing: "Transcribing",
  done: "Pasted",
  copied: "Copied",
  no_speech: "No speech detected",
  error: "Something went wrong",
  model_missing: "Model not installed",
};

const LABEL_CLASS = "truncate text-footnote text-fg";

function RecordingContent({ view, reducedMotion }: { readonly view: SessionView; readonly reducedMotion: boolean }) {
  const recording = view.status === "recording";
  const elapsed = useRunningClock({
    key: view.transcript_id ?? UNKNOWN_TAKE,
    baseMs: view.elapsed_ms,
    running: recording,
  });
  return (
    <div className="flex h-full w-full items-center gap-2 pr-2 pl-3">
      <span aria-hidden className="size-(--record-dot) shrink-0 rounded-pill bg-record motion-safe:animate-breathe" />
      <Waveform listening={recording} reducedMotion={reducedMotion} />
      <span className={`min-w-0 flex-1 text-center text-footnote text-fg ${NUMERIC_CLASS}`}>{formatClock(elapsed)}</span>
      <PillAction size="icon-sm" label="Stop dictation" onPress={stopTake}>
        <SquareIcon className="size-3 fill-current text-record" />
      </PillAction>
    </div>
  );
}

function ErrorContent({ view }: { readonly view: SessionView }) {
  const copy = describeAppError(view.error ?? { code: "Internal" });
  return (
    <div className="flex h-full w-full items-center gap-2 pr-2 pl-4">
      <StatusGlyph status="warning" />
      <span className={`min-w-0 flex-1 ${LABEL_CLASS}`}>{copy.title}</span>
      <PillAction
        className="text-accent"
        onPress={() => {
          performPillAction(copy.action?.id ?? "open_history");
        }}
      >
        Open
      </PillAction>
    </div>
  );
}

function PillContent({
  kind,
  view,
  reducedMotion,
}: {
  readonly kind: PillKind;
  readonly view: SessionView;
  readonly reducedMotion: boolean;
}) {
  switch (kind) {
    case "recording":
      return <RecordingContent view={view} reducedMotion={reducedMotion} />;
    case "cancel":
      return (
        <div className="flex h-full w-full items-center justify-center gap-2 px-4">
          <CountdownRing remainingMs={view.countdown_remaining_ms ?? 0} />
          <span className={LABEL_CLASS}>Cancelling · Esc to undo</span>
        </div>
      );
    case "processing":
      return (
        <span
          className={[
            "text-footnote font-medium text-fg-secondary",
            "motion-safe:bg-linear-to-r motion-safe:from-fg-tertiary motion-safe:via-fg motion-safe:to-fg-tertiary",
            "motion-safe:bg-size-[200%_100%] motion-safe:bg-clip-text motion-safe:text-transparent",
            "motion-safe:animate-shimmer",
          ].join(" ")}
        >
          Transcribing
        </span>
      );
    case "done":
      return <StatusGlyph status="success" label={PILL_LABELS.done} />;
    case "copied":
      return (
        <div className="flex items-center gap-2 px-4">
          <StatusGlyph status="success" />
          <span className={LABEL_CLASS}>Copied</span>
        </div>
      );
    case "no_speech":
      return <span className={`px-4 ${LABEL_CLASS}`}>No speech detected</span>;
    case "error":
      return <ErrorContent view={view} />;
    case "model_missing":
      return (
        <div className="flex h-full w-full items-center gap-2 pr-2 pl-4">
          <span className={`min-w-0 flex-1 ${LABEL_CLASS}`}>Model not installed</span>
          <PillAction
            className="text-accent"
            onPress={() => {
              openPage("models");
            }}
          >
            Set up
          </PillAction>
        </div>
      );
  }
}

interface PillSurfaceProps {
  readonly view: SessionView;
  readonly kind: PillKind;
  readonly reducedMotion: boolean;
  /** An enter, morph or exit animation finished: buttons are where they will stay. */
  readonly onSettled: () => void;
}

function PillSurface({ view, kind, reducedMotion, onSettled }: PillSurfaceProps) {
  const width = readLengthToken(PILL_WIDTH_TOKENS[kind]);
  return (
    <GlassSurface variant="pill" asChild>
      <motion.div
        data-slot="pill"
        data-kind={kind}
        role="status"
        aria-label={PILL_LABELS[kind]}
        className="relative h-(--pill-height) overflow-hidden"
        initial={{ ...stillPose(PILL_ENTER_FROM, reducedMotion), width }}
        animate={{
          ...PILL_REST,
          width,
          transition: {
            default: transitionFor("pillEnter", reducedMotion),
            width: transitionFor("pillMorph", reducedMotion),
          },
        }}
        exit={{ ...stillPose(PILL_EXIT_TO, reducedMotion), transition: transitionFor("pillExit", reducedMotion) }}
        onAnimationComplete={onSettled}
      >
        <AnimatePresence initial={false}>
          <motion.div
            key={kind}
            className="absolute inset-0 flex items-center justify-center"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={fadeTransition()}
          >
            <PillContent kind={kind} view={view} reducedMotion={reducedMotion} />
          </motion.div>
        </AnimatePresence>
      </motion.div>
    </GlassSurface>
  );
}

export function Pill() {
  const view = useSessionView();
  const reducedMotion = useReducedMotion() ?? false;
  const [delayMs] = useState(() => readDurationToken("--delay-loading") * MS_PER_SECOND);
  const processingShown = useDelayedFlag(isFinishing(view) ? (view?.transcript_id ?? UNKNOWN_TAKE) : null, delayMs);
  const kind = pillKind(view, processingShown);
  const [registry] = useState(() => new PillHitAreaRegistry(reportHitAreas));
  useEffect(
    () => () => {
      registry.dispose();
    },
    [registry],
  );

  return (
    <PillHitAreasContext value={registry}>
      <div data-slot="pill-stage" className="fixed inset-0 flex items-end justify-center pb-6 select-none">
        <AnimatePresence onExitComplete={reportExited}>
          {view !== null && kind !== null && (
            <PillSurface
              key="pill"
              view={view}
              kind={kind}
              reducedMotion={reducedMotion}
              onSettled={() => {
                registry.measure();
              }}
            />
          )}
        </AnimatePresence>
      </div>
    </PillHitAreasContext>
  );
}
