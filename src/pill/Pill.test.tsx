/**
 * SOURCE OF TRUTH KEYWORDS: Pill test, pill states test, pill stop button test, pill exit test, pill Set up test, pill hit areas test, idle pill test, pill style test, pill drag test
 * WHAT:  Renders the Pill against pushed SessionStateChanged views and checks each 04 §4 layout, the stop button, the
 *        error and model-missing actions, the hit-area report and the exit report; with the pill settings (PillLook,
 *        read and pushed) the idle layout, the compact and monochrome styles and the drag a movable pill starts.
 * WHY:   The pill is the only UI a user sees while dictating; a wrong layout, a dead button or a window that never
 *        hides would be visible on every take.
 * WHERE: Runs in the `web` Vitest project with the bindings' commands and lib/echo-events mocked, animations skipped
 *        and the real tokens.css values applied.
 */
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { MotionGlobalConfig } from "motion/react";
import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import type { PillLook, SessionView } from "@/bindings";
import { applyDesignTokens, clearDesignTokens } from "@/test/design-tokens";
import { Pill } from "./Pill";

const IDLE: SessionView = {
  status: "idle",
  transcript_id: null,
  elapsed_ms: 0,
  countdown_remaining_ms: null,
  outcome: null,
  error: null,
};

const TAKE = "01J9Z3Q4W5E6R7T8Y9U0I1O2P3";

const DEFAULT_LOOK: PillLook = { visibility: "recording", style: "full", movable: false };

const ok = { status: "ok", data: null } as const;

const mocks = vi.hoisted(() => ({
  handlers: new Map<string, (payload: unknown) => void>(),
  sessionGetState: vi.fn(),
  sessionInput: vi.fn(),
  pillSetHitAreas: vi.fn(),
  pillExited: vi.fn(),
  pillGetLook: vi.fn(),
  pillDrag: vi.fn(),
  appOpenPage: vi.fn(),
  appOpenLogsDir: vi.fn(),
  appOpenMicPrivacySettings: vi.fn(),
  onboardingOpen: vi.fn(),
}));

vi.mock("@/bindings", () => ({
  commands: {
    sessionGetState: mocks.sessionGetState,
    sessionInput: mocks.sessionInput,
    pillSetHitAreas: mocks.pillSetHitAreas,
    pillExited: mocks.pillExited,
    pillGetLook: mocks.pillGetLook,
    pillDrag: mocks.pillDrag,
    appOpenPage: mocks.appOpenPage,
    appOpenLogsDir: mocks.appOpenLogsDir,
    appOpenMicPrivacySettings: mocks.appOpenMicPrivacySettings,
    onboardingOpen: mocks.onboardingOpen,
  },
}));

vi.mock("@/lib/echo-events", () => ({
  subscribeEchoEvent: (name: string, handler: (payload: unknown) => void) => {
    mocks.handlers.set(name, handler);
    return () => {
      mocks.handlers.delete(name);
    };
  },
}));

function push(view: Partial<SessionView>): void {
  const handler = mocks.handlers.get("sessionStateChanged");
  if (handler === undefined) {
    throw new Error("the pill does not follow SessionStateChanged");
  }
  act(() => {
    handler({ ...IDLE, ...view });
  });
}

function pushLook(look: Partial<PillLook>): void {
  const handler = mocks.handlers.get("pillLookChanged");
  if (handler === undefined) {
    throw new Error("the pill does not follow PillLookChanged");
  }
  act(() => {
    handler({ ...DEFAULT_LOOK, ...look });
  });
}

async function renderPill(): Promise<void> {
  render(<Pill />);
  await waitFor(() => {
    expect(mocks.sessionGetState).toHaveBeenCalled();
    expect(mocks.pillGetLook).toHaveBeenCalled();
  });
}

beforeAll(() => {
  MotionGlobalConfig.skipAnimations = true;
});

afterAll(() => {
  MotionGlobalConfig.skipAnimations = false;
});

