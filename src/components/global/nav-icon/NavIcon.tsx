/**
 * SOURCE OF TRUTH KEYWORDS: NavIcon, NAV_ICONS, registry nav icon, lucide icon map, sidebar icon, page icon
 * WHAT:  Draws the lucide icon a registry nav entry names (`icon: NavIcon`), decorative (aria-hidden) unless a
 *        label is given.
 * WHY:   The registry names icons as a closed union (types/nav.rs), so this map is checked for exhaustiveness by
 *        tsc: a new icon in Rust fails the build until it is drawable here. Size and stroke come from the
 *        surrounding tokens (04 §3.9: --size-icon-md in nav, stroke from globals.css), so the icon itself carries
 *        no visual values.
 * WHERE: app/shell/Sidebar.tsx (nav items) and the route pages (their EmptyState icon). Exported through
 *        components/global/index.ts.
 */
import type { LucideIcon, LucideProps } from "lucide-react";
import { BoxesIcon, HistoryIcon, LayoutDashboardIcon, SettingsIcon } from "lucide-react";
import type { NavIcon as NavIconName } from "@/bindings";

const NAV_ICONS: Readonly<Record<NavIconName, LucideIcon>> = {
  "layout-dashboard": LayoutDashboardIcon,
  history: HistoryIcon,
  boxes: BoxesIcon,
  settings: SettingsIcon,
};

export type NavIconProps = Omit<LucideProps, "ref"> & {
  readonly icon: NavIconName;
};

export function NavIcon({ icon, ...props }: NavIconProps) {
  const Icon = NAV_ICONS[icon];
  const decorative = props["aria-label"] === undefined;
  return <Icon data-slot="nav-icon" aria-hidden={decorative} {...props} />;
}
