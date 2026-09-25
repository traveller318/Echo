/**
 * SOURCE OF TRUTH KEYWORDS: settingsBySection, settings page layout, visible settings, caps-gated settings, hidden settings, section grouping
 * WHAT:  Groups the settings the page shows by registry section, in registry order: a setting is shown only when it
 *        is `visible` and its caps requirement holds now; a section with nothing to show is left out.
 * WHY:   Hidden settings are internal state kept as settings (onboarding completion), and a setting whose capability
 *        is missing (the language picker with one language, the accelerator without a GPU, update checks without an
 *        update source) is not offered at all (02 §3.4). Both rules read registry data and the caps Rust reports,
 *        never a setting key, so a new setting or requirement needs no change here. Pure, so it is tested alone.
 * WHERE: routes/settings/index.tsx.
 */
import type { CapsRequirement, SettingSectionSpec, SettingSpec } from "@/bindings";

export interface SettingsSectionGroup {
  readonly section: SettingSectionSpec;
  readonly settings: readonly SettingSpec[];
}

/** Whether the page shows `spec` while `caps` hold. */
export function isSettingShown(spec: SettingSpec, caps: readonly CapsRequirement[]): boolean {
  return spec.visible && (spec.requires === null || caps.includes(spec.requires));
}

export function settingsBySection(
  sections: readonly SettingSectionSpec[],
  settings: readonly SettingSpec[],
  caps: readonly CapsRequirement[],
): SettingsSectionGroup[] {
  return sections
    .map((section) => ({
      section,
      settings: settings.filter((spec) => spec.section === section.section && isSettingShown(spec, caps)),
    }))
    .filter((group) => group.settings.length > 0);
}
