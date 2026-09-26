/**
 * SOURCE OF TRUTH KEYWORDS: Titlebar, custom titlebar, drag region, data-tauri-drag-region, minimize, close window, window controls, sidebar toggle, wave logo brand
 * WHAT:  The main window's own titlebar (the window has no native decorations): a full-width --titlebar-height strip
 *        that drags the window (double-click toggles maximize), with the wave logo and the Echo name at the left, an
 *        optional sidebar toggle beside them (`sidebar`), and minimal minimize and close buttons at the right.
 * WHY:   04 §5 specifies a custom minimal titlebar above the sidebar and content cards. `data-tauri-drag-region="deep"`
 *        makes the whole strip a drag handle while Tauri still lets the buttons be clicked. Close only *asks* to
 *        close: app/windows.rs turns every close of the main window into a hide (02 §9), so the titlebar, Alt+F4 and
 *        the taskbar all behave the same. The window API is called at click time, never at import, so the component
 *        renders anywhere (tests, the error frame). A failed call is shown as an AppError toast; the tooltips name
 *        the icon-only buttons. The toggle is a prop, not a store read, because only ShellFrame knows whether a
 *        sidebar exists (onboarding and the registry gate have none). The logo is imported `?no-inline` because the
 *        CSP's img-src is 'self' only: Vite would inline a small image as a data: URI, which the webview would block.
 * WHERE: app/shell/ShellFrame.tsx (every main-window state). Needs the capability permissions in
 *        src-tauri/capabilities/main.json (start-dragging, internal-toggle-maximize, minimize, close).
 */
import { getCurrentWindow } from "@tauri-apps/api/window";
import { MinusIcon, PanelLeftIcon, XIcon } from "lucide-react";
import type { ReactNode } from "react";
import waveLogo from "@/assets/wave-logo.png?no-inline";
import { Button, Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui";
import { toAppError } from "@/lib/app-error";
import { showAppErrorToast } from "@/stores/toast-store";

/** The sidebar toggle's state and target; absent when the frame has no sidebar. */
export interface TitlebarSidebarToggle {
  readonly open: boolean;
  /** Id of the element the toggle shows and hides (aria-controls). */
  readonly controls: string;
  readonly onToggle: () => void;
}

export interface TitlebarProps {
  readonly sidebar?: TitlebarSidebarToggle;
}

function reportWindowError(error: unknown): void {
  showAppErrorToast(toAppError(error));
}

function minimize(): void {
  getCurrentWindow().minimize().catch(reportWindowError);
}

function close(): void {
  getCurrentWindow().close().catch(reportWindowError);
}

function TitlebarButton({
  label,
  onClick,
  children,
  ...aria
}: {
  label: string;
  onClick: () => void;
  children: ReactNode;
  "aria-expanded"?: boolean;
  "aria-controls"?: string;
}) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button variant="ghost" size="icon-sm" aria-label={label} onClick={onClick} {...aria}>
          {children}
        </Button>
      </TooltipTrigger>
      <TooltipContent>{label}</TooltipContent>
    </Tooltip>
  );
}

export function Titlebar({ sidebar }: TitlebarProps) {
  return (
    <header
      data-slot="titlebar"
      data-tauri-drag-region="deep"
      className="flex h-titlebar shrink-0 items-center gap-1 px-3 select-none"
    >
      <div data-slot="titlebar-brand" className="flex items-center gap-2 ps-1 pe-2">
        <img
          src={waveLogo}
          alt=""
          aria-hidden
          draggable={false}
          className="w-(--titlebar-logo-width) shrink-0"
        />
        <span className="text-callout font-semibold text-fg">Echo</span>
      </div>
      {sidebar === undefined ? null : (
        <TitlebarButton
          label={sidebar.open ? "Hide sidebar" : "Show sidebar"}
          aria-expanded={sidebar.open}
          aria-controls={sidebar.controls}
          onClick={sidebar.onToggle}
        >
          <PanelLeftIcon aria-hidden="true" />
        </TitlebarButton>
      )}
      <div className="ms-auto flex items-center gap-1">
        <TitlebarButton label="Minimize" onClick={minimize}>
          <MinusIcon aria-hidden="true" />
        </TitlebarButton>
        <TitlebarButton label="Close" onClick={close}>
          <XIcon aria-hidden="true" />
        </TitlebarButton>
      </div>
    </header>
  );
}
