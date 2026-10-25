/*!
 * SOURCE OF TRUTH KEYWORDS: register_app, host portal registry, org.freedesktop.host.portal.Registry, portal app id, unsandboxed app identity, ashpd shared connection
 * WHAT:  `register_app`: tells xdg-desktop-portal that this process is the application `app_id` (Echo's Tauri
 *        identifier), once per process; later calls return at once.
 * WHY:   A portal learns who is asking from the sandbox (Flatpak, Snap); a normally installed app has no identity, so
 *        the portal stores nothing for it: the desktop's permission for typing, a remembered shortcut, a keybinding
 *        named `app.echo.desktop:record` all hang off this id. ashpd keeps one D-Bus connection for the whole process,
 *        and the registry accepts one registration per connection, so every portal user (hotkeys, paste) funnels
 *        through here. A desktop without the registry interface (older xdg-desktop-portal) simply keeps working
 *        without an id, so a failure is logged at debug and never stops a feature.
 *        The portal also needs a desktop file named after the id (desktop_entry.rs), made first.
 * WHERE: adapters/hotkey/portal_shortcuts.rs and adapters/inserter/portal_remote_desktop.rs, before their first portal call.
 */

use ashpd::AppID;
use tokio::sync::OnceCell;

static REGISTERED: OnceCell<()> = OnceCell::const_new();

pub async fn register_app(app_id: &str) {
    REGISTERED
        .get_or_init(|| async {
            super::desktop_entry::ensure(app_id);
            let id = match AppID::try_from(app_id) {
                Ok(id) => id,
                Err(error) => {
                    tracing::warn!(%error, app_id, "the app id is not a valid portal id");
                    return;
                }
            };
            if let Err(error) = ashpd::register_host_app(id).await {
                tracing::warn!(%error, "the portal registry did not take Echo's app id");
            }
        })
        .await;
}
