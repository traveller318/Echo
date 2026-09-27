/**
 * SOURCE OF TRUTH KEYWORDS: Pill, pill window UI, pill morph, pill states, recording pill, compact recording pill, idle pill, movable pill, pill drag, cancel pending pill, copied pill, pill error, model missing Set up
 * WHAT:  The pill window's UI: follows the session (useSessionView) and the pill settings (usePillLook) and renders
 *        its 04 §4 layout — idle (the resting pill, while the user keeps it on screen), recording (full style: logo,
 *        waveform, ✕, stop; compact styles: logo, waveform, stop), cancel pending (draining ring, "Cancelling",
 *        "Undo"), copied, no speech, error (message, "Open") and model missing ("Set up") — inside a glass pill that
 *        springs in, morphs its width between layouts and springs out. ✕ and "Undo" send the Esc input, so the
 *        machine's cancel countdown and undo apply unchanged. Once the take stops the pill leaves (or rests, when it
 *        stays on screen): no "Transcribing" wait and no ✓ for a pasted take, the text landing is the confirmation.
 *        While the pill is movable, a press on its surface (not on a button) asks Rust to drag it.
 * WHY:   Rust owns the session and the settings; the pill keeps no copy of domain state, only display state (audio
 *        levels). The pill is anchored bottom-centre of its window, --space-6 above the window's bottom edge; Rust
 *        places the window (04 §4), including where the user dragged it. Layouts cross-fade while the surface
 *        springs its width (`pillMorph`); under reduced motion every spring becomes the --duration-base fade and
 *        nothing moves or scales (04 §3.7). When the pill has left, the page tells Rust (`pill_exited`) so the window
 *        is hidden only after the animation. Buttons report their rectangles (PillHitAreaRegistry), because the
 *        window lets every other click through; a movable pill reports its whole surface too, so a press on it
 *        reaches the page. The drag itself runs in Rust (the window moves under the cursor without ever being
 *        activated), so the page only starts it. Until the look is known the pill uses the default style and shows
 *        only during takes. Status is never colour alone: each layout has a glyph or words, and the surface names
 *        its state for assistive tech.
 * WHERE: Mounted by src/pill.tsx in the pill window. Layout choice in pill-layout.ts, Rust calls in
 *        pill-commands.ts, pill-only parts in pill/_components, parts shared with Settings in
 *        components/global/pill-face.
 */
import { SquareIcon, XIcon } from "lucide-react";
import { AnimatePresence, motion, useReducedMotion } from "motion/react";
import { useEffect, useState, type PointerEvent } from "react";
import type { PillStyle, SessionView } from "@/bindings";
import { GlassSurface, PILL_STYLES, PillMark, PillRest, Waveform } from "@/components/global";
import { usePillLook } from "@/hooks/use-pill-look";
import { useSessionView } from "@/hooks/use-session-view";
import { describeAppError } from "@/lib/app-error";
import { cn } from "@/lib/cn";
import {
  fadeTransition,
  PILL_ENTER_FROM,
  PILL_EXIT_TO,
  PILL_REST,
  readLengthToken,
  stillPose,
  transitionFor,
} from "@/styles/motion";
import { CountdownRing, PillAction, StatusGlyph } from "./_components";
import { PillHitAreaRegistry, PillHitAreasContext, usePillHitArea } from "./hit-areas";
import {
  cancelTake,
  dragPill,
  openOnboarding,
  performPillAction,
  reportExited,
  reportHitAreas,
  stopTake,
} from "./pill-commands";
import { pillKind, pillWidthToken, type PillKind } from "./pill-layout";

/** The style drawn until Rust's pill settings arrive (the registry default). */
const DEFAULT_STYLE: PillStyle = "full";

/** What the pill says it is doing, for assistive tech. */
const PILL_LABELS: Readonly<Record<PillKind, string>> = {
  idle: "Echo is ready",
  recording: "Recording",
  cancel: "Cancelling",
  copied: "Copied",
  no_speech: "No speech detected",
  error: "Something went wrong",
  model_missing: "Model not installed",
};

const LABEL_CLASS = "truncate text-footnote text-fg";

function StopAction({ className }: { readonly className?: string }) {
  return (
    <PillAction size="icon-sm" label="Stop dictation" className={className} onPress={stopTake}>
      <span className="flex size-(--pill-stop-size) items-center justify-center rounded-pill bg-record text-accent-fg">
        <SquareIcon className="size-2 fill-current" />
      </span>
    </PillAction>
  );
}

