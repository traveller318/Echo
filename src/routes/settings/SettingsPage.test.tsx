/**
 * SOURCE OF TRUTH KEYWORDS: SettingsPage test, settings route test, registry-generated settings test, hidden setting test, caps-gated setting test, settings write test, About test, hotkeys paused notice test, capture lease test
 * WHAT:  Verifies the Settings page against mocked commands: one card per registry section with its heading, only
 *        visible settings whose caps requirement holds, rows showing the effective values, a change sent through
 *        `settings_set`, a reset through `settings_reset`, a refused hotkey shown inline, and the notice for a selected
 *        engine whose model is not ready (grammar polish downloading), the hotkeys-paused notice and its resume, a
 *        hotkey field holding the capture lease while it captures, and About (version, engine, memory, licenses, logs
 *        folder, the Troubleshooting disclosure); plus the pure grouping, notice-copy and About-copy rules.
 * WHY:   The page wires the registry, the settings reads and SettingField together; this is the one place that
 *        whole flow runs short of Rust.
 * WHERE: Runs in the `web` Vitest project with `@/bindings` mocked.
 */
import { QueryClientProvider } from "@tanstack/react-query";
import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type {
  AboutView,
  CapsRequirement,
  HotkeyStatus,
  ModelEntry,
  ModelProgress,
  ModelsView,
  NavItem,
  RegistryView,
  SettingEntry,
  SettingSpec,
  SettingsAvailability,
  SpeechEngineStatus,
} from "@/bindings";
import { RegistryContext } from "@/hooks";
import { createEchoQueryClient } from "@/lib/query-client";
import { acceleratorReasonCopy, engineSummary, modelLicenses, versionLabel } from "./_components/about-copy";
import { setupNoticeCopy } from "./_components/model-setup-copy";
import { settingsBySection } from "./_components/settings-layout";

const NAV: NavItem = { id: "settings", label: "Settings", icon: "settings", route: "/settings", order: 3 };
const MODELS_NAV: NavItem = { id: "models", label: "Models", icon: "boxes", route: "/models", order: 2 };

const MB = 1_048_576;

/** The grammar polish card: Qwen3 and the llama.cpp runtime it requires. */
function grammarModel(overrides: Partial<ModelEntry> = {}): ModelEntry {
  const manifest = {
    kind: "model" as const,
    license: "Apache-2.0",
    attribution: null,
    revision: "abc",
    files: [],
    archive: null,
    requires: [],
    bundled: false,
  };
  return {
    engine: {
      id: "qwen3-1.7b",
      label: "Qwen3 1.7B grammar polish",
      model_id: "qwen3-1.7b-q4-k-m",
      caps: { kind: "polisher", latency_class: "slow", languages: { kind: "any" }, needs_model: true },
    },
    model: { ...manifest, id: "qwen3-1.7b-q4-k-m", label: "Qwen3 1.7B (Q4_K_M)", requires: ["llama-cpp-vulkan"] },
    requires: [{ ...manifest, id: "llama-cpp-vulkan", label: "llama.cpp runtime", kind: "runtime", license: "MIT" }],
    download_bytes: 1_300 * MB,
    status: { kind: "not_installed" },
    selection: { kind: "selectable", active: true },
    runtime: null,
    transfer: null,
    ...overrides,
  };
}

function modelsView(entries: ModelEntry[]): ModelsView {
  return { entries, network: "granted" };
}

function setting(overrides: Partial<SettingSpec> & Pick<SettingSpec, "key" | "section" | "label">): SettingSpec {
  return {
    help: `${overrides.label} help.`,
    kind: { kind: "bool" },
    default: { kind: "bool", value: true },
    restart_required: false,
    visible: true,
    requires: null,
    ...overrides,
  };
}

