/*!
 * SOURCE OF TRUTH KEYWORDS: system commands, appearance_get, app_open_logs_dir, app_open_mic_privacy_settings, AppearanceView, logs folder, microphone privacy settings
 * WHAT:  The system command group (02 §4.3). `appearance_get` returns the AppearanceView (theme, transparency,
 *        backdrop) both windows paint from; `app_open_logs_dir` shows the local log folder and
 *        `app_open_mic_privacy_settings` opens the Windows microphone privacy page.
 * WHY:   The UI reads appearance once through this command and then stays fresh from AppearanceChanged, never
 *        polling (02 §4.4); the view is computed on demand from the live settings and the SystemAppearance port,
 *        so there is no cached copy to drift. The two "open" commands are the targets of the `open_logs` and
 *        `open_mic_privacy` AppError actions (src/lib/app-error.ts), so every error toast action works; they go
 *        through the SystemLauncher port and the resolved AppPaths, so no handler names a Windows URI or a path.
 *        The update commands join this group with their step.
 * WHERE: Registered through `ipc::commands::catalog`; called from the UI as `commands.appearanceGet()` by
 *        src/lib/appearance.ts and as `commands.appOpenLogsDir()` / `commands.appOpenMicPrivacySettings()` by the
 *        app shell's error actions (src/app/shell/use-app-error-action.ts), later by onboarding and About.
 */

use crate::{
    ipc::{CommandCtx, factory::echo_command},
    pipeline::appearance,
    types::{AppError, AppearanceView, PortError, SettingsPage},
};

echo_command! {
    /// The theme, transparency and backdrop both windows paint from right now.
    name: appearance_get,
    output: AppearanceView,
    permission: None,
    reentrancy: Shared,
    handler: get_appearance,
}

echo_command! {
    /// Shows the folder that holds Echo's local log files.
    name: app_open_logs_dir,
    output: (),
    permission: None,
    reentrancy: Shared,
    handler: open_logs_dir,
}

echo_command! {
    /// Opens the Windows privacy page where microphone access for desktop apps is turned on.
    name: app_open_mic_privacy_settings,
    output: (),
    permission: None,
    reentrancy: Shared,
    handler: open_mic_privacy_settings,
}

/// The appearance view for the settings in effect and the current Windows switches.
pub async fn get_appearance(ctx: &CommandCtx, (): ()) -> Result<AppearanceView, AppError> {
    Ok(appearance::current(&ctx.settings(), ctx.appearance()))
}

/// Hands the logs folder to the system file manager.
pub async fn open_logs_dir(ctx: &CommandCtx, (): ()) -> Result<(), PortError> {
    ctx.launcher().open_folder(&ctx.paths().logs_dir())
}

/// Hands the microphone privacy page to the system settings app.
pub async fn open_mic_privacy_settings(ctx: &CommandCtx, (): ()) -> Result<(), PortError> {
    ctx.launcher()
        .open_settings_page(SettingsPage::MicrophonePrivacy)
}

#[cfg(test)]
mod tests {
    use std::task::Poll;

    use super::*;
    use crate::{
        ipc::{factory, testing},
        ports::fakes::{FakePrivacyConsent, LaunchCall, poll_once},
        registry,
        types::{Backdrop, CommandSpec, Reentrancy, ThemePreference, Transparency},
    };

    const fn spec(name: &'static str) -> CommandSpec {
        CommandSpec {
            name,
            permission: None,
            reentrancy: Reentrancy::Shared,
        }
    }

    const SPEC: CommandSpec = spec("appearance_get");

    fn run(ctx: &CommandCtx) -> AppearanceView {
        let Poll::Ready(Ok(view)) = poll_once(factory::run(ctx, &SPEC, (), get_appearance)) else {
            panic!("appearance_get did not return a view");
        };
        view
    }

    #[test]
    fn returns_the_live_view_and_follows_the_transparency_switch() {
        let harness = testing::harness(
            registry::settings::defaults(),
            FakePrivacyConsent::granted(),
        );
        assert_eq!(
            run(&harness.ctx),
            AppearanceView {
                theme: ThemePreference::System,
                transparency: Transparency::Full,
                backdrop: Backdrop::Mica,
            }
        );
        harness.appearance.switch(Transparency::Reduced);
        let reduced = run(&harness.ctx);
        assert_eq!(reduced.transparency, Transparency::Reduced);
        assert_eq!(reduced.backdrop, Backdrop::Solid);
    }

    #[test]
    fn open_logs_dir_shows_the_resolved_logs_folder() {
        let harness = testing::harness(
            registry::settings::defaults(),
            FakePrivacyConsent::granted(),
        );
        let result = poll_once(factory::run(
            &harness.ctx,
            &spec("app_open_logs_dir"),
            (),
            open_logs_dir,
        ));
        assert_eq!(result, Poll::Ready(Ok(())));
        assert_eq!(
            harness.launcher.calls(),
            [LaunchCall::Folder(harness.ctx.paths().logs_dir())]
        );
    }

    #[test]
    fn open_mic_privacy_settings_opens_the_microphone_page() {
        let harness = testing::harness(
            registry::settings::defaults(),
            FakePrivacyConsent::granted(),
        );
        let result = poll_once(factory::run(
            &harness.ctx,
            &spec("app_open_mic_privacy_settings"),
            (),
            open_mic_privacy_settings,
        ));
        assert_eq!(result, Poll::Ready(Ok(())));
        assert_eq!(
            harness.launcher.calls(),
            [LaunchCall::SettingsPage(SettingsPage::MicrophonePrivacy)]
        );
    }

    #[test]
    fn a_shell_failure_reaches_the_ui_as_internal_only() {
        let harness = testing::harness(
            registry::settings::defaults(),
            FakePrivacyConsent::granted(),
        );
        harness.launcher.fail_next(
            PortError::new(AppError::Internal).with_detail("ShellExecuteW failed with code 2"),
        );
        let result = poll_once(factory::run(
            &harness.ctx,
            &spec("app_open_logs_dir"),
            (),
            open_logs_dir,
        ));
        assert_eq!(result, Poll::Ready(Err(AppError::Internal)));
        assert!(harness.launcher.calls().is_empty());
    }
}
