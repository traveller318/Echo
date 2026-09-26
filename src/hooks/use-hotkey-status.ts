/**
 * SOURCE OF TRUTH KEYWORDS: useHotkeyStatus, useSetHotkeysPaused, useHotkeyCaptureLease, HOTKEY_STATUS_QUERY, hotkeys_status, hotkeys_pause, hotkeys_capture, pause hotkeys, capture lease
 * WHAT:  The hotkey switch data layer: `useHotkeyStatus()` reads whether Echo's hotkeys are paused (the tray's "Pause
 *        hotkeys") or off while a field captures; `useSetHotkeysPaused()` switches the pause; `useHotkeyCaptureLease()`
 *        gives a hotkey field the callback that switches Echo's hotkeys off while it captures and back on when it
 *        stops or unmounts.
 * WHY:   Rust's HotkeyGate owns the state (root CLAUDE.md §7): the status refetches on HotkeyStatusChanged, never by
 *        polling, so Settings and the tray always agree. While a field captures, Echo's keyboard hook must not act on
 *        the keys (holding the current dictation chord would start a take and a bound chord would never reach the
 *        page), so the field holds a lease; lease calls go through an ordered queue so a quick start/stop never lands
 *        reversed, and Rust ends a lease the page forgot after a minute.
 * WHERE: routes/settings (the paused notice), components/global/setting-field HotkeyControl (the lease).
 */
import type { UseQueryResult } from "@tanstack/react-query";
import { useCallback, useEffect, useRef } from "react";
import { commands, type HotkeyStatus } from "@/bindings";
import { createCommandQueue } from "@/lib/command-queue";
import type { EchoEventName } from "@/lib/echo-events";
import { useEchoMutation } from "./use-echo-mutation";
import { useEchoQuery, type EchoQuery } from "./use-echo-query";

/** Rust events after which the hotkey status is stale. */
export const HOTKEY_STATUS_EVENTS: readonly EchoEventName[] = ["hotkeyStatusChanged"];

export const HOTKEY_STATUS_QUERY: EchoQuery<HotkeyStatus> = {
  queryKey: ["hotkeys", "status"],
  command: commands.hotkeysStatus,
  invalidatedBy: HOTKEY_STATUS_EVENTS,
};

export function useHotkeyStatus(): UseQueryResult<HotkeyStatus> {
  return useEchoQuery(HOTKEY_STATUS_QUERY);
}

export interface SetHotkeysPaused {
  /** Pauses (true) or resumes (false) Echo's hotkeys. */
  readonly setPaused: (paused: boolean) => void;
  readonly pending: boolean;
}

export function useSetHotkeysPaused(): SetHotkeysPaused {
  const pause = useEchoMutation(commands.hotkeysPause);
  return {
    setPaused: (paused) => {
      pause.mutate({ paused });
    },
    pending: pause.isPending,
  };
}

const sendCapture = createCommandQueue("switch Echo's hotkeys around a shortcut capture");

/** The callback a hotkey field reports its capturing state to; releases the lease when the field unmounts. */
export function useHotkeyCaptureLease(): (capturing: boolean) => void {
  const held = useRef(false);
  useEffect(
    () => () => {
      if (held.current) {
        held.current = false;
        sendCapture(() => commands.hotkeysCapture({ active: false }));
      }
    },
    [],
  );
  return useCallback((capturing: boolean) => {
    if (held.current === capturing) {
      return;
    }
    held.current = capturing;
    sendCapture(() => commands.hotkeysCapture({ active: capturing }));
  }, []);
}
