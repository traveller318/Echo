/**
 * SOURCE OF TRUTH KEYWORDS: MicCheckPanel test, microphone test UI test, live level meter test, mic verdict copy test, blocked microphone test
 * WHAT:  Renders MicCheckPanel with Tauri's IPC and events mocked and verifies: the test listens to the given device,
 *        the meter follows AudioLevel while it listens, the verdict is worded when it answers, and a refused check
 *        shows its AppError copy inline.
 * WHY:   The panel is the proof the microphone works before a take depends on it (05 §4); a meter that never moves or a
 *        verdict that never shows would send the user on with a broken microphone.
 * WHERE: Runs in the `web` Vitest project (jsdom) with @tauri-apps/api/mocks (events mocked).
 */
import { QueryClientProvider } from "@tanstack/react-query";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { events, type MicCheck } from "@/bindings";
import { micCheckWindowMs } from "@/hooks/use-mic-check";
import { createEchoQueryClient } from "@/lib/query-client";
import { clearTauriMocks, mockTauri } from "@/test/tauri-mocks";
import { MIC_VERDICT_LOOK } from "./mic-verdict";
import { MicCheckPanel } from "./MicCheckPanel";

function renderPanel(device: string | null) {
  render(
    <QueryClientProvider client={createEchoQueryClient()}>
      <MicCheckPanel device={device} />
    </QueryClientProvider>,
  );
}

afterEach(async () => {
  await clearTauriMocks();
  vi.clearAllMocks();
});

describe("MicCheckPanel", () => {
  it("listens to the device, follows the live level and words the verdict", async () => {
    let answer: (check: MicCheck) => void = () => undefined;
    const ipc = vi.fn((cmd: string) =>
      cmd === "audio_test_level"
        ? new Promise<MicCheck>((resolve) => {
            answer = resolve;
          })
        : null,
    );
    mockTauri(ipc);
    renderPanel("usb-mic");

    fireEvent.click(screen.getByRole("button", { name: "Test microphone" }));
    await waitFor(() => {
      expect(ipc).toHaveBeenCalledWith(
        "audio_test_level",
        expect.objectContaining({ input: { device: "usb-mic", window_ms: micCheckWindowMs() } }),
      );
    });
    expect(screen.getByRole("button", { name: "Listening" })).toBeDisabled();
    const meter = screen.getByRole("progressbar", { name: "Microphone level" });
    expect(meter).toHaveAttribute("aria-valuenow", "0");

    await act(async () => {
      await events.audioLevel.emit({ rms: 0.1 });
    });
    await waitFor(() => {
      expect(Number(meter.getAttribute("aria-valuenow"))).toBeGreaterThan(0);
    });

    await act(async () => {
      answer({ peak_rms: 0.2, mean_rms: 0.1, verdict: "good" });
      await Promise.resolve();
    });
    const good = MIC_VERDICT_LOOK.good;
    expect(await screen.findByText(`${good.title}. ${good.body}`)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Test again" })).toBeEnabled();
  });

  it("shows a refused check inline with its copy", async () => {
    const refused = vi
      .fn<() => Promise<MicCheck>>()
      .mockRejectedValue({ code: "PermissionDenied", permission: "microphone" });
    mockTauri((cmd) => (cmd === "audio_test_level" ? refused() : null));
    renderPanel(null);
    fireEvent.click(screen.getByRole("button", { name: "Test microphone" }));
    expect(await screen.findByText(/^Microphone access is off\./)).toBeInTheDocument();
  });
});
