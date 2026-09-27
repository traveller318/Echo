/**
 * SOURCE OF TRUTH KEYWORDS: SettingsPage, settings route, settings page, registry-generated settings, settings sections, settings availability, hotkeys paused notice
 * WHAT:  The Settings screen (04 §5): a notice while Echo's hotkeys are paused, a notice for each selected engine
 *        whose model is not ready (grammar polish downloading after it was switched on), then one card per registry
 *        section this page owns (a section on its own page, the dictionary, is not here), each holding a SettingField
 *        row per setting the page may show (visible, and its caps requirement holds). Every row saves on its own and takes effect at once (or says it needs a restart).
 * WHY:   The page is generated from the registry (root CLAUDE.md §7): specs and section headings come from
 *        RegistryView, values from `settings_get_all`, choices and caps from `settings_availability` (both read by
 *        the shared SettingsReads gate, which also draws the loading and error states); the page keeps no copy of any
 *        of them and stays fresh from SettingsChanged (a change in another window, or a new engine's languages).
 *        Adding a setting is a registry entry with no change here.
 * WHERE: Lazy-loaded by app/routes.tsx for the `settings` nav entry (app/nav-page.ts).
 */
import type { NavPageProps } from "@/app/nav-page";
import { Page, SettingRow, SettingsReads } from "@/components/global";
import { useRegistryView } from "@/hooks";
import { settingsBySection } from "@/lib/settings-layout";
import { HotkeyPauseNotice } from "./_components/HotkeyPauseNotice";
import { ModelSetupNotices } from "./_components/ModelSetupNotices";
import { SettingsSection } from "./_components/SettingsSection";

export default function SettingsPage({ nav }: NavPageProps) {
  const { settings, sections } = useRegistryView();
  return (
    <Page title={nav.label}>
      <SettingsReads icon={nav.icon} loadingLabel="Loading settings">
        {({ values, availability }) => (
          <>
            <HotkeyPauseNotice />
            <ModelSetupNotices />
            {settingsBySection(sections, settings, availability.caps, nav.id).map(({ section, settings: shown }) => (
              <SettingsSection key={section.section} label={section.label}>
                {shown.map((spec) => (
                  <SettingRow
                    key={spec.key}
                    spec={spec}
                    value={values.get(spec.key) ?? spec.default}
                    availability={availability}
                  />
                ))}
              </SettingsSection>
            ))}
          </>
        )}
      </SettingsReads>
    </Page>
  );
}
