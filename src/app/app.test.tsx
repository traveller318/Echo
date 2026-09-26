/**
 * SOURCE OF TRUTH KEYWORDS: app shell test, sidebar navigation test, route fallback test, titlebar window controls test, toast action test, route error test, onboarding gate test, sidebar toggle test
 * WHAT:  Renders the routed shell (buildAppRoutes on a memory router, the real lazy pages, the real Providers) with
 *        Tauri's IPC mocked, and verifies: registry-ordered sidebar with the current page marked, navigation,
 *        the catch-all redirect, the titlebar's minimize and close calls, the sidebar toggle, AppError toast actions (page and
 *        command), that a failing page keeps the sidebar and offers a reload, and that onboarding opens at launch when
 *        Rust says it is due and whenever OnboardingRequested arrives.
 * WHY:   The shell is the frame every later page lives in (04 §5); these are the behaviours a user notices first
 *        and the ones a registry or router change would silently break.
 * WHERE: Runs in the `web` Vitest project (jsdom) with Tauri's IPC and events mocked (test/tauri-mocks.ts).
 */
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { createMemoryRouter } from "react-router";
import { RouterProvider } from "react-router/dom";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { events, type NavItem, type OnboardingView, type RegistryView, type SessionView } from "@/bindings";
import { RegistryContext } from "@/hooks/use-registry";
import { useShellStore } from "@/stores/shell-store";
import { showAppErrorToast, useToastStore } from "@/stores/toast-store";
import { clearTauriMocks, mockTauri } from "@/test/tauri-mocks";
import { NAV_PAGES, type NavPages } from "./nav-page";
import { Providers } from "./providers";
import { buildAppRoutes } from "./routes";

const NAV: NavItem[] = [
  { id: "settings", label: "Settings", icon: "settings", route: "/settings", order: 3 },
  { id: "dashboard", label: "Dashboard", icon: "layout-dashboard", route: "/", order: 0 },
  { id: "models", label: "Models", icon: "boxes", route: "/models", order: 2 },
  { id: "history", label: "History", icon: "history", route: "/history", order: 1 },
];

const REGISTRY: RegistryView = { settings: [], sections: [], hotkeys: [], nav: NAV, engines: [], metrics: [], credits: [] };

const ipc = vi.fn<(cmd: string, payload?: unknown) => unknown>();

function renderShell(path = "/", pages: NavPages = NAV_PAGES) {
  const router = createMemoryRouter(buildAppRoutes(REGISTRY.nav, pages), { initialEntries: [path] });
  render(
    <Providers>
      <RegistryContext value={REGISTRY}>
        <RouterProvider router={router} />
      </RegistryContext>
    </Providers>,
  );
  return router;
}

/** Onboarding finished and the model installed: nothing is due. */
const SETTLED: OnboardingView = {
  required: false,
  first_run: false,
  speech_engine: "parakeet-tdt-0.6b-v3",
  speech_model_ready: true,
  microphone: "granted",
  record_hotkey: "Ctrl+Alt",
  hold_to_talk: true,
  steps: [],
};

const IDLE: SessionView = {
  status: "idle",
  transcript_id: null,
  elapsed_ms: 0,
  countdown_remaining_ms: null,
  outcome: null,
  error: null,
};

/** What the mocked Rust answers for reads the real pages make on mount. */
const PAGE_READS: Readonly<Record<string, unknown>> = {
  history_list: { items: [], next_cursor: null },
  metrics_summary: { range: "all_time", values: [] },
  metrics_activity: [],
  onboarding_get: SETTLED,
  session_get_state: IDLE,
};

beforeEach(() => {
  mockTauri((cmd, payload) => ipc(cmd, payload));
  ipc.mockImplementation((cmd) => PAGE_READS[cmd] ?? null);
  useToastStore.setState({ toasts: [], nextId: 1 });
  useShellStore.setState({ sidebarOpen: true });
});

afterEach(async () => {
  await clearTauriMocks();
  vi.clearAllMocks();
});

