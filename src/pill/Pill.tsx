/**
 * SOURCE OF TRUTH KEYWORDS: Pill, pill window UI, pill morph, pill states, recording pill, cancel pending pill, processing shimmer, done check, pill error, model missing Set up
 * WHAT:  The pill window's UI: follows the session (useSessionView) and renders its 04 §4 layout — recording (logo,
 *        waveform, ✕, stop), cancel pending (draining ring, "Cancelling", "Undo"), processing (shimmer, after
 *        --delay-loading), copied, no speech, error (message, "Open") and model missing ("Set up") —
 *        inside a glass pill that springs in, morphs its width between layouts and springs out. Recording is the
 *        compact layout: Echo's wave logo, a hairline divider, the accent waveform, ✕ and a red stop button.
 *        ✕ and "Undo" send the Esc input, so the machine's cancel countdown and undo apply unchanged; a pasted take
 *        needs no ✓, the pill just leaves.
 * WHY:   Rust owns the session; the pill keeps no copy of domain state, only display state (audio levels,
 *        whether the loading delay has passed). The pill is anchored bottom-centre, --space-6 above the
 *        window's bottom edge, which Rust puts on the taskbar's edge (04 §4). Layouts cross-fade while the surface
 *        springs its width (`pillMorph`); under reduced motion every spring becomes the --duration-base fade and
 *        nothing moves or scales (04 §3.7). When the pill has left, the page tells Rust (`pill_exited`) so the window
 *        is hidden only after the animation. Buttons report their rectangles (PillHitAreaRegistry), because the
 *        window lets every other click through. Status is never colour alone: each layout has a glyph or words, and
 *        the surface names its state for assistive tech. The logo is imported `?no-inline` because the CSP's
 *        img-src is 'self' only: Vite would inline a small image as a data: URI, which the webview would block.
 * WHERE: Mounted by src/pill.tsx in the pill window. Layout choice in pill-layout.ts, Rust calls in
 *        pill-commands.ts, parts in pill/_components.
 */
import { SquareIcon, XIcon } from "lucide-react";
import { AnimatePresence, motion, useReducedMotion } from "motion/react";
import { useEffect, useState } from "react";
import waveLogo from "@/assets/wave-logo.png?no-inline";
import type { SessionView } from "@/bindings";
import { GlassSurface } from "@/components/global";
import { useDelayedFlag } from "@/hooks/use-delayed-flag";
import { useSessionView } from "@/hooks/use-session-view";
import { describeAppError } from "@/lib/app-error";
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
import {
  cancelTake,
  openOnboarding,
  performPillAction,
  reportExited,
  reportHitAreas,
  stopTake,
} from "./pill-commands";
import { isFinishing, PILL_WIDTH_TOKENS, pillKind, type PillKind } from "./pill-layout";

const MS_PER_SECOND = 1000;

/** The wait key of a finishing take that has no row (never expected, but the delay must still run). */
const UNKNOWN_TAKE = "take";

/** What the pill says it is doing, for assistive tech. */
const PILL_LABELS: Readonly<Record<PillKind, string>> = {
  recording: "Recording",
  cancel: "Cancelling",
  processing: "Transcribing",
  copied: "Copied",
  no_speech: "No speech detected",
  error: "Something went wrong",
  model_missing: "Model not installed",
};

const LABEL_CLASS = "truncate text-footnote text-fg";

function RecordingContent({ view, reducedMotion }: { readonly view: SessionView; readonly reducedMotion: boolean }) {
  return (
    <div className="flex h-full w-full items-center gap-2 pl-2">
      <img src={waveLogo} alt="" aria-hidden draggable={false} className="w-(--pill-logo-width) shrink-0" />
      <span aria-hidden className="h-(--pill-divider-height) w-hairline shrink-0 bg-fg-tertiary" />
      <div className="flex min-w-0 flex-1 justify-center">
        <Waveform listening={view.status === "recording"} reducedMotion={reducedMotion} />
      </div>
      {/* The buttons' empty target edges take the spacing (glyphs stay apart, targets stay 28px), and the stop
          target reaches the pill's right edge: its circle is then as far from that edge as from the top and bottom. */}
      <div className="-ml-2 flex shrink-0 items-center">
        <PillAction size="icon-sm" label="Cancel dictation" className="text-fg-secondary" onPress={cancelTake}>
          <XIcon />
        </PillAction>
        <PillAction size="icon-sm" label="Stop dictation" className="-ml-2" onPress={stopTake}>
          <span className="flex size-(--pill-stop-size) items-center justify-center rounded-pill bg-record text-accent-fg">
            <SquareIcon className="size-2 fill-current" />
          </span>
        </PillAction>
      </div>
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
        <div className="flex h-full w-full items-center gap-2 pr-1 pl-3">
          <CountdownRing remainingMs={view.countdown_remaining_ms ?? 0} />
          <span className={`min-w-0 flex-1 ${LABEL_CLASS}`}>Cancelling</span>
          <PillAction className="text-accent" onPress={cancelTake}>
            Undo
          </PillAction>
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
              openOnboarding();
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