interface RecordingContentProps {
  readonly view: SessionView;
  readonly pillStyle: PillStyle;
  readonly reducedMotion: boolean;
}

function RecordingContent({ view, pillStyle, reducedMotion }: RecordingContentProps) {
  const spec = PILL_STYLES[pillStyle];
  const waveform = (
    <div className="flex min-w-0 flex-1 justify-center">
      <Waveform listening={view.status === "recording"} reducedMotion={reducedMotion} tone={spec.tone} />
    </div>
  );
  if (spec.compact) {
    // Logo, waveform and stop only; the stop target reaches the right edge like the full layout's (04 §4).
    return (
      <div className="flex h-full w-full items-center gap-2 pl-2">
        <PillMark pillStyle={pillStyle} />
        {waveform}
        <StopAction />
      </div>
    );
  }
  return (
    <div className="flex h-full w-full items-center gap-2 pl-2">
      <PillMark pillStyle={pillStyle} />
      <span aria-hidden className="h-(--pill-divider-height) w-hairline shrink-0 bg-fg-tertiary" />
      {waveform}
      {/* The buttons' empty target edges take the spacing (glyphs stay apart, targets stay 28px), and the stop
          target reaches the pill's right edge: its circle is then as far from that edge as from the top and bottom. */}
      <div className="-ml-2 flex shrink-0 items-center">
        <PillAction size="icon-sm" label="Cancel dictation" className="text-fg-secondary" onPress={cancelTake}>
          <XIcon />
        </PillAction>
        <StopAction className="-ml-2" />
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
  pillStyle,
  reducedMotion,
}: {
  readonly kind: PillKind;
  readonly view: SessionView;
  readonly pillStyle: PillStyle;
  readonly reducedMotion: boolean;
}) {
  switch (kind) {
    case "idle":
      return <PillRest pillStyle={pillStyle} reducedMotion={reducedMotion} />;
    case "recording":
      return <RecordingContent view={view} pillStyle={pillStyle} reducedMotion={reducedMotion} />;
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

/** A primary-button press on the surface itself (not on one of its buttons) starts a drag. */
function startDrag(event: PointerEvent<HTMLDivElement>): void {
  if (event.button !== 0 || (event.target instanceof Element && event.target.closest("button") !== null)) {
    return;
  }
  event.preventDefault();
  dragPill();
}

interface PillSurfaceProps {
  readonly view: SessionView;
  readonly kind: PillKind;
  readonly pillStyle: PillStyle;
  /** The user may drag the pill: the surface takes clicks and a press drags it. */
  readonly movable: boolean;
  readonly reducedMotion: boolean;
  /** An enter, morph or exit animation finished: buttons are where they will stay. */
  readonly onSettled: () => void;
}

function PillSurface({ view, kind, pillStyle, movable, reducedMotion, onSettled }: PillSurfaceProps) {
  const width = readLengthToken(pillWidthToken(kind, pillStyle));
  const surfaceHitArea = usePillHitArea();
  return (
    <GlassSurface variant="pill" asChild>
      <motion.div
        ref={movable ? surfaceHitArea : undefined}
        data-slot="pill"
        data-kind={kind}
        data-style={pillStyle}
        data-movable={movable}
        role="status"
        aria-label={PILL_LABELS[kind]}
        className={cn("relative h-(--pill-height) overflow-hidden", movable && "cursor-grab active:cursor-grabbing")}
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
        onPointerDown={movable ? startDrag : undefined}
      >
        <AnimatePresence initial={false}>
          <motion.div
            key={`${kind}-${pillStyle}`}
            className="absolute inset-0 flex items-center justify-center"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={fadeTransition()}
          >
            <PillContent kind={kind} view={view} pillStyle={pillStyle} reducedMotion={reducedMotion} />
          </motion.div>
        </AnimatePresence>
      </motion.div>
    </GlassSurface>
  );
}

export function Pill() {
  const view = useSessionView();
  const look = usePillLook();
  const reducedMotion = useReducedMotion() ?? false;
  const kind = pillKind(view, look);
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
              pillStyle={look?.style ?? DEFAULT_STYLE}
              movable={look?.movable ?? false}
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
