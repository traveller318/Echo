/*!
 * SOURCE OF TRUTH KEYWORDS: RegistryView, registry_get, registry snapshot, settings specs, nav items, engine specs, metric specs, hotkey specs
 * WHAT:  RegistryView: every registry list the UI renders from (settings, hotkeys, nav, engines, metrics), as
 *        returned by the `registry_get` command.
 * WHY:   The UI builds its sidebar, router, Settings form, Models page and dashboard layout from the registry
 *        (02 §3.3), so it asks once at startup and never hardcodes a list. The registry is compiled in and does
 *        not change while the app runs, so one read is enough; values that do change (settings, engine status)
 *        come from their own commands and events. Hidden settings (`visible: false`) are included, because the
 *        UI filters on `visible` itself and onboarding reads hidden ones. Lists are in registry order (nav in
 *        sidebar order).
 * WHERE: Built by ipc/commands/settings.rs (`registry_get`) from registry/; read by the UI through the generated
 *        bindings.
 */

use serde::{Deserialize, Serialize};
use specta::Type;

use super::{EngineSpec, HotkeySpec, MetricSpec, NavItem, SettingSpec};

/// Every registry list the UI renders from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct RegistryView {
    pub settings: Vec<SettingSpec>,
    pub hotkeys: Vec<HotkeySpec>,
    pub nav: Vec<NavItem>,
    pub engines: Vec<EngineSpec>,
    pub metrics: Vec<MetricSpec>,
}
