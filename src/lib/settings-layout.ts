/**
 * SOURCE OF TRUTH KEYWORDS: settingsBySection, isSettingShown, settings page layout, visible settings, caps-gated settings, hidden settings, section grouping, settings of a page
 * WHAT:  Groups the settings one page shows by registry section, in registry order: only the sections whose `page` is
 *        that page, and in them only the settings that are `visible` with their caps requirement holding now; a
 *        section with nothing to show is left out.
 * WHY:   Hidden settings are internal state kept as settings (onboarding completion), and a setting whose capability
 *        is missing (the language picker with one language, the accelerator without a GPU, update checks without an
 *        update source) is not offered at all (02 §3.4). A section names the page that shows it (Settings, or the
 *        Dictionary page for its own switch and terms), so no page filters by a setting key. Every rule reads registry
 *        data and the caps Rust reports, so a new setting, section or requirement needs no change here. Pure, so it is
 *        tested alone.
 * WHERE: routes/settings/index.tsx and routes/dictionary/index.tsx.
 */
import type { CapsRequirement, NavId, SettingSectionSpec, SettingSpec } from "@/bindings";

export interface SettingsSectionGroup {
  readonly section: SettingSectionSpec;
  readonly settings: readonly SettingSpec[];
}

/** Whether a page shows `spec` while `caps` hold. */
export function isSettingShown(spec: SettingSpec, caps: readonly CapsRequirement[]): boolean {
  return spec.visible && (spec.requires === null || caps.includes(spec.requires));
}

export function settingsBySection(
  sections: readonly SettingSectionSpec[],
  settings: readonly SettingSpec[],
  caps: readonly CapsRequirement[],
  page: NavId,
): SettingsSectionGroup[] {
  return sections
    .filter((section) => section.page === page)
    .map((section) => ({
      section,
      settings: settings.filter((spec) => spec.section === section.section && isSettingShown(spec, caps)),
    }))
    .filter((group) => group.settings.length > 0);
}
