/**
 * SOURCE OF TRUTH KEYWORDS: SettingsSection, settings card, section card, GlassSurface card of rows
 * WHAT:  One Settings section: a GlassSurface card headed by the registry's section label, holding its rows
 *        separated by hairlines.
 * WHY:   04 §5: "Sections from the registry, each a card of SettingField rows". The heading labels the region, so
 *        screen readers can jump between sections.
 * WHERE: routes/settings/index.tsx, one per section with something to show.
 */
import { useId, type ReactNode } from "react";
import { GlassSurface } from "@/components/global";

export interface SettingsSectionProps {
  readonly label: string;
  readonly children: ReactNode;
}

export function SettingsSection({ label, children }: SettingsSectionProps) {
  const headingId = useId();
  return (
    <GlassSurface asChild className="flex flex-col px-5 py-4">
      <section aria-labelledby={headingId}>
        <h2 id={headingId} className="text-title3 text-fg">
          {label}
        </h2>
        <div className="flex flex-col divide-y divide-separator">{children}</div>
      </section>
    </GlassSurface>
  );
}
