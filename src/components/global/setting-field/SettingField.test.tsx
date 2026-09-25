/**
 * SOURCE OF TRUTH KEYWORDS: SettingField test, setting control test, every SettingKind control, reset to default test, inline setting error test, dictionary editor test
 * WHAT:  Renders SettingField for every SettingKind and verifies the control each gets, that valid edits are
 *        committed as tagged SettingValues, that invalid ones show Rust's message and commit nothing, the reset
 *        button, the restart badge, and an inline write error.
 * WHY:   Every setting in the app goes through this one component; a broken kind would silently disable a whole
 *        class of settings. jsdom lacks the pointer-capture, scroll and ResizeObserver APIs Radix Select and Slider call, so
 *        they are stubbed for this file.
 * WHERE: Runs in the `web` Vitest project (jsdom).
 */
import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { afterAll, beforeAll, describe, expect, it, vi } from "vitest";
import type { AppError, EnumOption, SettingKind, SettingSpec, SettingValue } from "@/bindings";
import { SettingField } from "./SettingField";

vi.mock("@/bindings", () => ({ SETTING_TOKEN_MAX_CHARS: 128 }));

const saved = {
  scrollIntoView: Object.getOwnPropertyDescriptor(Element.prototype, "scrollIntoView"),
  hasPointerCapture: Object.getOwnPropertyDescriptor(Element.prototype, "hasPointerCapture"),
  releasePointerCapture: Object.getOwnPropertyDescriptor(Element.prototype, "releasePointerCapture"),
};

/** Radix Slider measures its thumbs; jsdom has no ResizeObserver. */
class NoopResizeObserver {
  observe(): void {
    // jsdom has no layout to observe.
  }
  unobserve(): void {
    // Nothing is observed.
  }
  disconnect(): void {
    // Nothing is observed.
  }
}

beforeAll(() => {
  vi.stubGlobal("ResizeObserver", NoopResizeObserver);
  Object.defineProperty(Element.prototype, "scrollIntoView", { configurable: true, value: () => undefined });
  Object.defineProperty(Element.prototype, "hasPointerCapture", { configurable: true, value: () => false });
  Object.defineProperty(Element.prototype, "releasePointerCapture", { configurable: true, value: () => undefined });
});

afterAll(() => {
  vi.unstubAllGlobals();
  for (const [name, descriptor] of Object.entries(saved)) {
    if (descriptor === undefined) {
      Reflect.deleteProperty(Element.prototype, name);
    } else {
      Object.defineProperty(Element.prototype, name, descriptor);
    }
  }
});

function spec(kind: SettingKind, defaultValue: SettingValue, overrides: Partial<SettingSpec> = {}): SettingSpec {
  return {
    key: "test.value",
    section: "general",
    label: "Test setting",
    help: "What it does.",
    kind,
    default: defaultValue,
    restart_required: false,
    visible: true,
    requires: null,
    ...overrides,
  };
}

function renderField(
  setting: SettingSpec,
  value: SettingValue,
  extra: { options?: readonly EnumOption[]; error?: AppError | null; onReset?: () => void } = {},
) {
  const onCommit = vi.fn<(value: SettingValue) => void>();
  const view = render(<SettingField spec={setting} value={value} onCommit={onCommit} {...extra} />);
  return { onCommit, view };
}

