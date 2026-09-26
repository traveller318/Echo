/**
 * SOURCE OF TRUTH KEYWORDS: SettingRow, SettingRowFor, connected setting field, setting write row, settings page row, setting by key, hotkey capture lease
 * WHAT:  One setting row wired to Rust: a SettingField for `spec` with the value in effect, the choices it offers
 *        now, and writes through `settings_set` / `settings_reset` whose failure shows inline on the row.
 *        `SettingRowFor` finds the spec, its value and availability itself from a setting key.
 * WHY:   SettingField is data-agnostic; this is the one place a surface connects it to the settings commands, so every
 *        row behaves the same wherever it appears (Settings, onboarding's microphone and hotkey steps). A new attempt
 *        clears the previous failure first, so an old error never sits under a value that was since saved. A surface
 *        that shows a single setting names it by its registry key and never copies its spec or value
 *        (root CLAUDE.md §7: settings controls come from the registry); it renders nothing until the reads answer.
 *        A hotkey row holds the capture lease while its field captures, so Echo's own hotkeys stay out of the way.
 * WHERE: routes/settings/index.tsx (one SettingRow per shown setting); routes/onboarding steps (SettingRowFor).
 */
import type { SettingKey, SettingSpec, SettingValue, SettingsAvailability } from "@/bindings";
import {
  useHotkeyCaptureLease,
  useRegistryView,
  useSettingOptions,
  useSettingsAvailability,
  useSettingValues,
  useSettingWrite,
} from "@/hooks";
import { SettingField } from "../setting-field";

export interface SettingRowProps {
  readonly spec: SettingSpec;
  readonly value: SettingValue;
  readonly availability: SettingsAvailability;
  readonly className?: string;
}

export function SettingRow({ spec, value, availability, className }: SettingRowProps) {
  const write = useSettingWrite(spec.key);
  const { options, refresh } = useSettingOptions(spec, availability);
  const captureLease = useHotkeyCaptureLease();
  return (
    <SettingField
      spec={spec}
      value={value}
      options={options}
      error={write.error}
      pending={write.pending}
      onOptionsOpen={refresh}
      onCaptureChange={captureLease}
      className={className}
      onCommit={(next) => {
        write.clearError();
        write.set(next);
      }}
      onReset={() => {
        write.clearError();
        write.reset();
      }}
    />
  );
}

export interface SettingRowForProps {
  /** The registry key of the setting to show. */
  readonly settingKey: SettingKey;
  readonly className?: string;
}

export function SettingRowFor({ settingKey, className }: SettingRowForProps) {
  const { settings } = useRegistryView();
  const values = useSettingValues();
  const availability = useSettingsAvailability();
  const spec = settings.find((entry) => entry.key === settingKey);
  if (spec === undefined || values.data === undefined || availability.data === undefined) {
    return null;
  }
  return (
    <SettingRow
      spec={spec}
      value={values.data.get(spec.key) ?? spec.default}
      availability={availability.data}
      className={className}
    />
  );
}