describe("app shell", () => {
  it("lists the registry nav in order and marks the current page", async () => {
    renderShell("/");
    expect(await screen.findByRole("heading", { level: 1, name: "Dashboard" })).toBeInTheDocument();
    expect(await screen.findByRole("group", { name: "Recent takes" })).toBeInTheDocument();

    const nav = screen.getByRole("navigation", { name: "Main" });
    const links = within(nav).getAllByRole("link");
    expect(links.map((link) => link.textContent)).toEqual(["Dashboard", "History", "Models", "Settings"]);
    expect(within(nav).getByRole("link", { name: "Dashboard" })).toHaveAttribute("aria-current", "page");
  });

  it("navigates from the sidebar", async () => {
    const router = renderShell("/");
    await screen.findByRole("heading", { level: 1, name: "Dashboard" });

    fireEvent.click(screen.getByRole("link", { name: "History" }));
    expect(await screen.findByRole("heading", { level: 1, name: "History" })).toBeInTheDocument();
    expect(router.state.location.pathname).toBe("/history");
    expect(screen.getByRole("link", { name: "History" })).toHaveAttribute("aria-current", "page");
    expect(screen.getByRole("link", { name: "Dashboard" })).not.toHaveAttribute("aria-current");
  });

  it("sends an unknown path to the first page", async () => {
    const router = renderShell("/nowhere");
    expect(await screen.findByRole("heading", { level: 1, name: "Dashboard" })).toBeInTheDocument();
    expect(router.state.location.pathname).toBe("/");
  });

  it("minimizes and closes the window from the titlebar", async () => {
    renderShell("/");
    await screen.findByRole("heading", { level: 1, name: "Dashboard" });

    fireEvent.click(screen.getByRole("button", { name: "Minimize" }));
    fireEvent.click(screen.getByRole("button", { name: "Close" }));
    const commands = ipc.mock.calls.map(([cmd]) => cmd);
    expect(commands).toContain("plugin:window|minimize");
    expect(commands).toContain("plugin:window|close");
  });

  it("hides and shows the sidebar from the titlebar", async () => {
    renderShell("/");
    await screen.findByRole("heading", { level: 1, name: "Dashboard" });
    expect(screen.getByText("Echo")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Hide sidebar" }));
    const show = screen.getByRole("button", { name: "Show sidebar" });
    expect(show).toHaveAttribute("aria-expanded", "false");
    const slot = document.getElementById(show.getAttribute("aria-controls") ?? "");
    // jsdom keeps inert content in the accessibility tree; WebView2 drops it, so the attribute is the contract.
    expect(slot).toHaveAttribute("inert");
    expect(slot).toHaveAttribute("data-state", "closed");

    fireEvent.click(show);
    expect(screen.getByRole("button", { name: "Hide sidebar" })).toHaveAttribute("aria-expanded", "true");
    expect(slot).not.toHaveAttribute("inert");
    expect(slot).toHaveAttribute("data-state", "open");
  });

  it("opens a page from an AppError toast action", async () => {
    const router = renderShell("/");
    await screen.findByRole("heading", { level: 1, name: "Dashboard" });

    act(() => {
      showAppErrorToast({ code: "ModelMissing", model_id: "parakeet-tdt-0.6b-v3" });
    });
    expect(await screen.findByText("Model not installed")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Open Models" }));
    expect(await screen.findByRole("heading", { level: 1, name: "Models" })).toBeInTheDocument();
    expect(router.state.location.pathname).toBe("/models");
  });

  it("runs the logs command from an AppError toast action", async () => {
    renderShell("/");
    await screen.findByRole("heading", { level: 1, name: "Dashboard" });

    act(() => {
      showAppErrorToast({ code: "Internal" });
    });
    fireEvent.click(await screen.findByRole("button", { name: "Open logs folder" }));
    expect(ipc).toHaveBeenCalledWith("app_open_logs_dir", expect.anything());
  });

  it("keeps the sidebar and offers a reload when a page fails", async () => {
    const consoleError = vi.spyOn(console, "error").mockImplementation(() => undefined);
    const Broken = () => {
      throw new Error("page failed");
    };
    renderShell("/", { ...NAV_PAGES, dashboard: Broken });

    const alert = await screen.findByRole("alert");
    expect(within(alert).getByText("Something went wrong")).toBeInTheDocument();
    expect(within(alert).getByRole("button", { name: "Reload" })).toBeInTheDocument();
    expect(within(alert).getByRole("button", { name: "Open logs folder" })).toBeInTheDocument();
    expect(screen.getByRole("navigation", { name: "Main" })).toBeInTheDocument();
    consoleError.mockRestore();
  });

  it("opens onboarding at launch when Rust says it is due", async () => {
    const due: OnboardingView = {
      ...SETTLED,
      required: true,
      first_run: true,
      steps: [{ id: "practice", label: "Try it", shown: "always", settings: [] }],
    };
    ipc.mockImplementation((cmd) => (cmd === "onboarding_get" ? due : (PAGE_READS[cmd] ?? null)));
    const router = renderShell("/");
    expect(await screen.findByRole("heading", { level: 1, name: "Try your hotkey" })).toBeInTheDocument();
    expect(router.state.location.pathname).toBe("/onboarding");
    expect(screen.queryByRole("navigation", { name: "Main" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Hide sidebar" })).not.toBeInTheDocument();
  });

  it("stays on the page when nothing is due and opens onboarding when asked", async () => {
    const router = renderShell("/");
    await screen.findByRole("heading", { level: 1, name: "Dashboard" });
    await waitFor(() => {
      expect(ipc).toHaveBeenCalledWith("onboarding_get", expect.anything());
    });
    expect(router.state.location.pathname).toBe("/");

    await act(async () => {
      await events.onboardingRequested.emit({});
    });
    expect(await screen.findByRole("heading", { level: 1, name: "Echo is ready" })).toBeInTheDocument();
    expect(router.state.location.pathname).toBe("/onboarding");
  });
});
