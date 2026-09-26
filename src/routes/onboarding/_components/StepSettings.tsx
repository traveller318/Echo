/**
 * SOURCE OF TRUTH KEYWORDS: StepSettings, onboarding step settings, registry setting rows in onboarding
 * WHAT:  The settings an onboarding step offers (its registry `settings` keys), each as a connected setting row.
 * WHY:   The registry step entry names the settings (registry/onboarding.rs); their controls, validation and inline
 *        errors (a hotkey conflict) are the same generated rows Settings shows, so onboarding never names a setting
 *        or builds a control of its own (root CLAUDE.md §7).
 * WHERE: The microphone and practice steps (this folder).
 */
import type { SettingKey } from "@/bindings";
import { SettingRowFor } from "@/components/global";

export interface StepSettingsProps {
  readonly settings: readonly SettingKey[];
}

export function StepSettings({ settings }: StepSettingsProps) {
  if (settings.length === 0) {
    return null;
  }
  return (
    <div data-slot="step-settings" className="flex flex-col">
      {settings.map((key) => (
        <SettingRowFor key={key} settingKey={key} />
      ))}
    </div>
  );
}
