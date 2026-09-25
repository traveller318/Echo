/**
 * SOURCE OF TRUTH KEYWORDS: mockTauri, clearTauriMocks, Tauri IPC mock, event mock, listener teardown, mockIPC shouldMockEvents
 * WHAT:  `mockTauri(handler)`: the main window, every command answered by `handler`, and Tauri's event plugin
 *        mocked. `clearTauriMocks()`: unmounts what the test rendered, lets pending listen/unlisten calls settle,
 *        then removes the mocks.
 * WHY:   Rendered windows subscribe to Rust events (the shell follows NavigationRequested, History follows
 *        HistoryChanged), and an unsubscribe calls the event plugin's `unregisterListener`. Without the event mock
 *        it does not exist, and removing the mocks before React unmounts leaves late unsubscribes calling a deleted
 *        function (unhandled rejections that make Vitest fail). One helper keeps the order right in every test.
 * WHERE: app/app.test.tsx, app/router.test.tsx; any test that renders a window through the real Tauri API.
 */
import { clearMocks, mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import { act, cleanup } from "@testing-library/react";

export type IpcHandler = (cmd: string, payload?: unknown) => unknown;

export function mockTauri(handler: IpcHandler): void {
  mockWindows("main");
  mockIPC(handler, { shouldMockEvents: true });
}

export async function clearTauriMocks(): Promise<void> {
  cleanup();
  await act(async () => {
    await Promise.resolve();
  });
  clearMocks();
}
