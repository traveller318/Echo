/**
 * SOURCE OF TRUTH KEYWORDS: SettingRow, connected setting field, setting write row, settings page row
 * WHAT:  One Settings row wired to Rust: a SettingField for `spec` with the value in effect, the choices it offers
 *        now, and writes through `settings_set` / `settings_reset` whose failure shows inline on the row.
 * WHY:   SettingField is data-agnostic; this is the one place the page connects it to the settings commands, so every
 *        row behaves the same. A new attempt clears the previous failure first, so an old error never sits under a
 *        value that was since saved.
 * WHERE: routes/settings/index.tsx, one per shown setting.
 */
import type { SettingSpec, SettingValue, SettingsAvailability } from "@/bindings";
import { SettingField } from "@/components/global";
import { useSettingOptions, useSettingWrite } from "@/hooks";

export interface SettingRowProps {
  readonly spec: SettingSpec;
  readonly value: SettingValue;
  readonly availability: SettingsAvailability;
}

export function SettingRow({ spec, value, availability }: SettingRowProps) {
  const write = useSettingWrite(spec.key);
  const { options, refresh } = useSettingOptions(spec, availability);
  return (
    <SettingField
      spec={spec}
      value={value}
      options={options}
      error={write.error}
      pending={write.pending}
      onOptionsOpen={refresh}
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
