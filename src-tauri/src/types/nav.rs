/*!
 * SOURCE OF TRUTH KEYWORDS: NavItem, NavId, NavIcon, sidebar navigation, nav entry, route, nav order, lucide icon, route page
 * WHAT:  A sidebar navigation entry (NavItem: id, label, icon, route, order), the closed set of pages an entry can
 *        open (NavId) and the closed set of icons it can use (NavIcon, serialized as the lucide-react icon name).
 * WHY:   The sidebar and the router are built from registry nav entries (02 §3.3), so adding a page is an entry,
 *        not a shell change. The UI must own a React page for every entry and a drawing for every icon, so both
 *        are enums, not free text: the generated TS types are string unions and the UI's page and icon maps are
 *        checked for exhaustiveness by tsc. An entry can never name a page or an icon the UI cannot render, and
 *        a new NavId fails the frontend build until its route folder exists.
 * WHERE: Entries in registry/nav; sent to the UI by `registry_get`; rendered by src/app/shell (sidebar),
 *        src/app/routes.tsx (routes) and src/app/nav-page.ts (pages keyed by NavId).
 */

use serde::{Deserialize, Serialize};
use specta::Type;

use super::StaticStr;

/// Registry id of a sidebar navigation item; each one has a page in `src/routes/<id>/`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "kebab-case")]
pub enum NavId {
    Dashboard,
    History,
    Models,
    Settings,
}

impl NavId {
    /// The wire value, which is also the route folder name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Dashboard => "dashboard",
            Self::History => "history",
            Self::Models => "models",
            Self::Settings => "settings",
        }
    }
}

/**
 * SOURCE OF TRUTH KEYWORDS: OpenPageInput, app_open_page input, open main window on a page, pill Set up, pill Open
 * WHAT:  The input of `app_open_page`: which main-window page to show.
 * WHY:   The pill has no router and no rights on the main window (capabilities/pill.json), so its "Set up" and
 *        "Open" buttons ask Rust to bring the main window forward on a page; the page is a NavId, so only pages the
 *        registry has can be named, and serde already refuses anything else (nothing left for garde to check).
 * WHERE: ipc/commands/system.rs (`app_open_page`); sent by the pill (src/pill) and any surface without the router.
 */
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type, garde::Validate,
)]
pub struct OpenPageInput {
    #[garde(skip)]
    pub page: NavId,
}

/// A sidebar icon, serialized as its lucide-react name (e.g. `layout-dashboard`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "kebab-case")]
pub enum NavIcon {
    LayoutDashboard,
    History,
    Boxes,
    Settings,
}

/// A registry navigation entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct NavItem {
    pub id: NavId,
    pub label: StaticStr,
    pub icon: NavIcon,
    /// Router path, starting with `/`.
    pub route: StaticStr,
    /// Position in the sidebar, ascending; unique across entries.
    pub order: u16,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn items_serialize_with_kebab_ids_and_lucide_icon_names() {
        const ITEM: NavItem = NavItem {
            id: NavId::Dashboard,
            label: StaticStr::new("Dashboard"),
            icon: NavIcon::LayoutDashboard,
            route: StaticStr::new("/"),
            order: 0,
        };
        assert_eq!(
            serde_json::to_value(&ITEM).unwrap(),
            json!({
                "id": "dashboard",
                "label": "Dashboard",
                "icon": "layout-dashboard",
                "route": "/",
                "order": 0,
            })
        );
    }

    #[test]
    fn as_str_matches_the_wire_value() {
        for id in [
            NavId::Dashboard,
            NavId::History,
            NavId::Models,
            NavId::Settings,
        ] {
            assert_eq!(serde_json::to_value(id).unwrap(), json!(id.as_str()));
        }
    }
}
