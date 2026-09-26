/**
 * SOURCE OF TRUTH KEYWORDS: shell store, useShellStore, sidebar open, sidebar collapse, toggleSidebar, zustand
 * WHAT:  The main window chrome's UI state (Zustand): whether the sidebar card is open, and `toggleSidebar`.
 * WHY:   Collapsing the sidebar is a view choice, not domain state (root CLAUDE.md §7). A store rather than
 *        ShellFrame state keeps the choice when the router swaps layouts (onboarding and back) within one window;
 *        it is not persisted, so each launch opens with the sidebar shown.
 * WHERE: app/shell/ShellFrame.tsx (reads it, collapses the sidebar slot, wires the Titlebar toggle).
 */
import { create } from "zustand";

interface ShellState {
  readonly sidebarOpen: boolean;
}

export const useShellStore = create<ShellState>()(() => ({ sidebarOpen: true }));

export function toggleSidebar(): void {
  useShellStore.setState((state) => ({ sidebarOpen: !state.sidebarOpen }));
}
