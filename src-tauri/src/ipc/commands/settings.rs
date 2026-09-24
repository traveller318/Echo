/*!
 * SOURCE OF TRUTH KEYWORDS: settings commands, registry_get, RegistryView, registry lists, settings specs command
 * WHAT:  The settings command group. `registry_get` returns every registry list the UI renders from
 *        (RegistryView: settings, hotkeys, nav, engines, metrics).
 * WHY:   The UI builds its navigation, Settings form, Models page and dashboard layout from the registry and never
 *        hardcodes a list (02 §3.3, root CLAUDE.md §7). The registry is compiled in, so the read needs no
 *        permission, is Shared, and the UI calls it once at startup. `settings_get_all`, `settings_set` and
 *        `settings_reset` join this group with the settings service (step 06).
 * WHERE: Registered through `ipc::commands::catalog`; called from the UI as `commands.registryGet()`.
 */

use crate::{
    ipc::{CommandCtx, factory::echo_command},
    registry::{engines, hotkeys, metrics, nav, settings},
    types::{AppError, RegistryView},
};

echo_command! {
    /// Every registry list the UI renders from. Compiled in, so it never changes while the app runs.
    name: registry_get,
    output: RegistryView,
    permission: None,
    reentrancy: Shared,
    handler: get_registry,
}

/// Builds the RegistryView from the registry lists, in registry order (nav in sidebar order).
pub async fn get_registry(_: &CommandCtx, (): ()) -> Result<RegistryView, AppError> {
    Ok(RegistryView {
        settings: settings::SETTINGS.to_vec(),
        hotkeys: hotkeys::HOTKEYS.to_vec(),
        nav: nav::items().into_iter().cloned().collect(),
        engines: engines::specs(),
        metrics: metrics::METRICS.to_vec(),
    })
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, task::Poll};

    use super::*;
    use crate::{
        ipc::factory,
        ports::fakes::{FakePrivacyConsent, poll_once},
        types::{CommandSpec, Reentrancy, SharedSettings},
    };

    #[test]
    fn registry_view_carries_every_registry_list_in_order() {
        let ctx = CommandCtx::new(
            SharedSettings::new(settings::defaults()),
            Arc::new(FakePrivacyConsent::granted()),
        );
        let spec = CommandSpec {
            name: "registry_get",
            permission: None,
            reentrancy: Reentrancy::Shared,
        };
        let Poll::Ready(Ok(view)) = poll_once(factory::run(&ctx, &spec, (), get_registry)) else {
            panic!("registry_get did not return a view");
        };
        assert_eq!(view.settings, settings::SETTINGS);
        assert_eq!(view.hotkeys, hotkeys::HOTKEYS);
        assert_eq!(view.metrics, metrics::METRICS);
        assert_eq!(view.engines, engines::specs());
        let nav: Vec<&str> = view.nav.iter().map(|item| item.id.as_str()).collect();
        assert_eq!(nav, ["dashboard", "history", "models", "settings"]);
    }
}
