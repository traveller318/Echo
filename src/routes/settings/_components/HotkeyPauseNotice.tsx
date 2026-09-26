/**
 * SOURCE OF TRUTH KEYWORDS: HotkeyPauseNotice, hotkeys paused notice, resume hotkeys, tray pause hotkeys, Settings notice
 * WHAT:  An InlineNotice at the top of Settings while the user has paused Echo's hotkeys (the tray's "Pause
 *        hotkeys"), with a "Resume hotkeys" button; renders nothing otherwise.
 * WHY:   A paused hotkey looks like a broken app from the keyboard, and Settings is where someone looks when a
 *        shortcut does nothing; the state comes from the HotkeyGate through HotkeyStatusChanged, so it clears the moment
 *        the tray resumes them. A field that is only capturing a shortcut is not a pause and shows nothing.
 * WHERE: routes/settings/index.tsx, above the sections.
 */
import { PauseIcon } from "lucide-react";
import { InlineNotice } from "@/components/global";
import { Button } from "@/components/ui";
import { useHotkeyStatus, useSetHotkeysPaused } from "@/hooks";

export function HotkeyPauseNotice() {
  const status = useHotkeyStatus();
  const pause = useSetHotkeysPaused();
  if (status.data?.paused !== true) {
    return null;
  }
  return (
    <InlineNotice
      icon={<PauseIcon />}
      title="Hotkeys are paused"
      body="Echo's shortcuts won't start dictation or paste until you resume them. The tray icon can resume them too."
      action={
        <Button
          size="sm"
          disabled={pause.pending}
          onClick={() => {
            pause.setPaused(false);
          }}
        >
          Resume hotkeys
        </Button>
      }
    />
  );
}
