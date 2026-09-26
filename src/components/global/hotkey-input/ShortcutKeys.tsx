/**
 * SOURCE OF TRUTH KEYWORDS: ShortcutKeys, shortcut chips, Kbd chips, accelerator display, hotkey display
 * WHAT:  An accelerator (`Ctrl+Alt`, or a list of held keys) drawn as Kbd chips in one group.
 * WHY:   Hotkeys are shown as key chips everywhere (the hotkey field, onboarding's instructions, later tray tips); one
 *        component keeps the chips identical and the accelerator split in one place (accelerator.ts).
 * WHERE: HotkeyInput (this folder); routes/onboarding (hotkey and practice steps). Exported through
 *        components/global.
 */
import { Kbd, KbdGroup } from "@/components/ui";
import { acceleratorKeys } from "./accelerator";

export interface ShortcutKeysProps {
  /** A stored accelerator (`Ctrl+Alt+V`) or the keys to show, in order. */
  readonly shortcut: string | readonly string[];
  readonly className?: string;
}

export function ShortcutKeys({ shortcut, className }: ShortcutKeysProps) {
  const keys = typeof shortcut === "string" ? acceleratorKeys(shortcut) : shortcut;
  return (
    <KbdGroup className={className}>
      {keys.map((key) => (
        <Kbd key={key}>{key}</Kbd>
      ))}
    </KbdGroup>
  );
}
