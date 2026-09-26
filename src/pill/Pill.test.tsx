/**
 * SOURCE OF TRUTH KEYWORDS: Pill test, pill states test, pill stop button test, pill exit test, pill Set up test, pill hit areas test
 * WHAT:  Renders the Pill against pushed SessionStateChanged views and checks each 04 §4 layout, the stop button, the
 *        error and model-missing actions, the hit-area report and the exit report.
 * WHY:   The pill is the only UI a user sees while dictating; a wrong layout, a dead button or a window that never
 *        hides would be visible on every take.
 * WHERE: Runs in the `web` Vitest project with the bindings' commands and lib/echo-events mocked, animations skipped
 *        and the real tokens.css values applied.
 */
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { MotionGlobalConfig } from "motion/react";
import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import type { SessionView } from "@/bindings";
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

const ok = { status: "ok", data: null } as const;

const mocks = vi.hoisted(() => ({
  handlers: new Map<string, (payload: unknown) => void>(),
  sessionGetState: vi.fn(),
  sessionInput: vi.fn(),
  pillSetHitAreas: vi.fn(),
  pillExited: vi.fn(),
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

async function renderPill(): Promise<void> {
  render(<Pill />);
  await waitFor(() => {
    expect(mocks.sessionGetState).toHaveBeenCalled();
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
  for (const command of [
    mocks.sessionInput,
    mocks.pillSetHitAreas,
    mocks.pillExited,
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

  it("records with a timer and a stop button that sends the pill's stop", async () => {
    await renderPill();
    push({ status: "arming", transcript_id: TAKE });
    expect(screen.getByRole("status", { name: "Recording" })).toBeInTheDocument();
    push({ status: "recording", transcript_id: TAKE, elapsed_ms: 65_000 });
    expect(screen.getByText("1:05")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Stop dictation" }));
    await waitFor(() => {
      expect(mocks.sessionInput).toHaveBeenCalledWith("stop");
    });
    // The stop button is one of the pill's clickable areas.
    expect(mocks.pillSetHitAreas).toHaveBeenCalledWith({ areas: [expect.objectContaining({ x: 0, y: 0 })] });
  });

  it("keeps the recording layout while finishing, then shows Transcribing after the loading delay", async () => {
    await renderPill();
    push({ status: "recording", transcript_id: TAKE });
    push({ status: "finalizing", transcript_id: TAKE, elapsed_ms: 3_000 });
    expect(screen.getByRole("status", { name: "Recording" })).toBeInTheDocument();
    expect(await screen.findByText("Transcribing")).toBeInTheDocument();
  });

  it("shows the Esc countdown, the check, copy and no-speech results", async () => {
    await renderPill();
    push({ status: "cancel_pending", transcript_id: TAKE, countdown_remaining_ms: 3_000 });
    expect(screen.getByText("Cancelling · Esc to undo")).toBeInTheDocument();
    push({ status: "done", transcript_id: TAKE, outcome: "pasted" });
    expect(screen.getByRole("img", { name: "Done" })).toBeInTheDocument();
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
});
