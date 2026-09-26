/**
 * SOURCE OF TRUTH KEYWORDS: OnboardingPage test, onboarding flow test, microphone consent test, model step test, practice pad test, practice take test, onboarding complete test
 * WHAT:  Renders onboarding inside the real routed shell (buildAppRoutes on a memory router at ONBOARDING_ROUTE, the
 *        real Providers) with Tauri's IPC and events mocked, and walks it: the dot indicator and the blocked-consent
 *        notice with its privacy link; the model step's card, its import-only actions and a Continue that waits for
 *        the model; the practice step's take rehearsal, its hotkey settings and the text of shown takes in the practice
 *        pad; Finish storing completion and opening the first page; and the "ready" screen when nothing is due.
 * WHY:   Onboarding wires Rust's view, the session rehearsal, events and History together; this is the one place that
 *        whole flow runs short of Rust (02 §13 frontend tests).
 * WHERE: Runs in the `web` Vitest project (jsdom) with @tauri-apps/api/mocks (events mocked).
 */
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { createMemoryRouter } from "react-router";
import { RouterProvider } from "react-router/dom";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  events,
  type ModelEntry,
  type ModelsView,
  type OnboardingStepSpec,
  type OnboardingView,
  type RegistryView,
  type SessionView,
  type Transcript,
} from "@/bindings";
import { Providers } from "@/app/providers";
import { buildAppRoutes } from "@/app/routes";
import { ONBOARDING_ROUTE } from "@/app/screen-pages";
import { RegistryContext } from "@/hooks/use-registry";
import { clearTauriMocks, mockTauri } from "@/test/tauri-mocks";
import OnboardingPage from "./index";

const REGISTRY: RegistryView = {
  settings: [],
  sections: [],
  hotkeys: [],
  nav: [
    { id: "dashboard", label: "Dashboard", icon: "layout-dashboard", route: "/", order: 0 },
    { id: "models", label: "Models", icon: "boxes", route: "/models", order: 2 },
  ],
  engines: [],
  metrics: [],
};

const STEPS: Readonly<Record<OnboardingStepSpec["id"], OnboardingStepSpec>> = {
  microphone: { id: "microphone", label: "Microphone", shown: "first_run", settings: [] },
  model: { id: "model", label: "Speech model", shown: "speech_model_missing", settings: [] },
  practice: { id: "practice", label: "Try it", shown: "always", settings: ["hotkeys.record", "hotkeys.mode"] },
};

function onboarding(overrides: Partial<OnboardingView> = {}): OnboardingView {
  return {
    required: true,
    first_run: true,
    speech_engine: "parakeet-tdt-0.6b-v3",
    speech_model_ready: false,
    microphone: "granted",
    record_hotkey: "Ctrl+Alt",
    hold_to_talk: true,
    steps: [STEPS.microphone, STEPS.model, STEPS.practice],
    ...overrides,
  };
}

const PARAKEET: ModelEntry = {
  engine: {
    id: "parakeet-tdt-0.6b-v3",
    label: "Parakeet TDT 0.6B v3",
    model_id: "parakeet-tdt-0.6b-v3",
    caps: {
      kind: "asr",
      languages: ["en"],
      auto_language: true,
      punctuation: true,
      casing: true,
      accelerators: ["cpu"],
      max_segment_s: 30,
    },
  },
  model: {
    id: "parakeet-tdt-0.6b-v3",
    label: "Parakeet TDT 0.6B v3",
    kind: "model",
    license: "CC-BY-4.0",
    attribution: null,
    revision: "abc",
    files: [],
    archive: null,
    requires: [],
    bundled: false,
  },
  requires: [],
  download_bytes: 1_048_576 * 600,
  status: { kind: "not_installed" },
  selection: { kind: "selectable", active: true },
  runtime: null,
  transfer: null,
};

const MODELS: ModelsView = { entries: [PARAKEET], network: "granted" };

