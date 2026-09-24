/*!
 * SOURCE OF TRUTH KEYWORDS: bootstrap, composition root, command context, build adapters, managed state, startup wiring
 * WHAT:  Builds what the running app manages: the CommandCtx with the live settings and the selected adapters.
 * WHY:   The composition root is the only place that names a concrete adapter (02 §3.2); every other layer sees
 *        ports. Settings start from the registry defaults until the database lands (step 06), which then resolves
 *        the stored rows over them here; nothing else changes shape when it does. Logging, paths, the database
 *        and the session actor join this file with their steps.
 * WHERE: Called by app::run before the Tauri builder runs.
 */

use std::sync::Arc;

use crate::{
    adapters::consent::Win32PrivacyConsent, ipc::CommandCtx, registry, types::SharedSettings,
};

/// The context every command handler receives.
pub fn command_ctx() -> CommandCtx {
    CommandCtx::new(
        SharedSettings::new(registry::settings::defaults()),
        Arc::new(Win32PrivacyConsent::new()),
    )
}
