/*!
 * SOURCE OF TRUTH KEYWORDS: NavItem, NavIcon, sidebar navigation, nav entry, route, nav order, lucide icon
 * WHAT:  A sidebar navigation entry (NavItem: id, label, icon, route, order) and the closed set of icons an entry
 *        can use (NavIcon, serialized as the lucide-react icon name).
 * WHY:   The sidebar and the router are built from registry nav entries (02 §3.3), so adding a page is an entry,
 *        not a component change. The icon is an enum, not free text, so the generated TS type is a string union
 *        and the UI's icon map is checked for exhaustiveness by tsc: an entry can never name an icon the UI
 *        cannot draw.
 * WHERE: Entries in registry/nav; sent to the UI by `registry_get`; rendered by src/app/shell (sidebar) and
 *        src/app/router.tsx.
 */

use serde::{Deserialize, Serialize};
use specta::Type;

use super::{NavId, StaticStr};

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
    fn items_serialize_with_lucide_icon_names() {
        const ITEM: NavItem = NavItem {
            id: NavId::from_static("dashboard"),
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
}
