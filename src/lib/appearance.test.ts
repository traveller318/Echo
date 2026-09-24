/**
 * SOURCE OF TRUTH KEYWORDS: appearance test, applyAppearance test, syncAppearance test, data-theme test, appearance race test
 * WHAT:  Verifies applyAppearance writes and removes the three <html> attributes, and syncAppearance listens
 *        before it reads, prefers a change that arrived during the read, and reports failures without throwing.
 * WHY:   Both windows theme themselves only through these attributes (docs/04 §2); a missed or stale update
 *        would leave a window in the wrong theme until restart.
 * WHERE: Runs in the `web` Vitest project (jsdom) with the generated bindings mocked.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { AppearanceView } from "@/bindings";

type Listener = (event: { payload: AppearanceView }) => void;

const bindings = vi.hoisted(() => ({
  listener: null as Listener | null,
  listen: vi.fn(),
  appearanceGet: vi.fn(),
  unlisten: vi.fn(),
}));

vi.mock("@/bindings", () => ({
  commands: { appearanceGet: bindings.appearanceGet },
  events: { appearanceChanged: { listen: bindings.listen } },
}));

const { applyAppearance, syncAppearance } = await import("./appearance");

const MICA_DARK: AppearanceView = { theme: "dark", transparency: "full", backdrop: "mica" };
const SYSTEM_REDUCED: AppearanceView = { theme: "system", transparency: "reduced", backdrop: "solid" };

describe("applyAppearance", () => {
  it("writes an explicit theme, full transparency and the backdrop", () => {
    const root = document.createElement("html");
    applyAppearance(MICA_DARK, root);
    expect(root.getAttribute("data-theme")).toBe("dark");
    expect(root.hasAttribute("data-transparency")).toBe(false);
    expect(root.getAttribute("data-backdrop")).toBe("mica");
  });

  it("removes data-theme for system and marks reduced transparency", () => {
    const root = document.createElement("html");
    applyAppearance(MICA_DARK, root);
    applyAppearance(SYSTEM_REDUCED, root);
    expect(root.hasAttribute("data-theme")).toBe(false);
    expect(root.getAttribute("data-transparency")).toBe("reduced");
    expect(root.getAttribute("data-backdrop")).toBe("solid");
  });
});

describe("syncAppearance", () => {
  let root: HTMLElement;

  beforeEach(() => {
    root = document.createElement("html");
    bindings.listener = null;
    bindings.listen.mockImplementation((listener: Listener) => {
      bindings.listener = listener;
      return Promise.resolve(bindings.unlisten);
    });
  });

  afterEach(() => {
    vi.clearAllMocks();
  });

  it("subscribes before reading, applies the read, then every change", async () => {
    bindings.appearanceGet.mockImplementation(() => {
      expect(bindings.listen).toHaveBeenCalledOnce();
      return Promise.resolve({ status: "ok", data: MICA_DARK });
    });
    const unsubscribe = await syncAppearance({ root });
    expect(root.getAttribute("data-theme")).toBe("dark");

    bindings.listener?.({ payload: SYSTEM_REDUCED });
    expect(root.getAttribute("data-transparency")).toBe("reduced");

    unsubscribe();
    expect(bindings.unlisten).toHaveBeenCalledOnce();
  });

  it("keeps a change that arrived while the read was in flight", async () => {
    bindings.appearanceGet.mockImplementation(() => {
      bindings.listener?.({ payload: SYSTEM_REDUCED });
      return Promise.resolve({ status: "ok", data: MICA_DARK });
    });
    await syncAppearance({ root });
    expect(root.hasAttribute("data-theme")).toBe(false);
    expect(root.getAttribute("data-backdrop")).toBe("solid");
  });

  it("reports a failed read and keeps listening", async () => {
    const onError = vi.fn();
    bindings.appearanceGet.mockResolvedValue({ status: "error", error: { code: "Internal" } });
    await syncAppearance({ root, onError });
    expect(onError).toHaveBeenCalledWith({ code: "Internal" });
    expect(root.hasAttribute("data-backdrop")).toBe(false);
    bindings.listener?.({ payload: MICA_DARK });
    expect(root.getAttribute("data-backdrop")).toBe("mica");
  });

  it("never rejects when the subscription itself fails", async () => {
    const onError = vi.fn();
    bindings.listen.mockRejectedValue(new Error("no Tauri here"));
    const unsubscribe = await syncAppearance({ root, onError });
    expect(onError).toHaveBeenCalledWith({ code: "Internal" });
    expect(bindings.appearanceGet).not.toHaveBeenCalled();
    expect(() => {
      unsubscribe();
    }).not.toThrow();
  });
});