describe("SettingField", () => {
  it("renders a Bool as a switch that saves when flipped", async () => {
    const { onCommit } = renderField(spec({ kind: "bool" }, { kind: "bool", value: true }), {
      kind: "bool",
      value: true,
    });
    const toggle = screen.getByRole("switch", { name: "Test setting" });
    expect(toggle).toHaveAccessibleDescription("What it does.");
    act(() => {
      fireEvent.click(toggle);
    });
    await vi.waitFor(() => {
      expect(onCommit).toHaveBeenCalledExactlyOnceWith({ kind: "bool", value: false });
    });
  });

  it("renders an Int as a slider and number field that save only valid numbers", async () => {
    const int: SettingKind = { kind: "int", min: 10, max: 200, unit: "words_per_minute" };
    const { onCommit } = renderField(spec(int, { kind: "int", value: 40 }), { kind: "int", value: 40 });
    expect(screen.getByRole("slider", { name: "Test setting" })).toHaveAttribute("aria-valuenow", "40");
    expect(screen.getByText("wpm")).toBeInTheDocument();
    const field = screen.getByRole("spinbutton", { name: "Test setting" });
    await act(async () => {
      fireEvent.change(field, { target: { value: "5" } });
      fireEvent.blur(field);
      await Promise.resolve();
    });
    expect(await screen.findByText("Choose a number from 10 to 200.")).toBeInTheDocument();
    expect(onCommit).not.toHaveBeenCalled();
    await act(async () => {
      fireEvent.change(field, { target: { value: "65" } });
      fireEvent.keyDown(field, { key: "Enter" });
      await Promise.resolve();
    });
    await vi.waitFor(() => {
      expect(onCommit).toHaveBeenCalledExactlyOnceWith({ kind: "int", value: 65 });
    });
  });

  it("renders an Enum as a select of the options offered and saves a pick", async () => {
    const options: EnumOption[] = [
      { value: "system", label: "Match Windows", requires: null },
      { value: "dark", label: "Dark", requires: null },
    ];
    const kind: SettingKind = { kind: "enum", options: { from: "fixed", list: options } };
    const { onCommit } = renderField(spec(kind, { kind: "enum", value: "system" }), { kind: "enum", value: "system" }, {
      options,
    });
    const trigger = screen.getByRole("combobox", { name: "Test setting" });
    expect(trigger).toHaveTextContent("Match Windows");
    act(() => {
      fireEvent.keyDown(trigger, { key: "Enter" });
    });
    const dark = await screen.findByRole("option", { name: "Dark" });
    await act(async () => {
      fireEvent.keyDown(dark, { key: "Enter" });
      await Promise.resolve();
    });
    await vi.waitFor(() => {
      expect(onCommit).toHaveBeenCalledExactlyOnceWith({ kind: "enum", value: "dark" });
    });
  });

  it("names languages and says when nothing can be chosen", () => {
    const languages: SettingKind = { kind: "enum", options: { from: "runtime", source: "asr_languages" } };
    renderField(spec(languages, { kind: "enum", value: "auto" }), { kind: "enum", value: "de" }, {
      options: [
        { value: "auto", label: "Auto-detect", requires: null },
        { value: "de", label: "de", requires: null },
      ],
    });
    expect(screen.getByRole("combobox", { name: "Test setting" })).toHaveTextContent("German");

    const polishers: SettingKind = { kind: "enum", options: { from: "runtime", source: "model_polishers" } };
    const { view } = renderField(
      spec(polishers, { kind: "enum", value: "qwen3-1.7b" }, { label: "Polish model" }),
      { kind: "enum", value: "qwen3-1.7b" },
      { options: [] },
    );
    const empty = within(view.container).getByRole("combobox", { name: "Polish model" });
    expect(empty).toBeDisabled();
    expect(empty).toHaveTextContent("Nothing to choose yet");
  });

  it("renders a Hotkey as a capture field that saves a new combination", async () => {
    const { onCommit } = renderField(spec({ kind: "hotkey" }, { kind: "hotkey", value: "Ctrl+Alt" }), {
      kind: "hotkey",
      value: "Ctrl+Alt",
    });
    const field = screen.getByRole("button", { name: "Test setting" });
    act(() => {
      field.focus();
      fireEvent.click(field);
    });
    await act(async () => {
      fireEvent.keyDown(field, { code: "KeyK", ctrlKey: true, altKey: true });
      await Promise.resolve();
    });
    await vi.waitFor(() => {
      expect(onCommit).toHaveBeenCalledExactlyOnceWith({ kind: "hotkey", value: "Ctrl+Alt+K" });
    });
  });

  it("renders a Device as a microphone select that keeps a disconnected pin visible", async () => {
    const microphones: EnumOption[] = [{ value: "usb-mic", label: "USB Microphone", requires: null }];
    const { onCommit } = renderField(
      spec({ kind: "device" }, { kind: "device", value: null }),
      { kind: "device", value: "gone-mic" },
      { options: microphones },
    );
    const trigger = screen.getByRole("combobox", { name: "Test setting" });
    expect(trigger).toHaveTextContent("Saved microphone (not connected)");
    act(() => {
      fireEvent.keyDown(trigger, { key: "Enter" });
    });
    const systemDefault = await screen.findByRole("option", { name: "System default" });
    await act(async () => {
      fireEvent.keyDown(systemDefault, { key: "Enter" });
      await Promise.resolve();
    });
    await vi.waitFor(() => {
      expect(onCommit).toHaveBeenCalledExactlyOnceWith({ kind: "device", value: null });
    });
  });

  it("renders Text as a field that saves on Enter and refuses what Rust would", async () => {
    const { onCommit } = renderField(spec({ kind: "text", max_len: 5 }, { kind: "text", value: "" }), {
      kind: "text",
      value: "",
    });
    const field = screen.getByRole("textbox", { name: "Test setting" });
    await act(async () => {
      fireEvent.change(field, { target: { value: "toolong" } });
      fireEvent.keyDown(field, { key: "Enter" });
      await Promise.resolve();
    });
    expect(await screen.findByText("Use at most 5 characters.")).toBeInTheDocument();
    expect(onCommit).not.toHaveBeenCalled();
    await act(async () => {
      fireEvent.change(field, { target: { value: "note" } });
      fireEvent.keyDown(field, { key: "Enter" });
      await Promise.resolve();
    });
    await vi.waitFor(() => {
      expect(onCommit).toHaveBeenCalledExactlyOnceWith({ kind: "text", value: "note" });
    });
  });

  it("edits Pairs in a sheet and saves them together once every row is valid", async () => {
    const pairs: SettingKind = { kind: "pairs", max_pairs: 500, max_len: 100 };
    const { onCommit } = renderField(spec(pairs, { kind: "pairs", value: [] }, { label: "Dictionary" }), {
      kind: "pairs",
      value: [{ from: "echo", to: "Echo" }],
    });
    expect(screen.getByText("1 word")).toBeInTheDocument();
    act(() => {
      fireEvent.click(screen.getByRole("button", { name: "Dictionary" }));
    });
    const sheet = await screen.findByRole("dialog", { name: "Dictionary" });
    act(() => {
      fireEvent.click(within(sheet).getByRole("button", { name: "Add entry" }));
    });
    const word = within(sheet).getByRole("textbox", { name: "Word 2" });
    await act(async () => {
      fireEvent.change(word, { target: { value: "ECHO" } });
      fireEvent.click(within(sheet).getByRole("button", { name: "Save" }));
      await Promise.resolve();
    });
    expect(await within(sheet).findByText('"ECHO" is listed more than once.')).toBeInTheDocument();
    expect(onCommit).not.toHaveBeenCalled();
    await act(async () => {
      fireEvent.change(word, { target: { value: "parakeet" } });
      fireEvent.change(within(sheet).getByRole("textbox", { name: "Write word 2 as" }), {
        target: { value: "Parakeet" },
      });
      fireEvent.click(within(sheet).getByRole("button", { name: "Save" }));
      await Promise.resolve();
    });
    await vi.waitFor(() => {
      expect(onCommit).toHaveBeenCalledExactlyOnceWith({
        kind: "pairs",
        value: [
          { from: "echo", to: "Echo" },
          { from: "parakeet", to: "Parakeet" },
        ],
      });
    });
  });

  it("offers a reset only when the value differs from the default", () => {
    const onReset = vi.fn<() => void>();
    const setting = spec({ kind: "bool" }, { kind: "bool", value: true });
    const { view } = renderField(setting, { kind: "bool", value: true }, { onReset });
    expect(screen.queryByRole("button", { name: "Reset Test setting to default" })).not.toBeInTheDocument();
    view.unmount();
    renderField(setting, { kind: "bool", value: false }, { onReset });
    act(() => {
      fireEvent.click(screen.getByRole("button", { name: "Reset Test setting to default" }));
    });
    expect(onReset).toHaveBeenCalledOnce();
  });

  it("shows a refused write inline and a restart note", () => {
    renderField(
      spec({ kind: "hotkey" }, { kind: "hotkey", value: "Ctrl+Alt" }, { restart_required: true }),
      { kind: "hotkey", value: "Ctrl+Alt" },
      { error: { code: "Hotkey", reason: "conflict" } },
    );
    expect(screen.getByRole("alert")).toHaveTextContent(
      "That shortcut is taken. Another Echo shortcut already uses it. Pick a different one.",
    );
    expect(screen.getByRole("button", { name: "Test setting" })).toHaveAttribute("aria-invalid", "true");
    expect(screen.getByText("Needs restart")).toBeInTheDocument();
    expect(screen.getByText(/Takes effect the next time Echo starts\./)).toBeInTheDocument();
  });
});
