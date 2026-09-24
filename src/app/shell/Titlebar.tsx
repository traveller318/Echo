/**
 * SOURCE OF TRUTH KEYWORDS: Titlebar, custom titlebar, drag region, data-tauri-drag-region, minimize, close window, window controls
 * WHAT:  The main window's own titlebar (the window has no native decorations): a --titlebar-height strip that
 *        drags the window (double-click toggles maximize) with minimal minimize and close buttons at the right.
 * WHY:   04 §5 specifies a custom minimal titlebar. `data-tauri-drag-region="deep"` makes the whole strip a drag
 *        handle while Tauri still lets the buttons be clicked. Close only *asks* to close: app/windows.rs turns every
 *        close of the main window into a hide (02 §9), so the titlebar, Alt+F4 and the taskbar all behave the same.
 *        The window API is called at click time, never at import, so the component renders anywhere (tests, the
 *        error frame). A failed call is shown as an AppError toast; the tooltips name the icon-only buttons.
 * WHERE: app/shell/ShellFrame.tsx (every main-window state). Needs the capability permissions in
 *        src-tauri/capabilities/main.json (start-dragging, internal-toggle-maximize, minimize, close).
 */
import { getCurrentWindow } from "@tauri-apps/api/window";
import { MinusIcon, XIcon } from "lucide-react";
import type { ReactNode } from "react";
import { Button, Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui";
import { toAppError } from "@/lib/app-error";
import { showAppErrorToast } from "@/stores/toast-store";

function reportWindowError(error: unknown): void {
  showAppErrorToast(toAppError(error));
}

function minimize(): void {
  getCurrentWindow().minimize().catch(reportWindowError);
}

function close(): void {
  getCurrentWindow().close().catch(reportWindowError);
}

function WindowButton({ label, onClick, children }: { label: string; onClick: () => void; children: ReactNode }) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button variant="ghost" size="icon-sm" aria-label={label} onClick={onClick}>
          {children}
        </Button>
      </TooltipTrigger>
      <TooltipContent>{label}</TooltipContent>
    </Tooltip>
  );
}

export function Titlebar() {
  return (
    <header
      data-slot="titlebar"
      data-tauri-drag-region="deep"
      className="flex h-titlebar shrink-0 items-center justify-end gap-1 px-2 select-none"
    >
      <WindowButton label="Minimize" onClick={minimize}>
        <MinusIcon aria-hidden="true" />
      </WindowButton>
      <WindowButton label="Close" onClick={close}>
        <XIcon aria-hidden="true" />
      </WindowButton>
    </header>
  );
}
