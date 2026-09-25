/**
 * SOURCE OF TRUTH KEYWORDS: SettingsPage test, settings route test, registry-generated settings test, hidden setting test, caps-gated setting test, settings write test
 * WHAT:  Verifies the Settings page against mocked commands: one card per registry section with its heading, only
 *        visible settings whose caps requirement holds, rows showing the effective values, a change sent through
 *        `settings_set`, a reset through `settings_reset`, and a refused hotkey shown inline; plus the pure grouping
 *        rule on its own.
 * WHY:   The page wires the registry, the settings reads and SettingField together; this is the one place that
 *        whole flow runs short of Rust.
 * WHERE: Runs in the `web` Vitest project with `@/bindings` mocked.
 */
import { QueryClientProvider } from "@tanstack/react-query";
import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type {
  CapsRequirement,
  NavItem,
  RegistryView,
  SettingEntry,
  SettingSpec,
  SettingsAvailability,
} from "@/bindings";
import { RegistryContext } from "@/hooks";
import { createEchoQueryClient } from "@/lib/query-client";
import { settingsBySection } from "./_components/settings-layout";

const NAV: NavItem = { id: "settings", label: "Settings", icon: "settings", route: "/settings", order: 3 };

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
  nav: [NAV],
  engines: [],
  metrics: [],
};

const VALUES: SettingEntry[] = [
  { key: "general.sound_cues", value: { kind: "bool", value: false } },
  { key: "general.onboarded", value: { kind: "bool", value: false } },
  { key: "hotkeys.record", value: { kind: "hotkey", value: "Ctrl+Alt" } },
  { key: "updates.auto_check", value: { kind: "bool", value: true } },
];

const AVAILABILITY: SettingsAvailability = { caps: ["hotkey_release", "multiple_languages"], options: [] };

const mocks = vi.hoisted(() => ({
  settingsGetAll: vi.fn(),
  settingsAvailability: vi.fn(),
  settingsSet: vi.fn(),
  settingsReset: vi.fn(),
  audioListDevices: vi.fn(),
}));

vi.mock("@/bindings", () => ({
  SETTING_TOKEN_MAX_CHARS: 128,
  commands: mocks,
  // No Rust events in this test: the page is rendered without the invalidation bridge.
  events: {},
}));

const { default: SettingsPage } = await import("./index");

beforeEach(() => {
  mocks.settingsGetAll.mockResolvedValue({ status: "ok", data: VALUES });
  mocks.settingsAvailability.mockResolvedValue({ status: "ok", data: AVAILABILITY });
  mocks.settingsSet.mockImplementation((input: SettingEntry) => Promise.resolve({ status: "ok", data: input }));
  mocks.settingsReset.mockResolvedValue({ status: "ok", data: VALUES[0] });
  mocks.audioListDevices.mockResolvedValue({ status: "ok", data: [] });
});

afterEach(() => {
  vi.clearAllMocks();
});

function renderPage() {
  return render(
    <QueryClientProvider client={createEchoQueryClient()}>
      <RegistryContext value={REGISTRY}>
        <SettingsPage nav={NAV} />
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
    ]);
    expect(screen.getByRole("switch", { name: "Sound cues" })).not.toBeChecked();
    expect(screen.queryByText("Onboarding done")).not.toBeInTheDocument();
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
