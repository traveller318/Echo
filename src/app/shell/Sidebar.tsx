/**
 * SOURCE OF TRUTH KEYWORDS: Sidebar, main navigation, registry nav, NavLink, accent-soft selected, sidebar glass, aria-current
 * WHAT:  The main window's sidebar: a `sidebar` GlassSurface --sidebar-width wide, a drag strip with the Echo name
 *        level with the titlebar, then one link per registry nav entry (icon + label) in registry order; the
 *        current page is the --color-accent-soft pill with an accent icon.
 * WHY:   04 §5 builds the sidebar from the registry, so a new page is a registry entry, never an edit here.
 *        NavLink sets aria-current="page" and the active style from the router, so selection has no second source
 *        of truth. Links are plain anchors, so Tab and Enter work without extra code (04 §7); status is not colour
 *        alone because the selected item also gets the filled background.
 * WHERE: app/shell/ShellLayout.tsx; reads hooks/use-registry.ts (useNavItems) and draws icons with NavIcon.
 */
import { NavLink } from "react-router";
import { GlassSurface, NavIcon } from "@/components/global";
import { useNavItems } from "@/hooks";
import { cn } from "@/lib/cn";

export function Sidebar() {
  const items = useNavItems();
  return (
    <GlassSurface variant="sidebar" asChild>
      <nav data-slot="sidebar" aria-label="Main" className="flex w-sidebar shrink-0 flex-col">
        <div data-tauri-drag-region="deep" className="flex h-titlebar shrink-0 items-center px-5 select-none">
          <span className="text-callout font-semibold text-fg">Echo</span>
        </div>
        <ul className="flex flex-col gap-1 px-3 py-2">
          {items.map((item) => (
            <li key={item.id}>
              <NavLink
                to={item.route}
                className={({ isActive }) =>
                  cn(
                    "flex h-control items-center gap-3 rounded-control px-3 text-callout text-fg",
                    "transition-colors duration-(--duration-fast) ease-standard",
                    "[&_svg]:size-icon-md [&_svg]:shrink-0 [&_svg]:text-fg-secondary",
                    isActive ? "bg-accent-soft [&_svg]:text-accent" : "hover:bg-fill-hover active:bg-fill-pressed",
                  )
                }
              >
                <NavIcon icon={item.icon} />
                <span className="truncate">{item.label}</span>
              </NavLink>
            </li>
          ))}
        </ul>
      </nav>
    </GlassSurface>
  );
}
