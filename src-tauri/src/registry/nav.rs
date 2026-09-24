/*!
 * SOURCE OF TRUTH KEYWORDS: nav registry, NAV, sidebar entries, dashboard, history, models, settings, route order
 * WHAT:  The main window's sidebar entries (dashboard, history, models, settings) with label, icon, route and
 *        order, and the list in sidebar order.
 * WHY:   The sidebar and router are built from these (02 §3.3), so adding a page is a NavId variant, an entry here
 *        and its route folder (src/routes/<id>/, which tsc demands through the generated NavId union). Onboarding is a route, not a nav item: it is reached on first run, never from the sidebar.
 * WHERE: Sent to the UI by `registry_get`; rendered by src/app/shell (sidebar) and src/app/routes.tsx (routes).
 */

use crate::types::{NavIcon, NavId, NavItem, StaticStr};

/// Every sidebar entry.
pub const NAV: &[NavItem] = &[
    NavItem {
        id: NavId::Dashboard,
        label: StaticStr::new("Dashboard"),
        icon: NavIcon::LayoutDashboard,
        route: StaticStr::new("/"),
        order: 0,
    },
    NavItem {
        id: NavId::History,
        label: StaticStr::new("History"),
        icon: NavIcon::History,
        route: StaticStr::new("/history"),
        order: 1,
    },
    NavItem {
        id: NavId::Models,
        label: StaticStr::new("Models"),
        icon: NavIcon::Boxes,
        route: StaticStr::new("/models"),
        order: 2,
    },
    NavItem {
        id: NavId::Settings,
        label: StaticStr::new("Settings"),
        icon: NavIcon::Settings,
        route: StaticStr::new("/settings"),
        order: 3,
    },
];

/// Every entry in sidebar order.
pub fn items() -> Vec<&'static NavItem> {
    let mut items: Vec<&'static NavItem> = NAV.iter().collect();
    items.sort_by_key(|item| item.order);
    items
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;
    use crate::registry::tests::is_registry_id;

    #[test]
    fn ids_routes_and_order_are_unique() {
        let (mut ids, mut routes, mut orders) = (HashSet::new(), HashSet::new(), HashSet::new());
        for item in NAV {
            let id = item.id.as_str();
            assert!(is_registry_id(id), "{id}");
            assert!(ids.insert(id), "duplicate nav id {id}");
            assert!(item.route.starts_with('/'), "{id} route");
            assert!(
                routes.insert(item.route.as_str()),
                "duplicate route {}",
                item.route
            );
            assert!(
                orders.insert(item.order),
                "duplicate nav order {}",
                item.order
            );
        }
    }

    #[test]
    fn sidebar_lists_the_four_pages_and_not_onboarding() {
        let order: Vec<&str> = items().iter().map(|item| item.id.as_str()).collect();
        assert_eq!(order, ["dashboard", "history", "models", "settings"]);
        assert!(NAV.iter().all(|item| item.route.as_str() != "/onboarding"));
    }
}
