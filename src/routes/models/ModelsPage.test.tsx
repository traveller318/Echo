/**
 * SOURCE OF TRUTH KEYWORDS: ModelsPage test, models route test, download button test, model progress event test, offline notice test, remove model test, modelActionPlan table
 * WHAT:  Verifies the Models page against mocked commands and a mocked ModelProgress event: a card per entry with its
 *        badge, facts and actions; Download, Cancel, Import and Remove reaching their commands (Remove only after the
 *        confirmation); live progress from the event; the offline notice with downloads disabled; plus the pure
 *        action plan as a table.
 * WHY:   The page wires `models_list`, the progress event, the actions and ModelCard together; this is the one place
 *        that flow runs short of Rust.
 * WHERE: Runs in the `web` Vitest project with `@/bindings` mocked.
 */
import { QueryClientProvider } from "@tanstack/react-query";
import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type {
  EngineSpec,
  ModelEntry,
  ModelManifest,
  ModelProgress,
  ModelsView,
  NavItem,
  RegistryView,
} from "@/bindings";
import { RegistryContext } from "@/hooks";
import { createEchoQueryClient } from "@/lib/query-client";
import { modelActionPlan } from "./_components/model-actions";

const NAV: NavItem = { id: "models", label: "Models", icon: "boxes", route: "/models", order: 2 };

const REGISTRY: RegistryView = {
  settings: [],
  sections: [],
  hotkeys: [],
  nav: [NAV, { id: "settings", label: "Settings", icon: "settings", route: "/settings", order: 3 }],
  engines: [],
  metrics: [],
};

const PARAKEET_ENGINE: EngineSpec = {
  id: "parakeet-tdt-0.6b-v3",
  label: "Parakeet TDT 0.6B v3",
  model_id: "parakeet-tdt-0.6b-v3",
  caps: {
    kind: "asr",
    languages: ["en", "de", "fr", "es"],
    auto_language: true,
    punctuation: true,
    casing: true,
    accelerators: ["cpu"],
    max_segment_s: 30,
  },
};

const PARAKEET_MODEL: ModelManifest = {
  id: "parakeet-tdt-0.6b-v3",
  label: "Parakeet TDT 0.6B v3",
  kind: "model",
  license: "CC-BY-4.0",
  attribution: "NVIDIA Parakeet TDT 0.6B v3 by NVIDIA (CC BY 4.0)",
  revision: "abc",
  files: [{ name: "encoder.onnx", url: "https://huggingface.co/x/encoder.onnx", sha256: "0".repeat(64), bytes: 1_048_576 * 600 }],
  archive: null,
  requires: [],
  bundled: false,
};

const SILERO: ModelEntry = {
  engine: { id: "silero-vad-v5", label: "Silero VAD", model_id: "silero-vad-v5", caps: { kind: "vad", frame_ms: 32 } },
  model: { ...PARAKEET_MODEL, id: "silero-vad-v5", label: "Silero VAD v5", license: "MIT", attribution: null, bundled: true },
  requires: [],
  download_bytes: 1_048_576 * 600,
  status: { kind: "installed" },
  selection: { kind: "built_in" },
  runtime: null,
  transfer: null,
};

function parakeet(overrides: Partial<ModelEntry> = {}): ModelEntry {
  return {
    engine: PARAKEET_ENGINE,
    model: PARAKEET_MODEL,
    requires: [],
    download_bytes: 1_048_576 * 600,
    status: { kind: "not_installed" },
    selection: { kind: "selectable", active: true },
    runtime: null,
    transfer: null,
    ...overrides,
  };
}

function view(entry: ModelEntry, network: ModelsView["network"] = "granted"): ModelsView {
  return { entries: [entry, SILERO], network };
}

const mocks = vi.hoisted(() => ({
  modelsList: vi.fn(),
  modelsDownload: vi.fn(),
  modelsCancelDownload: vi.fn(),
  modelsImport: vi.fn(),
  modelsVerify: vi.fn(),
  modelsRemove: vi.fn(),
  modelsSetActive: vi.fn(),
}));