const SETTINGS: SettingSpec[] = [
  setting({ key: "general.sound_cues", section: "general", label: "Sound cues" }),
  setting({ key: "general.onboarded", section: "general", label: "Onboarding done", visible: false }),
  setting({
    key: "general.debug_log",
    section: "general",
    label: "Detailed logging",
    visible: false,
    default: { kind: "bool", value: false },
  }),
  setting({
    key: "hotkeys.record",
    section: "hotkeys",
    label: "Dictation hotkey",
    kind: { kind: "hotkey" },
    default: { kind: "hotkey", value: "Ctrl+Alt" },
  }),
  setting({ key: "updates.auto_check", section: "updates", label: "Check for updates", requires: "updater_available" }),
];

const REGISTRY: RegistryView = {
  settings: SETTINGS,
  sections: [
    { section: "general", label: "General" },
    { section: "hotkeys", label: "Hotkeys" },
    { section: "updates", label: "Updates" },
  ],
  hotkeys: [],
  nav: [NAV, MODELS_NAV],
  engines: [],
  metrics: [],
};

const VALUES: SettingEntry[] = [
  { key: "general.sound_cues", value: { kind: "bool", value: false } },
  { key: "general.onboarded", value: { kind: "bool", value: false } },
  { key: "general.debug_log", value: { kind: "bool", value: false } },
  { key: "hotkeys.record", value: { kind: "hotkey", value: "Ctrl+Alt" } },
  { key: "updates.auto_check", value: { kind: "bool", value: true } },
];

const AVAILABILITY: SettingsAvailability = { caps: ["hotkey_release", "multiple_languages"], options: [] };

const ABOUT: AboutView = {
  app: { version: "0.1.0", development: false },
  memory: { working_set: 612 * MB, private_bytes: 540 * MB },
};

const ENGINE: SpeechEngineStatus = {
  readiness: { kind: "ready", engine_id: "parakeet-tdt-0.6b-v3", accelerator: "cpu" },
  accelerator: {
    engine_id: "parakeet-tdt-0.6b-v3",
    accelerator: "cpu",
    reason: { kind: "measured" },
    gpu: null,
    bring_up: { load_ms: 900, warm_up_ms: 300 },
    benchmark: null,
  },
};

const RUNNING: HotkeyStatus = { paused: false, capturing: false };

const mocks = vi.hoisted(() => ({
  settingsGetAll: vi.fn(),
  settingsAvailability: vi.fn(),
  settingsSet: vi.fn(),
  settingsReset: vi.fn(),
  audioListDevices: vi.fn(),
  modelsList: vi.fn(),
  appAbout: vi.fn(),
  appOpenLogsDir: vi.fn(),
  engineStatus: vi.fn(),
  engineRemeasure: vi.fn(),
  hotkeysStatus: vi.fn(),
  hotkeysPause: vi.fn(),
  hotkeysCapture: vi.fn(),
}));

const progressEvent = vi.hoisted(() => ({
  handler: null as ((event: { payload: unknown }) => void) | null,
}));

vi.mock("@/bindings", () => ({
  SETTING_TOKEN_MAX_CHARS: 128,
  DEBUG_LOG_SETTING: "general.debug_log",
  commands: mocks,
  // Only live model progress is followed; the page is rendered without the invalidation bridge.
  events: {
    modelProgress: {
      listen: (handler: (event: { payload: unknown }) => void) => {
        progressEvent.handler = handler;
        return Promise.resolve(() => {
          progressEvent.handler = null;
        });
      },
    },
  },
}));

const { default: SettingsPage } = await import("./index");

beforeEach(() => {
  mocks.settingsGetAll.mockResolvedValue({ status: "ok", data: VALUES });
  mocks.settingsAvailability.mockResolvedValue({ status: "ok", data: AVAILABILITY });
  mocks.settingsSet.mockImplementation((input: SettingEntry) => Promise.resolve({ status: "ok", data: input }));
  mocks.settingsReset.mockResolvedValue({ status: "ok", data: VALUES[0] });
  mocks.audioListDevices.mockResolvedValue({ status: "ok", data: [] });
  mocks.modelsList.mockResolvedValue({ status: "ok", data: modelsView([grammarModel({ status: { kind: "installed" } })]) });
  mocks.appAbout.mockResolvedValue({ status: "ok", data: ABOUT });
  mocks.appOpenLogsDir.mockResolvedValue({ status: "ok", data: null });
  mocks.engineStatus.mockResolvedValue({ status: "ok", data: ENGINE });
  mocks.engineRemeasure.mockResolvedValue({ status: "ok", data: null });
  mocks.hotkeysStatus.mockResolvedValue({ status: "ok", data: RUNNING });
  mocks.hotkeysPause.mockResolvedValue({ status: "ok", data: RUNNING });
  mocks.hotkeysCapture.mockResolvedValue({ status: "ok", data: RUNNING });
});

