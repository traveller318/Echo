/*!
 * SOURCE OF TRUTH KEYWORDS: system commands, appearance_get, app_open_logs_dir, app_open_mic_privacy_settings, app_open_page, app_about, AboutView, NavigationRequested, AppearanceView, logs folder, microphone privacy settings, memory use
 * WHAT:  The system command group (02 §4.3). `appearance_get` returns the AppearanceView (theme, transparency,
 *        backdrop) both windows paint from; `app_open_logs_dir` shows the local log folder and
 *        `app_open_mic_privacy_settings` opens the Windows microphone privacy page; `app_open_page` brings the main
 *        window forward and asks it (NavigationRequested) to show a page; `app_about` returns what Settings → About
 *        shows besides the engine and the models: the version, the build kind and Echo's memory use now.
 * WHY:   The UI reads appearance once through this command and then stays fresh from AppearanceChanged, never
 *        polling (02 §4.4); the view is computed on demand from the live settings and the SystemAppearance port,
 *        so there is no cached copy to drift. The two "open" commands are the targets of the `open_logs` and
 *        `open_mic_privacy` AppError actions (src/lib/app-error.ts), so every error toast action works; they go
 *        through the SystemLauncher port and the resolved AppPaths, so no handler names a Windows URI or a path.
 *        `app_open_page` serves surfaces without the main window's router (the pill's "Set up" and "Open"): the
 *        window is shown first, so the event reaches a live page; the page is a NavId, so only registry pages exist.
 *        About's memory is read at the call from the ProcessStats port; a failed read is logged and shown as
 *        unknown, never an error, because About must always open. The update commands join this group with their
 *        step.
 * WHERE: Registered through `ipc::commands::catalog`; called from the UI as `commands.appearanceGet()` by
 *        src/lib/appearance.ts and as `commands.appOpenLogsDir()` / `commands.appOpenMicPrivacySettings()` by the
 *        app shell's error actions (src/app/shell/use-app-error-action.ts), later by onboarding and About;
 *        `commands.appOpenPage({ page })` by the pill (src/pill), handled by the main window's shell.
 */

use crate::{
    ipc::{CommandCtx, factory::echo_command},
    pipeline::appearance,
    types::{
        AboutView, AppError, AppearanceView, NavigationRequested, OpenPageInput, PortError,
        SettingsPage,
    },
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

echo_command! {
    /// Brings the main window forward on a page (the pill's "Set up" and "Open").
    name: app_open_page,
    input: OpenPageInput,
    output: (),
    permission: None,
    reentrancy: Shared,
    handler: open_page,
}

echo_command! {
    /// Echo's version, whether this is a development build, and how much memory Echo uses right now.
    name: app_about,
    output: AboutView,
    permission: None,
    reentrancy: Shared,
    handler: about,
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

/// The About view: the build Tauri reported at startup and the memory Windows reports now (None when it cannot).
pub async fn about(ctx: &CommandCtx, (): ()) -> Result<AboutView, AppError> {
    let memory = ctx
        .process()
        .memory()
        .inspect_err(|error| tracing::warn!(detail = error.detail(), "memory use is unknown"))
        .ok();
    Ok(AboutView {
        app: ctx.app_info().clone(),
        memory,
    })
}

/// Shows the main window, then asks it to navigate to the page.
pub async fn open_page(ctx: &CommandCtx, input: OpenPageInput) -> Result<(), PortError> {
    ctx.main_window().show()?;
    ctx.emit(NavigationRequested { page: input.page });
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::task::Poll;

    use super::*;
    use crate::{
        ipc::{factory, testing},
        ports::fakes::{FakePrivacyConsent, LaunchCall, poll_once},
        registry,
        types::{
            AppEvent, AppInfo, Backdrop, ByteCount, CommandSpec, NavId, ProcessMemory, Reentrancy,
            ThemePreference, Transparency,
        },
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
    fn open_page_shows_the_main_window_then_asks_it_to_navigate() {
        let harness = testing::harness(
            registry::settings::defaults(),
            FakePrivacyConsent::granted(),
        );
        let input = OpenPageInput {
            page: NavId::Models,
        };
        let result = poll_once(factory::run(
            &harness.ctx,
            &spec("app_open_page"),
            input,
            open_page,
        ));
        assert_eq!(result, Poll::Ready(Ok(())));
        assert_eq!(harness.main_window.shows(), 1);
        assert_eq!(
            harness.events.events(),
            [AppEvent::NavigationRequested(NavigationRequested {
                page: NavId::Models
            })]
        );

        harness
            .main_window
            .fail_next(PortError::new(AppError::Internal).with_detail("no main window"));
        let failed = poll_once(factory::run(
            &harness.ctx,
            &spec("app_open_page"),
            input,
            open_page,
        ));
        assert_eq!(failed, Poll::Ready(Err(AppError::Internal)));
        assert_eq!(
            harness.events.events().len(),
            1,
            "no navigation without a window"
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

    #[test]
    fn about_reports_the_build_and_the_memory_in_use() {
        let harness = testing::harness(
            registry::settings::defaults(),
            FakePrivacyConsent::granted(),
        );
        let Poll::Ready(Ok(view)) =
            poll_once(factory::run(&harness.ctx, &spec("app_about"), (), about))
        else {
            panic!("app_about did not answer");
        };
        assert_eq!(
            view,
            AboutView {
                app: AppInfo {
                    version: String::from("0.1.0"),
                    development: false,
                },
                memory: Some(ProcessMemory {
                    working_set: ByteCount::new(testing::HARNESS_MEMORY.0),
                    private_bytes: ByteCount::new(testing::HARNESS_MEMORY.1),
                }),
            }
        );
    }
}
