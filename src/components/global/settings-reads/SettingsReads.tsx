/**
 * SOURCE OF TRUTH KEYWORDS: SettingsReads, settings reads gate, settings loading state, settings error state, try again, settings values and availability, render prop
 * WHAT:  Reads the settings in effect (`settings_get_all`) and what may be offered now (`settings_availability`),
 *        shows an EmptyState with "Try again" when either read failed, an indeterminate bar while they load, and
 *        otherwise renders `children` with both results (and whether the values are being read again).
 * WHY:   Every page built from registry settings (Settings, Dictionary) needs both reads before it can draw a row, and
 *        must fail and load the same way; one gate keeps that identical instead of copied per page. The data stays
 *        in the TanStack cache the hooks own (refetched on SettingsChanged), never in page state (root CLAUDE.md §7).
 *        The bar appears only after --delay-loading (ProgressBar), so a fast read never flashes. `refreshing` lets a
 *        page that saves whole values (the dictionary's list) wait for the value in effect before the next edit.
 * WHERE: routes/settings/index.tsx and routes/dictionary/index.tsx, inside their Page. Exported through
 *        components/global/index.ts.
 */
import type { ReactNode } from "react";
import type { NavIcon as NavIconName, SettingsAvailability } from "@/bindings";
import { Button } from "@/components/ui";
import { useSettingsAvailability, useSettingValues, type SettingValues } from "@/hooks";
import { describeAppError, toAppError } from "@/lib/app-error";
import { EmptyState } from "../empty-state";
import { NavIcon } from "../nav-icon";
import { ProgressBar } from "../progress-bar";

export interface SettingsReadsResult {
  readonly values: SettingValues;
  readonly availability: SettingsAvailability;
  /** The values are being read again (after a save or a SettingsChanged). */
  readonly refreshing: boolean;
}

export interface SettingsReadsProps {
  /** The page's icon, shown when a read failed. */
  readonly icon: NavIconName;
  /** Accessible name of the loading bar (e.g. "Loading settings"). */
  readonly loadingLabel: string;
  readonly children: (reads: SettingsReadsResult) => ReactNode;
}

export function SettingsReads({ icon, loadingLabel, children }: SettingsReadsProps) {
  const values = useSettingValues();
  const availability = useSettingsAvailability();

  if (values.isError || availability.isError) {
    const copy = describeAppError(toAppError(values.error ?? availability.error));
    return (
      <EmptyState
        icon={<NavIcon icon={icon} />}
        title={copy.title}
        body={copy.body}
        action={
          <Button
            onClick={() => {
              void values.refetch();
              void availability.refetch();
            }}
          >
            Try again
          </Button>
        }
      />
    );
  }

  if (values.data === undefined || availability.data === undefined) {
    return <ProgressBar value={null} aria-label={loadingLabel} />;
  }

  return children({ values: values.data, availability: availability.data, refreshing: values.isFetching });
}