afterEach(() => {
  vi.clearAllMocks();
});

function renderPage() {
  return render(
    <QueryClientProvider client={createEchoQueryClient()}>
      <RegistryContext value={REGISTRY}>
        <MemoryRouter>
          <SettingsPage nav={NAV} />
        </MemoryRouter>
      </RegistryContext>
    </QueryClientProvider>,
  );
}

describe("SettingsPage", () => {
  it("renders a card per section with the settings it may show", async () => {
    renderPage();
    expect(await screen.findByRole("region", { name: "General" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { level: 1, name: "Settings" })).toBeInTheDocument();
    expect(screen.getAllByRole("heading", { level: 2 }).map((heading) => heading.textContent)).toEqual([
      "General",
      "Hotkeys",
      "About",
    ]);
    expect(screen.getByRole("switch", { name: "Sound cues" })).not.toBeChecked();
    expect(screen.queryByText("Onboarding done")).not.toBeInTheDocument();
    expect(screen.queryByText("Detailed logging"), "hidden until Troubleshooting opens").not.toBeInTheDocument();
    expect(screen.queryByText("Check for updates")).not.toBeInTheDocument();
  });

  it("sends a change and a reset through the settings commands", async () => {
    renderPage();
    const cues = await screen.findByRole("switch", { name: "Sound cues" });
    act(() => {
      fireEvent.click(cues);
    });
    await vi.waitFor(() => {
      expect(mocks.settingsSet).toHaveBeenCalledWith({ key: "general.sound_cues", value: { kind: "bool", value: true } });
    });
    act(() => {
      fireEvent.click(screen.getByRole("button", { name: "Reset Sound cues to default" }));
    });
    await vi.waitFor(() => {
      expect(mocks.settingsReset).toHaveBeenCalledWith({ key: "general.sound_cues" });
    });
  });

  it("shows a refused hotkey inline and keeps the combination in effect", async () => {
    mocks.settingsSet.mockResolvedValue({ status: "error", error: { code: "Hotkey", reason: "conflict" } });
    renderPage();
    const hotkey = await screen.findByRole("button", { name: "Dictation hotkey" });
    act(() => {
      hotkey.focus();
      fireEvent.click(hotkey);
    });
    fireEvent.keyDown(hotkey, { code: "KeyV", ctrlKey: true, altKey: true });
    const row = hotkey.closest<HTMLElement>("[data-setting='hotkeys.record']");
    if (row === null) {
      throw new Error("no hotkey row");
    }
    expect(await within(row).findByRole("alert")).toHaveTextContent("That shortcut is taken.");
    await vi.waitFor(() => {
      expect(Array.from(hotkey.querySelectorAll("kbd"), (key) => key.textContent)).toEqual(["Ctrl", "Alt"]);
    });
    expect(hotkey).toHaveAttribute("aria-invalid", "true");
  });

  it("switches Echo's hotkeys off while a hotkey field captures and back on when it stops", async () => {
    renderPage();
    const hotkey = await screen.findByRole("button", { name: "Dictation hotkey" });
    act(() => {
      hotkey.focus();
      fireEvent.click(hotkey);
    });
    await vi.waitFor(() => {
      expect(mocks.hotkeysCapture).toHaveBeenCalledWith({ active: true });
    });
    fireEvent.keyDown(hotkey, { code: "Escape" });
    await vi.waitFor(() => {
      expect(mocks.hotkeysCapture).toHaveBeenLastCalledWith({ active: false });
    });
    expect(mocks.hotkeysCapture).toHaveBeenCalledTimes(2);
  });

  it("says when the hotkeys are paused and resumes them", async () => {
    mocks.hotkeysStatus.mockResolvedValue({ status: "ok", data: { paused: true, capturing: false } });
    renderPage();
    expect(await screen.findByText("Hotkeys are paused")).toBeInTheDocument();
    act(() => {
      fireEvent.click(screen.getByRole("button", { name: "Resume hotkeys" }));
    });
    await vi.waitFor(() => {
      expect(mocks.hotkeysPause).toHaveBeenCalledWith({ paused: false });
    });
  });

  it("shows About with the version, engine, memory and licenses, and opens the logs folder", async () => {
    mocks.modelsList.mockResolvedValue({
      status: "ok",
      data: modelsView([
        grammarModel({
          status: { kind: "installed" },
          model: { ...grammarModel().model, attribution: "Qwen3 by Alibaba Cloud." },
        }),
      ]),
    });
    renderPage();
    const about = await screen.findByRole("region", { name: "About" });
    expect(await within(about).findByText("0.1.0")).toBeInTheDocument();
    expect(await within(about).findByText("612 MB")).toBeInTheDocument();
    expect(within(about).getByText("Processor (CPU)")).toBeInTheDocument();
    expect(within(about).getByText("Measured as the faster choice on this PC.")).toBeInTheDocument();
    expect(within(about).getByText("Qwen3 by Alibaba Cloud.")).toBeInTheDocument();
    expect(within(about).getByText(/llama\.cpp runtime/)).toBeInTheDocument();
    act(() => {
      fireEvent.click(within(about).getByRole("button", { name: "Open logs folder" }));
    });
    await vi.waitFor(() => {
      expect(mocks.appOpenLogsDir).toHaveBeenCalled();
    });
    const troubleshooting = within(about).getByRole("button", { name: "Troubleshooting" });
    expect(troubleshooting).toHaveAttribute("aria-expanded", "false");
    act(() => {
      fireEvent.click(troubleshooting);
    });
    expect(troubleshooting).toHaveAttribute("aria-expanded", "true");
    const detailed = await within(about).findByRole("switch", { name: "Detailed logging" });
    act(() => {
      fireEvent.click(detailed);
    });
    await vi.waitFor(() => {
      expect(mocks.settingsSet).toHaveBeenCalledWith({ key: "general.debug_log", value: { kind: "bool", value: true } });
    });
    expect(within(about).queryByRole("button", { name: "Measure again" }), "no GPU, nothing to measure").toBeNull();
  });

  it("offers an explanation and a retry when the settings cannot be read", async () => {
    mocks.settingsGetAll.mockResolvedValue({ status: "error", error: { code: "Storage" } });
    renderPage();
    expect(await screen.findByText("Couldn't save to disk")).toBeInTheDocument();
    mocks.settingsGetAll.mockResolvedValue({ status: "ok", data: VALUES });
    act(() => {
      fireEvent.click(screen.getByRole("button", { name: "Try again" }));
    });
    expect(await screen.findByRole("switch", { name: "Sound cues" })).toBeInTheDocument();
  });
});

describe("settingsBySection", () => {
  it("keeps registry order, drops hidden and unavailable settings and empty sections", () => {
    const shown = (caps: CapsRequirement[]) =>
      settingsBySection(REGISTRY.sections, SETTINGS, caps).map((group) => [
        group.section.section,
        group.settings.map((spec) => spec.key),
      ]);
    expect(shown([])).toEqual([
      ["general", ["general.sound_cues"]],
      ["hotkeys", ["hotkeys.record"]],
    ]);
    expect(shown(["updater_available"])).toEqual([
      ["general", ["general.sound_cues"]],
      ["hotkeys", ["hotkeys.record"]],
      ["updates", ["updates.auto_check"]],
    ]);
  });
});

describe("model setup notices", () => {
  it("say when grammar polish is on but its model is not installed, and follow a download live", async () => {
    mocks.modelsList.mockResolvedValue({ status: "ok", data: modelsView([grammarModel()]) });
    renderPage();
    expect(await screen.findByText("Qwen3 1.7B grammar polish: Not installed")).toBeInTheDocument();
    expect(screen.getByText("Download it on the Models page to use it.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Open Models" })).toBeInTheDocument();
    await vi.waitFor(() => {
      expect(progressEvent.handler).not.toBeNull();
    });
    const progress: ModelProgress = {
      model_id: "qwen3-1.7b-q4-k-m",
      bytes: 312 * MB,
      total: 1_300 * MB,
      phase: "transferring",
    };
    act(() => {
      progressEvent.handler?.({ payload: progress });
    });
    expect(await screen.findByText("Qwen3 1.7B grammar polish: Downloading")).toBeInTheDocument();
  });

  it("stay away while every selected model is installed", async () => {
    renderPage();
    expect(await screen.findByRole("region", { name: "General" })).toBeInTheDocument();
    await vi.waitFor(() => {
      expect(mocks.modelsList).toHaveBeenCalled();
    });
    expect(screen.queryByRole("button", { name: "Open Models" })).not.toBeInTheDocument();
  });
});

