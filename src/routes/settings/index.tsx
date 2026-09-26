/**
 * SOURCE OF TRUTH KEYWORDS: SettingsPage, settings route, settings page, registry-generated settings, settings sections, settings availability
 * WHAT:  The Settings screen (04 §5): a notice for each selected engine whose model is not ready (grammar polish
 *        downloading after it was switched on), then one card per registry section, each holding a SettingField row
 *        per setting the page may show (visible, and its caps requirement holds). Every row saves on its own and
 *        takes effect at once (or says it needs a restart).
 * WHY:   The page is generated from the registry (root CLAUDE.md §7): specs and section headings come from
 *        RegistryView, values from `settings_get_all`, choices and caps from `settings_availability`; the page
 *        keeps no copy of any of them and stays fresh from SettingsChanged (a change in another window, or a new
 *        engine's languages). Adding a setting is a registry entry with no change here. Loading shows an
 *        indeterminate bar only after --delay-loading (ProgressBar), so a fast read never flashes.
 * WHERE: Lazy-loaded by app/routes.tsx for the `settings` nav entry (app/nav-page.ts).
 */
import type { NavPageProps } from "@/app/nav-page";
import { EmptyState, NavIcon, Page, ProgressBar, SettingRow } from "@/components/global";
import { Button } from "@/components/ui";
import { useRegistryView, useSettingsAvailability, useSettingValues } from "@/hooks";
import { describeAppError, toAppError } from "@/lib/app-error";
import { ModelSetupNotices } from "./_components/ModelSetupNotices";
import { SettingsSection } from "./_components/SettingsSection";
import { settingsBySection } from "./_components/settings-layout";

export default function SettingsPage({ nav }: NavPageProps) {
  const { settings, sections } = useRegistryView();
  const values = useSettingValues();
  const availability = useSettingsAvailability();

  if (values.isError || availability.isError) {
    const copy = describeAppError(toAppError(values.error ?? availability.error));
    return (
      <Page title={nav.label}>
        <EmptyState
          icon={<NavIcon icon={nav.icon} />}
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
      </Page>
    );
  }

  if (values.data === undefined || availability.data === undefined) {
    return (
      <Page title={nav.label}>
        <ProgressBar value={null} aria-label="Loading settings" />
      </Page>
    );
  }

  const current = values.data;
  const offered = availability.data;
  return (
    <Page title={nav.label}>
      <ModelSetupNotices />
      {settingsBySection(sections, settings, offered.caps).map(({ section, settings: shown }) => (
        <SettingsSection key={section.section} label={section.label}>
          {shown.map((spec) => (
            <SettingRow key={spec.key} spec={spec} value={current.get(spec.key) ?? spec.default} availability={offered} />
          ))}
        </SettingsSection>
      ))}
    </Page>
  );
}
