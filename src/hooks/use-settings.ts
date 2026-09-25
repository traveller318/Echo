/**
 * SOURCE OF TRUTH KEYWORDS: useSettingValues, useSettingsAvailability, useAudioDevices, useSettingOptions, useSettingWrite, SETTINGS_QUERY, SETTINGS_AVAILABILITY_QUERY, AUDIO_DEVICES_QUERY, AUDIO_DEVICE_EVENTS, settings_set, settings_reset
 * WHAT:  The Settings data layer: every effective value (`settings_get_all`, as a map by key), what the page may
 *        offer now (`settings_availability`: caps that hold, each choice setting's options), the microphones
 *        (`audio_list_devices`), the choices of one row (`useSettingOptions`) and `useSettingWrite(key)`, which
 *        sets or resets one setting and keeps its failure.
 * WHY:   Rust owns settings (root CLAUDE.md §7): values and availability are read once and refetched on
 *        SettingsChanged, never polled and never copied into a store; a write changes nothing in the cache by hand,
 *        its SettingsChanged refetches (a new engine can bring new language options, so availability follows the
 *        same event). A write's failure is kept per setting and shown inline on its row (a hotkey conflict, a value
 *        Rust refused), so it does not toast. The microphone list refetches on AudioDevicesChanged, which Rust sends
 *        once per real hot-plug change (pipeline/audio_devices.rs); it is also read again when the list is opened,
 *        which covers a PC where Windows' device notifications could not be watched.
 * WHERE: routes/settings (the page and its rows); onboarding (step 24) reuses the same reads and writes.
 */
import type { UseQueryResult } from "@tanstack/react-query";
import { useCallback, useMemo } from "react";
import {
  commands,
  type AppError,
  type AudioDevice,
  type EnumOption,
  type SettingEntry,
  type SettingKey,
  type SettingSpec,
  type SettingValue,
  type SettingsAvailability,
} from "@/bindings";
import { toAppError } from "@/lib/app-error";
import type { EchoEventName } from "@/lib/echo-events";
import { useEchoMutation } from "./use-echo-mutation";
import { useEchoQuery, type EchoQuery } from "./use-echo-query";

/** Rust events after which settings reads are stale. */
export const SETTINGS_EVENTS: readonly EchoEventName[] = ["settingsChanged"];

export const SETTINGS_QUERY: EchoQuery<SettingEntry[]> = {
  queryKey: ["settings", "values"],
  command: commands.settingsGetAll,
  invalidatedBy: SETTINGS_EVENTS,
};

export const SETTINGS_AVAILABILITY_QUERY: EchoQuery<SettingsAvailability> = {
  queryKey: ["settings", "availability"],
  command: commands.settingsAvailability,
  invalidatedBy: SETTINGS_EVENTS,
};

/** Rust events after which the microphone list is stale. */
export const AUDIO_DEVICE_EVENTS: readonly EchoEventName[] = ["audioDevicesChanged"];

export const AUDIO_DEVICES_QUERY: EchoQuery<AudioDevice[]> = {
  queryKey: ["audio", "devices"],
  command: commands.audioListDevices,
  invalidatedBy: AUDIO_DEVICE_EVENTS,
};

/** The effective values by key. */
export type SettingValues = ReadonlyMap<SettingKey, SettingValue>;

function byKey(entries: readonly SettingEntry[]): SettingValues {
  return new Map(entries.map((entry) => [entry.key, entry.value]));
}

export function useSettingValues(): UseQueryResult<SettingValues> {
  return useEchoQuery(SETTINGS_QUERY, { select: byKey });
}

export function useSettingsAvailability(): UseQueryResult<SettingsAvailability> {
  return useEchoQuery(SETTINGS_AVAILABILITY_QUERY);
}

/** The microphones Windows has now; `enabled` false skips the read (a row that is not shown). */
export function useAudioDevices(enabled = true): UseQueryResult<AudioDevice[]> {
  return useEchoQuery(AUDIO_DEVICES_QUERY, { enabled });
}

/**
 * SOURCE OF TRUTH KEYWORDS: useSettingOptions, setting choices, device options, enum options offered, refresh microphones
 * WHAT:  The choices a setting row offers: an Enum's options from `settings_availability`, a Device's microphones
 *        from `audio_list_devices` (as options: id → name), nothing for the other kinds; plus `refresh` for a list
 *        that is read again when it opens (microphones, as a fallback to their change event).
 * WHY:   Where a kind's choices come from is decided once here, so SettingField stays data-agnostic and the page
 *        never matches on a setting key. The microphone read runs only for a Device row.
 * WHERE: routes/settings rows; onboarding's microphone step (step 24).
 */
export interface SettingRowOptions {
  readonly options: readonly EnumOption[];
  readonly refresh?: () => void;
}

const NO_OPTIONS: readonly EnumOption[] = [];

export function useSettingOptions(spec: SettingSpec, availability: SettingsAvailability | undefined): SettingRowOptions {
  const isDevice = spec.kind.kind === "device";
  const devices = useAudioDevices(isDevice);
  const { refetch } = devices;
  const microphones = useMemo<readonly EnumOption[]>(
    () => (devices.data ?? []).map((device) => ({ value: device.id, label: device.name, requires: null })),
    [devices.data],
  );
  const refresh = useCallback(() => {
    void refetch();
  }, [refetch]);
  if (isDevice) {
    return { options: microphones, refresh };
  }
  return { options: availability?.options.find((entry) => entry.key === spec.key)?.options ?? NO_OPTIONS };
}

/** A write to one setting: a new value, or back to its default. */
export type SettingChange = { readonly kind: "set"; readonly value: SettingValue } | { readonly kind: "reset" };

export interface SettingWrite {
  readonly set: (value: SettingValue) => void;
  readonly reset: () => void;
  readonly pending: boolean;
  /** Why the last write failed; null after a success or `clearError`. */
  readonly error: AppError | null;
  readonly clearError: () => void;
}

export function useSettingWrite(key: SettingKey): SettingWrite {
  const mutation = useEchoMutation(
    (change: SettingChange) =>
      change.kind === "set" ? commands.settingsSet({ key, value: change.value }) : commands.settingsReset({ key }),
    { toastOnError: false },
  );
  return {
    set: (value) => {
      mutation.mutate({ kind: "set", value });
    },
    reset: () => {
      mutation.mutate({ kind: "reset" });
    },
    pending: mutation.isPending,
    error: mutation.error === null ? null : toAppError(mutation.error),
    clearError: mutation.reset,
  };
}