describe("setupNoticeCopy", () => {
  it("speaks only for a selected engine that is not ready", () => {
    expect(setupNoticeCopy(grammarModel({ selection: { kind: "selectable", active: false } }), null)).toBeNull();
    expect(setupNoticeCopy(grammarModel({ selection: { kind: "built_in" } }), null)).toBeNull();
    expect(setupNoticeCopy(grammarModel({ status: { kind: "installed" } }), null)).toBeNull();
    expect(setupNoticeCopy(grammarModel(), null)).toBe("Download it on the Models page to use it.");
    expect(setupNoticeCopy(grammarModel({ status: { kind: "partial", bytes: MB } }), null)).toBe(
      "The download stopped. Resume it on the Models page.",
    );
    expect(setupNoticeCopy(grammarModel({ status: { kind: "corrupt" } }), null)).toBe(
      "Its files are damaged. Download it again on the Models page.",
    );
    const waiting: ModelProgress = { model_id: "qwen3-1.7b-q4-k-m", bytes: MB, total: 2 * MB, phase: "waiting" };
    expect(setupNoticeCopy(grammarModel(), waiting)).toMatch(/resuming shortly/);
  });
});

describe("About copy", () => {
  it("names the build, the engine's state and every license once", () => {
    expect(versionLabel({ version: "0.1.0", development: false })).toBe("0.1.0");
    expect(versionLabel({ version: "0.1.0", development: true })).toBe("0.1.0 (development build)");
    const engines = [
      {
        id: "parakeet-tdt-0.6b-v3",
        label: "Parakeet TDT 0.6B v3",
        model_id: "parakeet-tdt-0.6b-v3",
        caps: {
          kind: "asr" as const,
          languages: ["en"],
          auto_language: true,
          punctuation: true,
          casing: true,
          accelerators: ["cpu" as const],
          max_segment_s: 30,
        },
      },
    ];
    expect(engineSummary(ENGINE, engines)).toBe("Parakeet TDT 0.6B v3");
    expect(engineSummary({ readiness: { kind: "unloaded" }, accelerator: null }, engines)).toBe("Not loaded yet");
    expect(
      engineSummary({ readiness: { kind: "loading", engine_id: "parakeet-tdt-0.6b-v3" }, accelerator: null }, engines),
    ).toBe("Parakeet TDT 0.6B v3 (loading)");
    expect(
      engineSummary(
        {
          readiness: {
            kind: "failed",
            engine_id: "parakeet-tdt-0.6b-v3",
            error: { code: "ModelMissing", model_id: "parakeet-tdt-0.6b-v3" },
          },
          accelerator: null,
        },
        engines,
      ),
    ).toMatch(/^Parakeet TDT 0\.6B v3: /);
    expect(acceleratorReasonCopy({ kind: "gpu_lost" })).toMatch(/processor took over/);
    const twice = modelsView([grammarModel(), grammarModel()]);
    expect(modelLicenses(twice).map((manifest) => manifest.id)).toEqual(["qwen3-1.7b-q4-k-m", "llama-cpp-vulkan"]);
  });
});
