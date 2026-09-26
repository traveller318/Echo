/**
 * SOURCE OF TRUTH KEYWORDS: useAbout, ABOUT_QUERY, app_about, AboutView, version, memory use, useOpenLogsFolder, app_open_logs_dir
 * WHAT:  `useAbout()` reads `app_about` (Echo's version, whether it is a development build, and its memory use now);
 *        `useOpenLogsFolder()` shows the local logs folder (`app_open_logs_dir`).
 * WHY:   The version is the one Rust read from tauri.conf.json, so About never disagrees with the installer. Memory
 *        changes all the time and no event announces it, so About reads it again when the window regains focus
 *        (useRefreshOnWindowFocus), never on a timer (root CLAUDE.md §7: no polling).
 * WHERE: routes/settings (About).
 */
import type { UseQueryResult } from "@tanstack/react-query";
import { commands, type AboutView } from "@/bindings";
import { useEchoMutation } from "./use-echo-mutation";
import { useEchoQuery, type EchoQuery } from "./use-echo-query";
import { useRefreshOnWindowFocus } from "./use-onboarding";

export const ABOUT_QUERY: EchoQuery<AboutView> = {
  queryKey: ["app", "about"],
  command: commands.appAbout,
};

export function useAbout(): UseQueryResult<AboutView> {
  useRefreshOnWindowFocus(ABOUT_QUERY);
  return useEchoQuery(ABOUT_QUERY);
}

export interface OpenLogsFolder {
  readonly open: () => void;
  readonly pending: boolean;
}

export function useOpenLogsFolder(): OpenLogsFolder {
  const open = useEchoMutation<undefined, null>(commands.appOpenLogsDir);
  return {
    open: () => {
      open.mutate(undefined);
    },
    pending: open.isPending,
  };
}