beforeEach(() => {
  applyDesignTokens();
  mocks.sessionGetState.mockResolvedValue({ status: "ok", data: IDLE });
  mocks.pillGetLook.mockResolvedValue({ status: "ok", data: DEFAULT_LOOK });
  for (const command of [
    mocks.sessionInput,
    mocks.pillSetHitAreas,
    mocks.pillExited,
    mocks.pillDrag,
    mocks.appOpenPage,
    mocks.appOpenLogsDir,
    mocks.appOpenMicPrivacySettings,
    mocks.onboardingOpen,
  ]) {
    command.mockResolvedValue(ok);
  }
});

afterEach(() => {
  clearDesignTokens();
  mocks.handlers.clear();
  vi.clearAllMocks();
});

describe("Pill", () => {
  it("shows nothing while idle", async () => {
    await renderPill();
    expect(screen.queryByRole("status")).toBeNull();
  });

  it("records with the waveform and a stop button that sends the pill's stop", async () => {
    await renderPill();
    push({ status: "arming", transcript_id: TAKE });
    expect(screen.getByRole("status", { name: "Recording" })).toBeInTheDocument();
    push({ status: "recording", transcript_id: TAKE, elapsed_ms: 65_000 });
    expect(document.querySelector("[data-slot='waveform']")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Stop dictation" }));
    await waitFor(() => {
      expect(mocks.sessionInput).toHaveBeenCalledWith("stop");
    });
    // The cancel and stop buttons are the pill's clickable areas.
    expect(mocks.pillSetHitAreas).toHaveBeenCalledWith({
      areas: [expect.objectContaining({ x: 0, y: 0 }), expect.objectContaining({ x: 0, y: 0 })],
    });
  });

  it("leaves as soon as the take stops and never shows Transcribing", async () => {
    await renderPill();
    push({ status: "recording", transcript_id: TAKE });
    push({ status: "finalizing", transcript_id: TAKE, elapsed_ms: 3_000 });
    await waitFor(() => {
      expect(screen.queryByRole("status")).toBeNull();
    });
    push({ status: "delivering", transcript_id: TAKE, elapsed_ms: 3_000 });
    expect(screen.queryByRole("status")).toBeNull();
    expect(screen.queryByText("Transcribing")).toBeNull();
    // A take that could not paste brings the pill back with its result.
    push({ status: "done", transcript_id: TAKE, outcome: "copied" });
    expect(screen.getByRole("status", { name: "Copied" })).toBeInTheDocument();
  });

  it("cancels with ✕ and undoes with Undo, both as Esc", async () => {
    await renderPill();
    push({ status: "recording", transcript_id: TAKE });
    fireEvent.click(screen.getByRole("button", { name: "Cancel dictation" }));
    await waitFor(() => {
      expect(mocks.sessionInput).toHaveBeenCalledWith("cancel");
    });
    push({ status: "cancel_pending", transcript_id: TAKE, countdown_remaining_ms: 3_000 });
    expect(screen.getByRole("status", { name: "Cancelling" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Undo" }));
    await waitFor(() => {
      expect(mocks.sessionInput).toHaveBeenCalledTimes(2);
    });
    expect(mocks.sessionInput).toHaveBeenLastCalledWith("cancel");
  });

  it("leaves without a check after a paste, and shows copy and no-speech results", async () => {
    await renderPill();
    push({ status: "recording", transcript_id: TAKE });
    push({ status: "done", transcript_id: TAKE, outcome: "pasted" });
    await waitFor(() => {
      expect(screen.queryByRole("status")).toBeNull();
    });
    push({ status: "done", transcript_id: TAKE, outcome: "copied" });
    expect(screen.getByText("Copied")).toBeInTheDocument();
    push({ status: "done", transcript_id: TAKE, outcome: "no_speech" });
    expect(screen.getByText("No speech detected")).toBeInTheDocument();
  });

  it("offers Open on an error and Set up when the model is missing", async () => {
    await renderPill();
    push({ status: "failed", transcript_id: TAKE, error: { code: "Asr" } });
    expect(screen.getByText("Couldn't transcribe that take")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Open" }));
    await waitFor(() => {
      expect(mocks.appOpenPage).toHaveBeenCalledWith({ page: "history" });
    });

    push({ status: "failed", error: { code: "Internal" } });
    fireEvent.click(screen.getByRole("button", { name: "Open" }));
    await waitFor(() => {
      expect(mocks.appOpenLogsDir).toHaveBeenCalled();
    });

    push({ status: "failed", error: { code: "ModelMissing", model_id: "parakeet-tdt-0.6b-v3" } });
    expect(screen.getByText("Model not installed")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Set up" }));
    await waitFor(() => {
      expect(mocks.onboardingOpen).toHaveBeenCalled();
    });
  });

  it("leaves when the take is over and tells Rust once it has left", async () => {
    await renderPill();
    push({ status: "recording", transcript_id: TAKE });
    push({ status: "discarded" });
    await waitFor(() => {
      expect(screen.queryByRole("status")).toBeNull();
    });
    await waitFor(() => {
      expect(mocks.pillExited).toHaveBeenCalledTimes(1);
    });
    // Without buttons nothing is clickable.
    expect(mocks.pillSetHitAreas).toHaveBeenLastCalledWith({ areas: [] });
  });

  it("rests in its idle look while kept on screen and follows a new look at once", async () => {
    mocks.pillGetLook.mockResolvedValue({ status: "ok", data: { ...DEFAULT_LOOK, visibility: "always" } });
    await renderPill();
    const idle = await screen.findByRole("status", { name: "Echo is ready" });
    expect(idle).toHaveAttribute("data-kind", "idle");
    expect(idle.querySelector("[data-slot=waveform]")).not.toBeNull();
    push({ status: "recording", transcript_id: TAKE });
    expect(screen.getByRole("status", { name: "Recording" })).toBeInTheDocument();
    push({ status: "idle" });
    expect(screen.getByRole("status", { name: "Echo is ready" })).toBeInTheDocument();
    expect(mocks.pillExited).not.toHaveBeenCalled();

    pushLook({ visibility: "recording" });
    await waitFor(() => {
      expect(screen.queryByRole("status")).toBeNull();
    });
    expect(mocks.pillExited).toHaveBeenCalledOnce();
  });

  it("draws the compact styles with their own logo, and records without ✕", async () => {
    await renderPill();
    pushLook({ visibility: "always", style: "icon" });
    const badge = screen.getByRole("status", { name: "Echo is ready" });
    expect(badge).toHaveAttribute("data-style", "icon");
    expect(badge.querySelector("[data-slot=waveform]")).toBeNull();
    expect(badge.querySelector("img")?.getAttribute("src")).toContain("wave-logo");

    pushLook({ visibility: "always", style: "mono" });
    push({ status: "recording", transcript_id: TAKE });
    const recording = screen.getByRole("status", { name: "Recording" });
    // The resting badge cross-fades out while the recording layout comes in.
    await waitFor(() => {
      expect(recording.querySelectorAll("img")).toHaveLength(1);
    });
    expect(recording.querySelector("img")?.getAttribute("src")).toContain("wave-grey");
    expect(screen.getByRole("button", { name: "Stop dictation" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Cancel dictation" })).toBeNull();
  });

  it("starts a drag from a press on a movable surface, never from its buttons", async () => {
    await renderPill();
    push({ status: "recording", transcript_id: TAKE });
    const surface = screen.getByRole("status", { name: "Recording" });
    fireEvent.pointerDown(surface, { button: 0 });
    expect(mocks.pillDrag).not.toHaveBeenCalled();

    pushLook({ movable: true });
    expect(surface).toHaveAttribute("data-movable", "true");
    fireEvent.pointerDown(screen.getByRole("button", { name: "Stop dictation" }), { button: 0 });
    fireEvent.pointerDown(surface, { button: 2 });
    expect(mocks.pillDrag).not.toHaveBeenCalled();
    fireEvent.pointerDown(surface, { button: 0 });
    await waitFor(() => {
      expect(mocks.pillDrag).toHaveBeenCalledOnce();
    });
  });
});