const IDLE: SessionView = {
  status: "idle",
  transcript_id: null,
  elapsed_ms: 0,
  countdown_remaining_ms: null,
  outcome: null,
  error: null,
};

const TAKE = "01J9ZC7Q8X4V7N2D3K5M6P7R8S";

const PRACTICE_TAKE: Transcript = {
  id: TAKE,
  created_at: 0,
  status: "done",
  raw_text: "hello from onboarding",
  final_text: "Hello from onboarding.",
  has_audio: true,
  duration_ms: 1200,
  speech_ms: 900,
  word_count: 3,
  engine_id: "parakeet-tdt-0.6b-v3",
  polisher_ids: ["rules"],
  language: null,
  latency_ms: 120,
  app_name: "echo.exe",
  error_code: null,
};

const ipc = vi.fn<(cmd: string, payload?: unknown) => unknown>();
let view = onboarding();

function renderOnboarding() {
  const router = createMemoryRouter(buildAppRoutes(REGISTRY.nav, undefined, OnboardingPage), {
    initialEntries: [ONBOARDING_ROUTE],
  });
  render(
    <Providers>
      <RegistryContext value={REGISTRY}>
        <RouterProvider router={router} />
      </RegistryContext>
    </Providers>,
  );
  return router;
}

/** The rehearsals sent so far, in order. */
function rehearsals(): unknown[] {
  return ipc.mock.calls
    .filter(([cmd]) => cmd === "session_rehearse")
    .map(([, payload]) => (typeof payload === "object" && payload !== null && "input" in payload ? payload.input : null));
}

beforeEach(() => {
  view = onboarding();
  mockTauri((cmd, payload) => ipc(cmd, payload));
  ipc.mockImplementation((cmd) => {
    switch (cmd) {
      case "onboarding_get":
        return view;
      case "onboarding_complete":
        view = onboarding({ required: false, first_run: false, speech_model_ready: true, steps: [] });
        return view;
      case "models_list":
        return MODELS;
      case "session_get_state":
        return IDLE;
      case "history_get":
        return PRACTICE_TAKE;
      case "settings_get_all":
        return [];
      case "settings_availability":
        return { caps: [], options: [] };
      // The first page Finish opens (the Dashboard) reads these on mount.
      case "history_list":
        return { items: [], next_cursor: null };
      case "metrics_summary":
        return { range: "all_time", values: [] };
      case "metrics_activity":
        return [];
      default:
        return null;
    }
  });
});

afterEach(async () => {
  await clearTauriMocks();
  vi.clearAllMocks();
});

async function continueTo(title: string): Promise<void> {
  fireEvent.click(screen.getByRole("button", { name: "Continue" }));
  expect(await screen.findByRole("heading", { level: 1, name: title })).toBeInTheDocument();
}