const progressEvent = vi.hoisted(() => ({
  handler: null as ((event: { payload: unknown }) => void) | null,
}));

vi.mock("@/bindings", () => ({
  commands: mocks,
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

const { default: ModelsPage } = await import("./index");

beforeEach(() => {
  for (const command of [
    mocks.modelsDownload,
    mocks.modelsImport,
    mocks.modelsVerify,
  ]) {
    command.mockResolvedValue({ status: "ok", data: "completed" });
  }
  mocks.modelsCancelDownload.mockResolvedValue({ status: "ok", data: null });
  mocks.modelsRemove.mockResolvedValue({ status: "ok", data: null });
  mocks.modelsSetActive.mockResolvedValue({ status: "ok", data: null });
});

afterEach(() => {
  vi.clearAllMocks();
});

function renderPage(data: ModelsView) {
  mocks.modelsList.mockResolvedValue({ status: "ok", data });
  return render(
    <QueryClientProvider client={createEchoQueryClient()}>
      <RegistryContext value={REGISTRY}>
        <MemoryRouter>
          <ModelsPage nav={NAV} />
        </MemoryRouter>
      </RegistryContext>
    </QueryClientProvider>,
  );
}

async function card(name: string) {
  return within(await screen.findByRole("article", { name }));
}

describe("ModelsPage", () => {
  it("shows a card per engine with its facts, status and download action", async () => {
    renderPage(view(parakeet()));
    const speech = await card("Parakeet TDT 0.6B v3");
    expect(speech.getByText("Not installed")).toBeInTheDocument();
    expect(speech.getByText("Speech recognition")).toBeInTheDocument();
    expect(speech.getByText("CC-BY-4.0")).toBeInTheDocument();
    expect(speech.getByText("600 MB")).toBeInTheDocument();
    expect(speech.getByText(/and 1 more$/)).toBeInTheDocument();
    const vad = await card("Silero VAD");
    expect(vad.getByText("Built in")).toBeInTheDocument();
    expect(vad.queryByRole("button")).not.toBeInTheDocument();

    act(() => {
      fireEvent.click(speech.getByRole("button", { name: "Download" }));
    });
    await vi.waitFor(() => {
      expect(mocks.modelsDownload).toHaveBeenCalledWith({ model_id: "parakeet-tdt-0.6b-v3" });
    });
    act(() => {
      fireEvent.click(speech.getByRole("button", { name: "Import from folder" }));
    });
    await vi.waitFor(() => {
      expect(mocks.modelsImport).toHaveBeenCalledWith({ model_id: "parakeet-tdt-0.6b-v3" });
    });
  });

  it("sizes a model with its runtime as one download and credits both", async () => {
    const runtime: ModelManifest = {
      ...PARAKEET_MODEL,
      id: "llama-cpp-vulkan",
      label: "llama.cpp runtime b11146 (Vulkan)",
      kind: "runtime",
      license: "MIT",
      attribution: "llama.cpp by the ggml authors (MIT License)",
    };
    const grammar = parakeet({
      engine: {
        id: "qwen3-1.7b",
        label: "Qwen3 1.7B grammar polish",
        model_id: "qwen3-1.7b-q4-k-m",
        caps: { kind: "polisher", latency_class: "slow", languages: { kind: "any" }, needs_model: true },
      },
      model: {
        ...PARAKEET_MODEL,
        id: "qwen3-1.7b-q4-k-m",
        label: "Qwen3 1.7B (Q4_K_M)",
        license: "Apache-2.0",
        attribution: "Qwen3-1.7B by the Qwen team",
        requires: ["llama-cpp-vulkan"],
      },
      requires: [runtime],
      download_bytes: 1_048_576 * 1_250,
      selection: { kind: "selectable", active: false },
    });
    renderPage(view(grammar));
    const polish = await card("Qwen3 1.7B grammar polish");
    expect(polish.getByText("Text cleanup")).toBeInTheDocument();
    expect(polish.getByText("1.2 GB")).toBeInTheDocument();
    expect(polish.getByText("llama.cpp runtime b11146 (Vulkan)")).toBeInTheDocument();
    expect(polish.getByText("Qwen3-1.7B by the Qwen team")).toBeInTheDocument();
    expect(polish.getByText("llama.cpp by the ggml authors (MIT License)")).toBeInTheDocument();
    expect(polish.getByText("Any language")).toBeInTheDocument();
  });

  it("follows live progress and cancels a running download", async () => {
    renderPage(view(parakeet({ status: { kind: "partial", bytes: 0 } })));
    const speech = await card("Parakeet TDT 0.6B v3");
    await vi.waitFor(() => {
      expect(progressEvent.handler).not.toBeNull();
    });
    const progress: ModelProgress = {
      model_id: "parakeet-tdt-0.6b-v3",
      bytes: 1_048_576 * 300,
      total: 1_048_576 * 600,
      phase: "transferring",
    };
    act(() => {
      progressEvent.handler?.({ payload: progress });
    });
    expect(speech.getByText("300 MB of 600 MB")).toBeInTheDocument();
    expect(speech.getByRole("progressbar")).toBeInTheDocument();
    expect(speech.queryByRole("button", { name: "Import from folder" })).not.toBeInTheDocument();
    act(() => {
      fireEvent.click(speech.getByRole("button", { name: "Cancel" }));
    });
    await vi.waitFor(() => {
      expect(mocks.modelsCancelDownload).toHaveBeenCalledWith({ model_id: "parakeet-tdt-0.6b-v3" });
    });
    act(() => {
      progressEvent.handler?.({ payload: { ...progress, phase: "cancelled" } });
    });
    expect(speech.queryByRole("progressbar")).not.toBeInTheDocument();
    expect(speech.getByRole("button", { name: "Resume download" })).toBeInTheDocument();
  });

  it("explains offline mode and disables downloads but not import", async () => {
    renderPage(view(parakeet(), "denied"));
    expect(await screen.findByText("Offline mode is on")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Open Settings" })).toBeInTheDocument();
    const speech = await card("Parakeet TDT 0.6B v3");
    expect(speech.getByRole("button", { name: "Download" })).toBeDisabled();
    expect(speech.getByRole("button", { name: "Import from folder" })).toBeEnabled();
  });

  it("removes an installed model only after the confirmation", async () => {
    renderPage(view(parakeet({ status: { kind: "installed" }, runtime: { kind: "ready", accelerator: "cpu" } })));
    const speech = await card("Parakeet TDT 0.6B v3");
    expect(speech.getByText("In use")).toBeInTheDocument();
    expect(speech.getByText("Processor (CPU)")).toBeInTheDocument();
    act(() => {
      fireEvent.click(speech.getByRole("button", { name: "Remove" }));
    });
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText(/Dictation uses this model/)).toBeInTheDocument();
    expect(mocks.modelsRemove).not.toHaveBeenCalled();
    act(() => {
      fireEvent.click(within(dialog).getByRole("button", { name: "Remove" }));
    });
    await vi.waitFor(() => {
      expect(mocks.modelsRemove).toHaveBeenCalledWith({ model_id: "parakeet-tdt-0.6b-v3" });
    });
  });
});

describe("modelActionPlan", () => {
  const running: ModelProgress = { model_id: "m", bytes: 1, total: 2, phase: "transferring" };

  it.each([
    ["not installed", parakeet(), null, "download", ["import"]],
    ["partial", parakeet({ status: { kind: "partial", bytes: 5 } }), null, "resume", ["import", "remove"]],
    ["damaged", parakeet({ status: { kind: "corrupt" } }), null, "redownload", ["import", "remove"]],
    ["installed and in use", parakeet({ status: { kind: "installed" } }), null, null, ["verify", "remove"]],
    [
      "installed, not in use",
      parakeet({ status: { kind: "installed" }, selection: { kind: "selectable", active: false } }),
      null,
      "use",
      ["verify", "remove"],
    ],
    ["transferring", parakeet(), running, "cancel", []],
    ["bundled", SILERO, null, null, []],
  ] as const)("%s", (_name, entry, transfer, primary, secondary) => {
    expect(modelActionPlan(entry, transfer)).toEqual({ primary, secondary });
  });
});
