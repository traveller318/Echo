/**
 * SOURCE OF TRUTH KEYWORDS: AppRouter test, registry gate test, registry load error test, try again test, titlebar always present
 * WHAT:  Renders AppRouter with Tauri's IPC mocked and verifies: the titlebar is there while the registry loads,
 *        a failed `registry_get` shows its calm copy with "Try again" (and the titlebar), and a successful retry
 *        builds the routed shell from the returned nav.
 * WHY:   Nothing routed can render before the registry exists; if the gate ever rendered nothing on failure, the
 *        frameless window could not even be closed.
 * WHERE: Runs in the `web` Vitest project (jsdom) with @tauri-apps/api/mocks.
 */
import { fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { RegistryView } from "@/bindings";
import { clearTauriMocks, mockTauri } from "@/test/tauri-mocks";
import { Providers } from "./providers";
import { AppRouter } from "./router";

const REGISTRY: RegistryView = {
  settings: [],
  sections: [],
  hotkeys: [],
  nav: [
    { id: "dashboard", label: "Dashboard", icon: "layout-dashboard", route: "/", order: 0 },
    { id: "history", label: "History", icon: "history", route: "/history", order: 1 },
  ],
  engines: [],
  metrics: [],
};

const registryGet = vi.fn<() => Promise<RegistryView>>();

beforeEach(() => {
  window.location.hash = "";
  mockTauri((cmd) => (cmd === "registry_get" ? registryGet() : null));
});

afterEach(async () => {
  await clearTauriMocks();
  vi.clearAllMocks();
});

function renderApp(): void {
  render(
    <Providers>
      <AppRouter />
    </Providers>,
  );
}

describe("AppRouter", () => {
  it("shows the titlebar while the registry loads", () => {
    registryGet.mockReturnValue(new Promise<RegistryView>(() => undefined));
    renderApp();
    expect(screen.getByRole("button", { name: "Close" })).toBeInTheDocument();
    expect(screen.queryByRole("navigation", { name: "Main" })).not.toBeInTheDocument();
  });

  it("shows a failed registry read with its copy and recovers on Try again", async () => {
    registryGet.mockRejectedValueOnce({ code: "Storage" }).mockResolvedValueOnce(REGISTRY);
    renderApp();

    expect(await screen.findByText("Couldn't save to disk")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Minimize" })).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Try again" }));
    expect(await screen.findByRole("navigation", { name: "Main" })).toBeInTheDocument();
    expect(await screen.findByRole("heading", { level: 1, name: "Dashboard" })).toBeInTheDocument();
    expect(registryGet).toHaveBeenCalledTimes(2);
  });
});