describe("OnboardingPage", () => {
  it("walks the microphone step with the consent notice and its privacy link", async () => {
    view = onboarding({ microphone: "denied" });
    renderOnboarding();
    expect(await screen.findByRole("heading", { level: 1, name: "Check your microphone" })).toBeInTheDocument();
    const progress = screen.getByRole("list", { name: "Setup progress" });
    expect(within(progress).getAllByRole("listitem")).toHaveLength(3);
    expect(within(progress).getByText("Step 1 of 3: Microphone, current")).toBeInTheDocument();
    expect(screen.queryByRole("navigation", { name: "Main" })).not.toBeInTheDocument();

    expect(screen.getByText("Microphone access is off")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Open privacy settings" }));
    await waitFor(() => {
      expect(ipc).toHaveBeenCalledWith("app_open_mic_privacy_settings", expect.anything());
    });
    expect(screen.getByRole("button", { name: "Test microphone" })).toBeInTheDocument();
  });

  it("offers the model's download and import, and waits for the model before moving on", async () => {
    renderOnboarding();
    await screen.findByRole("heading", { level: 1, name: "Check your microphone" });
    await continueTo("Get the speech model");

    const card = within(await screen.findByRole("article", { name: "Parakeet TDT 0.6B v3" }));
    expect(card.getByRole("button", { name: "Import from folder" })).toBeInTheDocument();
    expect(card.queryByRole("button", { name: "Remove" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Continue" })).toBeDisabled();
    fireEvent.click(card.getByRole("button", { name: "Download" }));
    await waitFor(() => {
      expect(ipc).toHaveBeenCalledWith(
        "models_download",
        expect.objectContaining({ input: { model_id: PARAKEET.model.id } }),
      );
    });

    view = onboarding({ speech_model_ready: true });
    await act(async () => {
      await events.modelsChanged.emit({});
    });
    await waitFor(() => {
      expect(screen.getByRole("button", { name: "Continue" })).toBeEnabled();
    });
    expect(screen.getByText("The speech model is installed.")).toBeInTheDocument();
  });

  it("moves from the model to the practice step and rehearses a take there", async () => {
    view = onboarding({ steps: [STEPS.model, STEPS.practice], speech_model_ready: true });
    renderOnboarding();
    await screen.findByRole("heading", { level: 1, name: "Get the speech model" });
    await continueTo("Try your hotkey");
    await waitFor(() => {
      expect(rehearsals().at(-1)).toBe("take");
    });
    expect(screen.getByRole("log", { name: "Practice pad" })).toHaveTextContent("Your words appear here.");
    expect(screen.getByText(/Another app may use the same keys/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Back" }));
    await screen.findByRole("heading", { level: 1, name: "Get the speech model" });
    await waitFor(() => {
      expect(rehearsals().at(-1)).toBe("off");
    });
  });

  it("writes the practice take's text into the pad, then finishes and opens the first page", async () => {
    view = onboarding({ steps: [STEPS.practice], first_run: false, speech_model_ready: true });
    const router = renderOnboarding();
    expect(await screen.findByRole("heading", { level: 1, name: "Try your hotkey" })).toBeInTheDocument();

    await act(async () => {
      await events.sessionStateChanged.emit({ ...IDLE, status: "recording", transcript_id: TAKE });
    });
    expect(await screen.findByText("Listening. Let go when you're done.")).toBeInTheDocument();
    await act(async () => {
      await events.sessionStateChanged.emit({ ...IDLE, status: "done", transcript_id: TAKE, outcome: "shown" });
    });
    const pad = screen.getByRole("log", { name: "Practice pad" });
    expect(await within(pad).findByText("Hello from onboarding.")).toBeInTheDocument();

    // A take with no speech explains itself and keeps the text already in the pad.
    await act(async () => {
      await events.sessionStateChanged.emit({ ...IDLE, status: "done", transcript_id: null, outcome: "no_speech" });
    });
    expect(await screen.findByText(/No speech detected/)).toBeInTheDocument();
    expect(within(pad).getByText("Hello from onboarding.")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Finish" }));
    await waitFor(() => {
      expect(router.state.location.pathname).toBe("/");
    });
    expect(ipc).toHaveBeenCalledWith("onboarding_complete", expect.anything());
    expect(await screen.findByText("You're all set")).toBeInTheDocument();
    await waitFor(() => {
      expect(rehearsals().at(-1)).toBe("off");
    });
  });

  it("explains a practice take that went to another app", async () => {
    view = onboarding({ steps: [STEPS.practice], speech_model_ready: true });
    renderOnboarding();
    await screen.findByRole("heading", { level: 1, name: "Try your hotkey" });
    await act(async () => {
      await events.sessionStateChanged.emit({ ...IDLE, status: "done", transcript_id: TAKE, outcome: "pasted" });
    });
    expect(await screen.findByText(/That take went to another app/)).toBeInTheDocument();
  });

  it("says Echo is ready when nothing is due", async () => {
    view = onboarding({ required: false, first_run: false, speech_model_ready: true, steps: [] });
    const router = renderOnboarding();
    expect(await screen.findByRole("heading", { level: 1, name: "Echo is ready" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Open Echo" }));
    await waitFor(() => {
      expect(router.state.location.pathname).toBe("/");
    });
  });
});
