/**
 * SOURCE OF TRUTH KEYWORDS: HotkeyInput test, hotkey capture test, accelerator format test, modifier-only capture test, bindable chord test
 * WHAT:  Verifies the accelerator helpers (key tokens in the adapter's spelling, modifier order, splitting, the
 *        bindable-chord rule) and the HotkeyInput capture: chips for the value, a main-key chord, a modifier-only
 *        chord, Escape cancelling, and the hints for keys that cannot be used.
 * WHY:   The captured text is what the hotkey adapter parses (adapters/hotkey/low_level_hook/chord.rs); a wrong
 *        spelling would be refused at bind time, and a lone letter would take a key from every app.
 * WHERE: Runs in the `web` Vitest project (jsdom).
 */
import { act, fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { acceleratorKeys, formatAccelerator, isBindableChord, mainKeyToken, modifierToken } from "./accelerator";
import { HotkeyInput } from "./HotkeyInput";

describe("accelerator helpers", () => {
  it("names keys the way the hotkey adapter parses them", () => {
    expect(mainKeyToken("KeyV")).toBe("V");
    expect(mainKeyToken("Digit7")).toBe("7");
    expect(mainKeyToken("F13")).toBe("F13");
    expect(mainKeyToken("F25")).toBeUndefined();
    expect(mainKeyToken("Numpad4")).toBe("Numpad4");
    expect(mainKeyToken("ArrowUp")).toBe("Up");
    expect(mainKeyToken("Space")).toBe("Space");
    expect(mainKeyToken("Escape")).toBeUndefined();
    expect(mainKeyToken("CapsLock")).toBeUndefined();
    expect(modifierToken("ControlRight")).toBe("Ctrl");
    expect(modifierToken("MetaLeft")).toBe("Win");
    expect(modifierToken("KeyA")).toBeUndefined();
  });

  it("orders modifiers and splits stored text", () => {
    expect(formatAccelerator(["Shift", "Ctrl", "Alt"], "Space")).toBe("Ctrl+Alt+Shift+Space");
    expect(formatAccelerator(["Alt", "Ctrl"])).toBe("Ctrl+Alt");
    expect(acceleratorKeys(" Ctrl + Alt ")).toEqual(["Ctrl", "Alt"]);
    expect(acceleratorKeys("")).toEqual([]);
  });

  it("accepts a key with a modifier, a function key alone, or two modifiers", () => {
    expect(isBindableChord(["Ctrl"], "V")).toBe(true);
    expect(isBindableChord([], "V")).toBe(false);
    expect(isBindableChord([], "F9")).toBe(true);
    expect(isBindableChord(["Ctrl", "Alt"])).toBe(true);
    expect(isBindableChord(["Ctrl"])).toBe(false);
  });
});

function renderInput(value = "Ctrl+Alt") {
  const onChange = vi.fn<(accelerator: string) => void>();
  render(<HotkeyInput aria-label="Dictation hotkey" value={value} onChange={onChange} />);
  const button = screen.getByRole("button", { name: "Dictation hotkey" });
  return { button, onChange };
}

function startCapture(button: HTMLElement) {
  act(() => {
    button.focus();
    fireEvent.click(button);
  });
}

describe("HotkeyInput", () => {
  it("shows the combination as key chips", () => {
    const { button } = renderInput("Ctrl+Alt+Space");
    expect(Array.from(button.querySelectorAll("kbd"), (key) => key.textContent)).toEqual(["Ctrl", "Alt", "Space"]);
  });

  it("captures a combination with a main key on its keydown", () => {
    const { button, onChange } = renderInput();
    startCapture(button);
    expect(button).toHaveAttribute("data-capturing", "true");
    fireEvent.keyDown(button, { code: "ControlLeft", ctrlKey: true });
    fireEvent.keyDown(button, { code: "ShiftLeft", ctrlKey: true, shiftKey: true });
    expect(Array.from(button.querySelectorAll("kbd"), (key) => key.textContent)).toEqual(["Ctrl", "Shift"]);
    fireEvent.keyDown(button, { code: "KeyD", ctrlKey: true, shiftKey: true });
    expect(onChange).toHaveBeenCalledExactlyOnceWith("Ctrl+Shift+D");
    expect(button).toHaveAttribute("data-capturing", "false");
  });

  it("captures modifiers alone when they are released together", () => {
    const { button, onChange } = renderInput("Ctrl+Shift+D");
    startCapture(button);
    fireEvent.keyDown(button, { code: "AltLeft", altKey: true });
    fireEvent.keyDown(button, { code: "ControlLeft", altKey: true, ctrlKey: true });
    fireEvent.keyUp(button, { code: "AltLeft", ctrlKey: true });
    expect(onChange).not.toHaveBeenCalled();
    fireEvent.keyUp(button, { code: "ControlLeft" });
    expect(onChange).toHaveBeenCalledExactlyOnceWith("Ctrl+Alt");
  });

  it("explains a key that needs a modifier and a lone modifier", () => {
    const { button, onChange } = renderInput();
    startCapture(button);
    fireEvent.keyDown(button, { code: "KeyA" });
    expect(screen.getByRole("status")).toHaveTextContent("Add Ctrl, Alt, Shift or Win to that key.");
    fireEvent.keyDown(button, { code: "ShiftLeft", shiftKey: true });
    fireEvent.keyUp(button, { code: "ShiftLeft" });
    expect(screen.getByRole("status")).toHaveTextContent("Hold two modifiers together, or add a key.");
    expect(onChange).not.toHaveBeenCalled();
    expect(button).toHaveAttribute("data-capturing", "true");
  });

  it("cancels on Escape and keeps the combination", () => {
    const { button, onChange } = renderInput("Ctrl+Alt");
    startCapture(button);
    fireEvent.keyDown(button, { code: "Escape" });
    expect(button).toHaveAttribute("data-capturing", "false");
    expect(onChange).not.toHaveBeenCalled();
    expect(Array.from(button.querySelectorAll("kbd"), (key) => key.textContent)).toEqual(["Ctrl", "Alt"]);
  });

  it("stops capturing when focus leaves", () => {
    const { button } = renderInput();
    startCapture(button);
    act(() => {
      button.blur();
    });
    expect(button).toHaveAttribute("data-capturing", "false");
  });
});
