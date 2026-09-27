/**
 * SOURCE OF TRUTH KEYWORDS: DictionaryPage, dictionary route, dictionary page, apply dictionary switch, saved terms, dictionary sidebar page
 * WHAT:  The Dictionary screen: the registry settings of every section this page owns, drawn in registry order: each
 *        Pairs setting as its searchable "Saved terms" card (DictionaryTerms), every other setting (the "Apply
 *        dictionary to transcripts" switch) as a SettingRow in a card of its own.
 * WHY:   The dictionary used to be one row in Settings → Cleanup; it now has its own sidebar page (registry nav entry)
 *        and its own registry section whose `page` is this one, so Settings leaves it out and this page never names a
 *        setting key (root CLAUDE.md §7: settings controls come from the registry). The switch is an ordinary Bool
 *        setting that Rust reads when it builds each take's polish context (off: no swaps, terms kept). Values and
 *        availability come through the shared SettingsReads gate, so loading and errors look like Settings.
 * WHERE: Lazy-loaded by app/routes.tsx for the `dictionary` nav entry (app/nav-page.ts).
 */
import type { NavPageProps } from "@/app/nav-page";
import { GlassSurface, Page, SettingRow, SettingsReads } from "@/components/global";
import { useRegistryView } from "@/hooks";
import { settingsBySection } from "@/lib/settings-layout";
import { DictionaryTerms } from "./_components/DictionaryTerms";

export default function DictionaryPage({ nav }: NavPageProps) {
  const { settings, sections } = useRegistryView();
  return (
    <Page title={nav.label} description="Fix words Echo gets wrong." className="h-full">
      <SettingsReads icon={nav.icon} loadingLabel="Loading your dictionary">
        {({ values, availability, refreshing }) =>
          settingsBySection(sections, settings, availability.caps, nav.id).flatMap(({ settings: shown }) =>
            shown.map((spec) => {
              const value = values.get(spec.key) ?? spec.default;
              return spec.kind.kind === "pairs" ? (
                <DictionaryTerms
                  key={spec.key}
                  spec={spec}
                  kind={spec.kind}
                  value={value}
                  refreshing={refreshing}
                  icon={nav.icon}
                />
              ) : (
                <GlassSurface key={spec.key} className="px-5 py-1">
                  <SettingRow spec={spec} value={value} availability={availability} />
                </GlassSurface>
              );
            }),
          )
        }
      </SettingsReads>
    </Page>
  );
}
