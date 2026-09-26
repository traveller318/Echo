/**
 * SOURCE OF TRUTH KEYWORDS: HotkeyInput, shortcut field, hotkey capture control, Kbd chips, accelerator input, rebind hotkey, onCaptureChange
 * WHAT:  A field that shows an accelerator (`value`) as Kbd chips and, when pressed, captures a new combination
 *        from the keyboard and reports it through `onChange`. Escape cancels; while capturing it shows the
 *        modifiers held and, when an attempt cannot be used, why.
 * WHY:   Hotkeys are shown as key chips everywhere (04 §6) and typed accelerator text would be error-prone, so the
 *        combination is captured, never typed. It is a button, so it is focusable, labelled by its FieldLabel and
 *        started with Enter or Space; `aria-invalid` gives it the --color-record border the other controls use, so a
 *        conflict shows in --color-record at the control (04 §5) while the message keeps readable text colour. The
 *        component is controlled and saves nothing itself: the caller decides (Settings writes the setting,
 *        onboarding may test it first), and hears when capturing starts and ends through `onCaptureChange` (Settings
 *        switches Echo's own hotkeys off meanwhile, so the field can read them).
 * WHERE: SettingField for Hotkey settings (components/global/setting-field); onboarding's hotkey step (step 24).
 */
import { KeyboardIcon } from "lucide-react";
import { useId, type ComponentProps } from "react";
import { cn } from "@/lib/cn";
import { acceleratorKeys } from "./accelerator";
import { ShortcutKeys } from "./ShortcutKeys";
import { useHotkeyCapture, type HotkeyCaptureHint } from "./use-hotkey-capture";

export type HotkeyInputProps = Omit<ComponentProps<"button">, "value" | "onChange" | "children" | "type"> & {
  /** The accelerator shown, e.g. `Ctrl+Alt+Space`; empty shows "Not set". */
  readonly value: string;
  /** Called once with each combination captured. */
  readonly onChange: (accelerator: string) => void;
  /** Called with true when capturing starts and false when it ends for any reason. */
  readonly onCaptureChange?: (capturing: boolean) => void;
  readonly invalid?: boolean;
};

const HINT_COPY: Readonly<Record<HotkeyCaptureHint, string>> = {
  "needs-modifier": "Add Ctrl, Alt, Shift or Win to that key.",
  "needs-second-modifier": "Hold two modifiers together, or add a key.",
  "unsupported-key": "That key can't be used in a shortcut. Try another one.",
};

export function HotkeyInput({
  value,
  onChange,
  onCaptureChange,
  invalid = false,
  className,
  onBlur,
  ...props
}: HotkeyInputProps) {
  const capture = useHotkeyCapture(onChange, onCaptureChange);
  const hintId = useId();
  const keys = capture.capturing ? capture.held : acceleratorKeys(value);
  const placeholder = capture.capturing ? "Press a shortcut" : "Not set";

  return (
    <div data-slot="hotkey-input" className="flex w-full flex-col gap-1">
      <button
        type="button"
        aria-invalid={invalid}
        aria-describedby={capture.hint === null ? undefined : hintId}
        data-capturing={capture.capturing}
        className={cn(
          "inline-flex h-control w-full min-w-0 items-center justify-between gap-2 rounded-control bg-fill px-2",
          "border-(length:--border-hairline) border-transparent text-body text-fg",
          "transition-[background-color,border-color] duration-(--duration-fast) ease-standard hover:bg-fill-hover",
          "data-[capturing=true]:border-accent data-[capturing=true]:bg-accent-soft",
          "disabled:pointer-events-none disabled:opacity-(--opacity-disabled)",
          "aria-invalid:border-record",
          className,
        )}
        onClick={capture.capturing ? undefined : capture.start}
        onKeyDown={capture.onKeyDown}
        onKeyUp={capture.onKeyUp}
        onBlur={(event) => {
          capture.cancel();
          onBlur?.(event);
        }}
        {...props}
      >
        {keys.length === 0 ? (
          <span className="truncate text-fg-tertiary">{placeholder}</span>
        ) : (
          <ShortcutKeys shortcut={keys} />
        )}
        <KeyboardIcon aria-hidden="true" className="size-icon-sm shrink-0 text-fg-secondary" />
        {capture.capturing ? <span className="sr-only">Press the new shortcut, or Escape to cancel.</span> : null}
      </button>
      {capture.hint === null ? null : (
        <p id={hintId} role="status" className="text-footnote text-fg-secondary">
          {HINT_COPY[capture.hint]}
        </p>
      )}
    </div>
  );
}
